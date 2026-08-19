//! `tpt-dsp-cli` — a small command-line DSP pipeline built on the tpt-dsp
//! crates.
//!
//! The library exposes the pieces the `tpt-dsp-cli` binary wires together so
//! they can be unit-tested in isolation:
//!
//! - WAV read/write helpers ([`read_wav`], [`write_wav`]).
//! - An effect chain ([`Effect`], [`parse_effect`], [`EffectChain`]) wrapping the
//!   `tpt-dsp-audio` effects (biquad, EQ, waveshaper, delay, convolution reverb).
//! - Raw IQ loading and FM demodulation ([`read_iq`], [`demod_iq`]).
//! - Spectrum / feature analysis for real and complex signals
//!   ([`analyze_real`], [`analyze_complex`]).
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::path::Path;

use anyhow::{bail, Context, Result};
use tpt_dsp_analysis::{find_peaks, linear_to_db, spectral_centroid, zero_crossing_rate, Averaging, RealtimeSpectrumAnalyzer, SpectrumConfig, DEFAULT_DB_FLOOR};
use tpt_dsp_audio::{ConvolutionReverb, Curve, Delay, Eq, Waveshaper};
use tpt_dsp_core::{
    windowed, Biquad, BiquadType, Complex32, FftPlan, FmDemodulator, FIRDecimator, WindowType,
};
use tpt_dsp_io::{parse_iq, IqFormat};

/// Decoded WAV audio: one `f32` sample buffer per channel, normalised to
/// `[-1, 1]`.
#[derive(Debug, Clone)]
pub struct WavData {
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Interleaved-by-channel sample buffers (channel-major order).
    pub channels: Vec<Vec<f32>>,
}

/// Read a WAV file, converting every channel to `f32` in `[-1, 1]`.
pub fn read_wav(path: &Path) -> Result<WavData> {
    let mut reader = hound::WavReader::open(path).with_context(|| format!("open wav `{}`", path.display()))?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let mut out: Vec<Vec<f32>> = vec![Vec::new(); channels];
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for (i, sample) in reader.samples::<f32>().flatten().enumerate() {
                out[i % channels].push(sample);
            }
        }
        hound::SampleFormat::Int => {
            let scale = ((1i64 << (spec.bits_per_sample - 1)) as f32).max(1.0);
            for (i, sample) in reader.samples::<i32>().flatten().enumerate() {
                out[i % channels].push(sample as f32 / scale);
            }
        }
    }
    let sample_rate = spec.sample_rate;
    Ok(WavData { sample_rate, channels })
}

/// Write `f32` WAV audio (32-bit float, channel-major interleaving).
pub fn write_wav(path: &Path, data: &WavData) -> Result<()> {
    let spec = hound::WavSpec {
        channels: data.channels.len() as u16,
        sample_rate: data.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).with_context(|| format!("create wav `{}`", path.display()))?;
    let frames = data.channels.iter().map(Vec::len).min().unwrap_or(0);
    for i in 0..frames {
        for channel in &data.channels {
            writer.write_sample(channel[i]).context("write wav sample")?;
        }
    }
    writer.finalize().context("finalize wav")?;
    Ok(())
}

/// A real-time effect applied by the CLI's filter subcommand.
///
/// Each variant maps onto one of the `tpt-dsp-audio` effects. Build a chain with
/// [`EffectChain::build`].
#[derive(Debug, Clone)]
pub enum Effect {
    /// A single RBJ biquad stage.
    Biquad {
        /// Response shape.
        kind: BiquadType,
        /// Corner / centre frequency in Hz.
        freq: f32,
        /// Filter Q (ignored by shelving filters).
        q: f32,
        /// Peak / shelf gain in dB (ignored by LP/HP/BP/notch/AP).
        gain: f32,
    },
    /// A waveshaping distortion stage.
    Waveshaper {
        /// Transfer curve.
        curve: Curve,
        /// Pre-gain drive amount (>= 0).
        drive: f32,
        /// Wet/dry mix, `0` = dry, `1` = wet.
        mix: f32,
    },
    /// A feedback delay line.
    Delay {
        /// Delay time in seconds.
        seconds: f32,
        /// Feedback gain in `[0, 0.99]`.
        feedback: f32,
        /// Wet/dry mix, `0` = dry, `1` = wet.
        mix: f32,
    },
    /// A convolution reverb with a synthesised decaying-noise impulse response.
    Reverb {
        /// Impulse-response decay time in seconds.
        decay: f32,
        /// Wet/dry mix, `0` = dry, `1` = wet.
        wet: f32,
    },
    /// A multi-band parametric EQ.
    Eq {
        /// `(center_hz, gain_db, q)` bands, applied in order.
        bands: Vec<(f32, f32, f32)>,
    },
}

impl Effect {
    /// Parse an [`Effect`] from a compact CLI spec.
    ///
    /// Recognised forms (fields separated by `:`):
    /// - `biquad:<type>:<freq>[:q[:gain_db]]` — `type` is one of `lowpass`,
    ///   `highpass`, `bandpass`, `notch`, `allpass`, `peaking`, `lowshelf`,
    ///   `highshelf`.
    /// - `waveshaper:<curve>:<drive>[:mix]` — `curve` is `tanh`, `hardclip`,
    ///   `cubic` or `poly`, where `poly` is followed by four coefficients
    ///   (`poly:c0,c1,c2,c3`).
    /// - `delay:<seconds>[:feedback[:mix]]`
    /// - `reverb:<seconds>[:wet]`
    /// - `eq:<f0>,<g0>,<q0>;[<f1>,<g1>,<q1>;…]`
    pub fn parse(spec: &str) -> Result<Self> {
        let (kind, rest) = spec.split_once(':').ok_or_else(|| anyhow::anyhow!("effect needs a `<kind>:` prefix: `{spec}`"))?;
        let fields: Vec<&str> = rest.split(':').collect();
        let num = |i: usize| -> Result<f32> {
            fields
                .get(i)
                .ok_or_else(|| anyhow::anyhow!("effect `{spec}` missing field {i}"))?
                .parse::<f32>()
                .with_context(|| format!("parse effect field `{spec}`"))
        };
        Ok(match kind {
            "biquad" => {
                let type_name = *fields.first().ok_or_else(|| anyhow::anyhow!("biquad needs a type"))?;
                let bt = parse_biquad_type(type_name)?;
                let freq = num(1)?;
                let q = fields.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.707);
                let gain = fields.get(3).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                Effect::Biquad { kind: bt, freq, q, gain }
            }
            "waveshaper" => {
                let curve = match *fields.first().ok_or_else(|| anyhow::anyhow!("waveshaper needs a curve"))? {
                    "tanh" => Curve::Tanh,
                    "hardclip" => Curve::HardClip,
                    "cubic" => Curve::Cubic,
                    "poly" => {
                        let c = fields
                            .get(1)
                            .ok_or_else(|| anyhow::anyhow!("poly needs 4 coefficients"))?
                            .split(',')
                            .map(|s| s.parse::<f32>())
                            .collect::<std::result::Result<Vec<f32>, _>>()
                            .context("parse poly coefficients")?;
                        if c.len() != 4 {
                            bail!("poly needs exactly 4 coefficients");
                        }
                        Curve::Polynomial([c[0], c[1], c[2], c[3]])
                    }
                    other => bail!("unknown waveshaper curve `{other}`"),
                };
                let drive = fields.get(2).and_then(|s| s.parse().ok()).unwrap_or(1.0);
                let mix = fields.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
                Effect::Waveshaper { curve, drive, mix }
            }
            "delay" => Effect::Delay {
                seconds: num(0)?,
                feedback: fields.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.3),
                mix: fields.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.5),
            },
            "reverb" => Effect::Reverb {
                decay: num(0)?,
                wet: fields.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.6),
            },
            "eq" => {
                let mut bands = Vec::new();
                for band in rest.split(';') {
                    if band.is_empty() {
                        continue;
                    }
                    let parts: Vec<f32> = band
                        .split(',')
                        .map(|s| s.parse::<f32>())
                        .collect::<std::result::Result<Vec<f32>, _>>()
                        .with_context(|| format!("parse eq band `{band}`"))?;
                    if parts.len() != 3 {
                        bail!("eq band needs `freq,gain,q`: `{band}`");
                    }
                    bands.push((parts[0], parts[1], parts[2]));
                }
                if bands.is_empty() {
                    bail!("eq needs at least one band");
                }
                Effect::Eq { bands }
            }
            other => bail!("unknown effect `{other}`"),
        })
    }
}

fn parse_biquad_type(name: &str) -> Result<BiquadType> {
    Ok(match name {
        "lowpass" => BiquadType::LowPass,
        "highpass" => BiquadType::HighPass,
        "bandpass" => BiquadType::BandPass,
        "notch" => BiquadType::Notch,
        "allpass" => BiquadType::AllPass,
        "peaking" => BiquadType::Peaking,
        "lowshelf" => BiquadType::LowShelf,
        "highshelf" => BiquadType::HighShelf,
        other => bail!("unknown biquad type `{other}`"),
    })
}

/// In-place signal processor used by [`EffectChain`].
trait Processor {
    /// Transform `buf` in place.
    fn process(&mut self, buf: &mut [f32]);
}

struct BiquadProc(Biquad<f32>);
impl Processor for BiquadProc {
    fn process(&mut self, buf: &mut [f32]) {
        let mut out = vec![0.0f32; buf.len()];
        self.0.process(buf, &mut out);
        buf.copy_from_slice(&out);
    }
}

struct EqProc(Eq);
impl Processor for EqProc {
    fn process(&mut self, buf: &mut [f32]) {
        self.0.process(buf);
    }
}

struct ShaperProc(Waveshaper);
impl Processor for ShaperProc {
    fn process(&mut self, buf: &mut [f32]) {
        self.0.process(buf);
    }
}

struct DelayProc(Delay);
impl Processor for DelayProc {
    fn process(&mut self, buf: &mut [f32]) {
        self.0.process(buf);
    }
}

struct ReverbProc(ConvolutionReverb);
impl Processor for ReverbProc {
    fn process(&mut self, buf: &mut [f32]) {
        let mut out = vec![0.0f32; buf.len()];
        self.0.process(buf, &mut out);
        buf.copy_from_slice(&out);
    }
}

/// A sequence of [`Processor`]s applied to a signal in order.
pub struct EffectChain {
    procs: Vec<Box<dyn Processor>>,
}

impl EffectChain {
    /// Build a chain from parsed [`Effect`] specs at `sample_rate` Hz.
    ///
    /// # Errors
    ///
    /// Returns an error if a filter frequency is outside `(0, sample_rate / 2]`
    /// or an effect parameter is invalid.
    pub fn build(sample_rate: f32, effects: &[Effect]) -> Result<Self> {
        let nyquist = sample_rate / 2.0;
        let mut procs: Vec<Box<dyn Processor>> = Vec::with_capacity(effects.len());
        for effect in effects {
            let boxed: Box<dyn Processor> = match effect {
                Effect::Biquad { kind, freq, q, gain } => {
                    if !(*freq > 0.0 && *freq <= nyquist) {
                        bail!("biquad frequency {freq} out of range (0, {nyquist}]");
                    }
                    Box::new(BiquadProc(Biquad::<f32>::design(*kind, sample_rate, *freq, *q, *gain)))
                }
                Effect::Waveshaper { curve, drive, mix } => {
                    Box::new(ShaperProc(Waveshaper::new(*curve, *drive, *mix)))
                }
                Effect::Delay { seconds, feedback, mix } => {
                    let max = ((*seconds * sample_rate).ceil() as usize).max(1) + 1;
                    let mut d = Delay::new(max);
                    d.set_delay_seconds(*seconds, sample_rate);
                    d.set_feedback(*feedback);
                    d.set_mix(*mix);
                    Box::new(DelayProc(d))
                }
                Effect::Reverb { decay, wet } => {
                    let ir = tpt_dsp_audio::generate_decay_ir(4096, sample_rate, *decay);
                    let mut r = ConvolutionReverb::new(&ir, 256);
                    r.set_wet(*wet);
                    Box::new(ReverbProc(r))
                }
                Effect::Eq { bands } => Box::new(EqProc(Eq::new(sample_rate, bands))),
            };
            procs.push(boxed);
        }
        Ok(Self { procs })
    }

    /// Apply the whole chain to one channel buffer in place.
    pub fn process_channel(&mut self, buf: &mut [f32]) {
        for proc in &mut self.procs {
            proc.process(buf);
        }
    }
}

/// Read a raw IQ byte file and parse it into [`Complex32`] samples.
pub fn read_iq(path: &Path, format: IqFormat) -> Result<Vec<Complex32>> {
    let bytes = std::fs::read(path).with_context(|| format!("read iq `{}`", path.display()))?;
    let bps = format.bytes_per_sample();
    if bytes.len() < bps {
        return Ok(Vec::new());
    }
    let mut out = vec![Complex32::default(); bytes.len() / bps];
    let parsed = parse_iq(format, &bytes, &mut out);
    out.truncate(parsed);
    Ok(out)
}

/// FM-demodulate a complex baseband stream into a mono `f32` signal.
///
/// `iq_rate` is the sample rate in Hz and `deviation` the FM deviation in Hz
/// (the discriminator scales a tone at `deviation` to `±1.0`). When `decimate`
/// is `Some(m)` with `m > 1`, an anti-alias FIR decimator reduces the output
/// rate to `iq_rate / m`.
pub fn demod_iq(iq: &[Complex32], iq_rate: f64, deviation: f64, decimate: Option<usize>) -> Vec<f32> {
    let mut demod = FmDemodulator::with_deviation(iq_rate as f32, deviation as f32);
    let mut audio = vec![0.0f32; iq.len()];
    demod.process(iq, &mut audio);
    if let Some(m) = decimate {
        if m > 1 {
            let mut dec = FIRDecimator::design_default(m, 63);
            let mut out = vec![0.0f32; dec.output_len(audio.len())];
            let written = dec.process(&audio, &mut out);
            out.truncate(written);
            audio = out;
        }
    }
    audio
}

/// A spectral / feature summary of a signal.
#[derive(Debug, Clone)]
pub struct SpectrumReport {
    /// Sample rate the analysis was performed at, in Hz.
    pub sample_rate: f64,
    /// Dominant (strongest) frequency in Hz.
    pub dominant_hz: f64,
    /// Peak level in dB (relative to full scale).
    pub peak_db: f64,
    /// RMS energy of the (real) signal.
    pub rms: f64,
    /// Zero-crossing rate of the (real) signal.
    pub zero_crossing_rate: f64,
    /// Spectral centroid in Hz.
    pub spectral_centroid_hz: f64,
    /// The `top` strongest peaks as `(bin, db)` pairs.
    pub top_peaks: Vec<(usize, f64)>,
    /// The full averaged spectrum as `(frequency_hz, magnitude_db)` pairs.
    pub spectrum: Vec<(f64, f64)>,
}

/// Analyse a real (WAV) signal: averaged one-sided magnitude spectrum plus
/// time-domain features.
pub fn analyze_real(samples: &[f32], sample_rate: f64, fft_size: usize, window: WindowType, top: usize) -> SpectrumReport {
    let sr = sample_rate as f32;
    let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size,
        sample_rate: sr,
        window,
        averaging: Averaging::Linear,
        ..SpectrumConfig::default()
    });
    for chunk in samples.chunks(fft_size) {
        let mut block = vec![0.0f32; fft_size];
        block[..chunk.len()].copy_from_slice(chunk);
        analyzer.process(&block);
    }
    let bin_width = analyzer.bin_width() as f64;
    let peak = analyzer.peak();
    let dom = peak.map(|p| (p.frequency as f64, p.magnitude_db as f64)).unwrap_or((0.0, DEFAULT_DB_FLOOR as f64));
    let centroid_bins = spectral_centroid(analyzer.magnitude()) as f64;
    let mut peaks: Vec<(usize, f64)> = find_peaks(analyzer.magnitude_db(), 0.1)
        .into_iter()
        .map(|b| (b, analyzer.magnitude_db()[b] as f64))
        .collect();
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    peaks.truncate(top);
    let spectrum = (0..analyzer.bins())
        .map(|b| (b as f64 * bin_width, analyzer.magnitude_db()[b] as f64))
        .collect();
    SpectrumReport {
        sample_rate,
        dominant_hz: dom.0,
        peak_db: dom.1,
        rms: tpt_dsp_analysis::rms(samples) as f64,
        zero_crossing_rate: zero_crossing_rate(samples) as f64,
        spectral_centroid_hz: centroid_bins * bin_width,
        top_peaks: peaks,
        spectrum,
    }
}

/// Analyse a complex (IQ) signal via its averaged two-sided magnitude spectrum.
pub fn analyze_complex(samples: &[Complex32], sample_rate: f64, fft_size: usize, window: WindowType, top: usize) -> SpectrumReport {
    let n = fft_size;
    let mut plan = FftPlan::new_forward(n);
    let mut avg = vec![0.0f64; n];
    let mut count = 0u64;
    let mut window_buf = vec![0.0f32; n];
    windowed(window, n, &mut window_buf);
    let mut buf = vec![Complex32::default(); n];
    for chunk in samples.chunks(n) {
        for (i, z) in chunk.iter().enumerate() {
            buf[i] = Complex32::new(z.re * window_buf[i], z.im * window_buf[i]);
        }
        for z in &mut buf[chunk.len()..] {
            *z = Complex32::default();
        }
        plan.process_inplace(&mut buf);
        for (acc, z) in avg.iter_mut().zip(buf.iter()) {
            *acc += z.norm() as f64;
        }
        count += 1;
    }
    if count > 0 {
        for acc in &mut avg {
            *acc /= count as f64;
        }
    }
    let bin_width = sample_rate / n as f64;
    let half = n / 2;
    let mut dom_bin = 1usize;
    let mut dom_val = 0.0f64;
    for k in 1..=half {
        if avg[k] > dom_val {
            dom_val = avg[k];
            dom_bin = k;
        }
    }
    let db_half: Vec<f64> = avg[..=half].iter().map(|&m| linear_to_db(m as f32, 1.0, DEFAULT_DB_FLOOR) as f64).collect();
    let centroid_bins = spectral_centroid(&db_half) as f64;
    let mut peaks: Vec<(usize, f64)> = find_peaks(&db_half, 0.1)
        .into_iter()
        .map(|b| (b, db_half[b]))
        .collect();
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    peaks.truncate(top);
    let spectrum = (0..=half).map(|b| (b as f64 * bin_width, db_half[b])).collect();
    let i_samples: Vec<f32> = samples.iter().map(|z| z.re).collect();
    SpectrumReport {
        sample_rate,
        dominant_hz: dom_bin as f64 * bin_width,
        peak_db: linear_to_db(dom_val as f32, 1.0, DEFAULT_DB_FLOOR) as f64,
        rms: tpt_dsp_analysis::rms(&i_samples) as f64,
        zero_crossing_rate: zero_crossing_rate(&i_samples) as f64,
        spectral_centroid_hz: centroid_bins * bin_width,
        top_peaks: peaks,
        spectrum,
    }
}

/// Write a `(frequency_hz, magnitude_db)` spectrum to a CSV file.
pub fn write_spectrum_csv(path: &Path, spectrum: &[(f64, f64)]) -> Result<()> {
    let mut file = std::fs::File::create(path).with_context(|| format!("create csv `{}`", path.display()))?;
    use std::io::Write;
    writeln!(file, "frequency_hz,magnitude_db").context("write csv header")?;
    for (freq, db) in spectrum {
        writeln!(file, "{freq},{db}").context("write csv row")?;
    }
    Ok(())
}

/// Apply an effect chain to every channel of a WAV file, writing the result.
pub fn filter_wav(input: &Path, output: &Path, effects: &[Effect]) -> Result<()> {
    let mut data = read_wav(input)?;
    if data.channels.is_empty() {
        bail!("wav `{}` has no channels", input.display());
    }
    let mut chain = EffectChain::build(data.sample_rate as f32, effects)?;
    for channel in &mut data.channels {
        chain.process_channel(channel);
    }
    write_wav(output, &data)
}

/// FM-demodulate a raw IQ file to a mono WAV file.
pub fn demod_file(input: &Path, output: &Path, format: IqFormat, iq_rate: f64, deviation: f64, decimate: Option<usize>) -> Result<()> {
    let iq = read_iq(input, format)?;
    if iq.is_empty() {
        bail!("no IQ samples parsed from `{}`", input.display());
    }
    let audio = demod_iq(&iq, iq_rate, deviation, decimate);
    let out_rate = if let Some(m) = decimate { iq_rate / m as f64 } else { iq_rate };
    let data = WavData {
        sample_rate: out_rate.round() as u32,
        channels: vec![audio],
    };
    write_wav(output, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_spec_parsing() {
        assert!(matches!(Effect::parse("biquad:lowpass:1000").unwrap(), Effect::Biquad { kind: BiquadType::LowPass, freq: 1000.0, .. }));
        assert!(matches!(Effect::parse("waveshaper:tanh:5:1").unwrap(), Effect::Waveshaper { curve: Curve::Tanh, .. }));
        assert!(matches!(Effect::parse("waveshaper:poly:1,2,3,4").unwrap(), Effect::Waveshaper { curve: Curve::Polynomial(_), .. }));
        match Effect::parse("eq:1000,6,1;2000,-3,1").unwrap() {
            Effect::Eq { bands } => assert_eq!(bands, vec![(1000.0, 6.0, 1.0), (2000.0, -3.0, 1.0)]),
            other => panic!("expected eq, got {other:?}"),
        }
        assert!(Effect::parse("bogus:1").is_err());
    }

    #[test]
    fn chain_lowpass_attenuates_a_wav() {
        let mut data = WavData {
            sample_rate: 48_000,
            channels: vec![(0..480).map(|i| (i as f32 * 0.4).sin()).collect()],
        };
        // 0.4 cycles/sample ≈ 9.6 kHz tone well above a 1 kHz low-pass.
        let mut chain = EffectChain::build(
            data.sample_rate as f32,
            &[Effect::Biquad { kind: BiquadType::LowPass, freq: 1000.0, q: 0.707, gain: 0.0 }],
        )
        .unwrap();
        let before = data.channels[0].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        chain.process_channel(&mut data.channels[0]);
        let after = data.channels[0].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(after < before, "low-pass should reduce a 9.6 kHz tone: {before} -> {after}");
    }

    #[test]
    fn analyze_real_finds_sine_peak() {
        let sr = 48_000.0f64;
        let samples: Vec<f32> = (0..1024).map(|i| (core::f32::consts::TAU * 1000.0 * i as f32 / 48000.0).sin()).collect();
        let report = analyze_real(&samples, sr, 1024, WindowType::Hann, 4);
        assert!((report.dominant_hz - 1000.0).abs() < 2.0, "dom {}", report.dominant_hz);
        assert!(report.peak_db > -3.0);
        assert!(report.spectrum.len() == 513);
    }

    #[test]
    fn demod_roundtrip_is_finite() {
        use tpt_dsp_core::exp_i;
        let iq: Vec<Complex32> = (0..2048).map(|i| exp_i(0.07 * i as f32)).collect();
        let audio = demod_iq(&iq, 2_400_000.0, 25_000.0, Some(10));
        assert!(!audio.is_empty());
        assert!(audio.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn wav_roundtrip_preserves_length() {
        let data = WavData {
            sample_rate: 44_100,
            channels: vec![(0..100).map(|i| (i as f32 * 0.01).sin()).collect()],
        };
        let dir = std::env::temp_dir();
        let path = dir.join("tpt_dsp_cli_test.wav");
        write_wav(&path, &data).unwrap();
        let back = read_wav(&path).unwrap();
        assert_eq!(back.channels[0].len(), 100);
        std::fs::remove_file(&path).ok();
    }
}
