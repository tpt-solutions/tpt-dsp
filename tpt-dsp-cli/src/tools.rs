//! Library side of the `gen`, `resample`, `design` and `response` subcommands.
//!
//! Each function returns data (a [`WavData`] or a `(frequency_hz, dB)` list)
//! so it can be tested without touching the filesystem. The binary writes the
//! results out.

use anyhow::{bail, Result};
use tpt_dsp_audio::{BlepShape, NoiseSource, Oscillator, PolyBlepOscillator, Waveform};
use tpt_dsp_core::{
    butterworth, cascade_magnitude_response, chebyshev1_lowpass, chebyshev2_lowpass, FftPlan,
    RationalResampler, Response, C32,
};

use crate::{Effect, EffectChain, WavData};

/// Waveforms `gen` can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenWaveform {
    /// Pure sine at `freq_hz`.
    Sine,
    /// Band-limited sawtooth (PolyBLEP).
    Saw,
    /// Band-limited 50% square (PolyBLEP).
    Square,
    /// Deterministic white noise (`freq_hz` is ignored).
    Noise,
    /// Linear sweep from 20 Hz up to `freq_hz`.
    Chirp,
}

/// Parameters for [`generate`].
#[derive(Debug, Clone, Copy)]
pub struct GenSpec {
    /// Which signal to produce.
    pub waveform: GenWaveform,
    /// Frequency (or sweep end) in Hz.
    pub freq_hz: f32,
    /// Duration in seconds.
    pub seconds: f32,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Peak amplitude in `[0, 1]`.
    pub amplitude: f32,
    /// Number of identical channels to write.
    pub channels: u16,
}

/// Generate a test signal.
///
/// # Errors
/// Returns an error for a non-positive duration or sample rate, a frequency
/// outside `(0, sample_rate / 2]` (except for noise), an amplitude outside
/// `[0, 1]`, or zero channels.
pub fn generate(spec: &GenSpec) -> Result<WavData> {
    if !(spec.seconds.is_finite() && spec.seconds > 0.0) {
        bail!("duration must be a positive number of seconds");
    }
    if spec.sample_rate == 0 {
        bail!("sample rate must be positive");
    }
    let sr = spec.sample_rate as f32;
    let nyquist = sr / 2.0;
    if spec.waveform != GenWaveform::Noise && !(spec.freq_hz > 0.0 && spec.freq_hz <= nyquist) {
        bail!("frequency {} Hz outside (0, {nyquist}]", spec.freq_hz);
    }
    if !(0.0..=1.0).contains(&spec.amplitude) {
        bail!("amplitude must be in [0, 1]");
    }
    if spec.channels == 0 {
        bail!("need at least one channel");
    }
    let n = (spec.seconds * sr).round() as usize;
    let mut one = vec![0.0f32; n];
    match spec.waveform {
        GenWaveform::Sine => {
            let mut osc = Oscillator::with_waveform(sr, spec.freq_hz, Waveform::Sine);
            for s in one.iter_mut() {
                *s = osc.tick();
            }
        }
        GenWaveform::Saw | GenWaveform::Square => {
            let shape = if spec.waveform == GenWaveform::Saw {
                BlepShape::Sawtooth
            } else {
                BlepShape::Square
            };
            let mut osc = PolyBlepOscillator::new(sr, spec.freq_hz, shape);
            osc.process(&mut one);
        }
        GenWaveform::Noise => {
            let mut noise = NoiseSource::new(0x1234_5678);
            for s in one.iter_mut() {
                *s = noise.next_uniform();
            }
        }
        GenWaveform::Chirp => {
            // Integrate the instantaneous frequency so the sweep is continuous.
            let f0 = 20.0f32.min(spec.freq_hz);
            let tau = core::f32::consts::TAU;
            let mut phase = 0.0f32;
            for (i, s) in one.iter_mut().enumerate() {
                let t = i as f32 / sr;
                let f = f0 + (spec.freq_hz - f0) * t / spec.seconds;
                *s = phase.sin();
                phase = (phase + tau * f / sr) % tau;
            }
        }
    }
    let scaled: Vec<f32> = one.iter().map(|x| x * spec.amplitude).collect();
    Ok(WavData {
        sample_rate: spec.sample_rate,
        channels: (0..spec.channels).map(|_| scaled.clone()).collect(),
    })
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Resample every channel to `to_rate` Hz with a polyphase rational resampler.
///
/// The ratio is reduced by its GCD first, so 48 000 → 44 100 becomes 147/160.
///
/// # Errors
/// Returns an error if either rate is zero.
pub fn resample(data: &WavData, to_rate: u32) -> Result<WavData> {
    if data.sample_rate == 0 || to_rate == 0 {
        bail!("sample rates must be positive");
    }
    if to_rate == data.sample_rate {
        return Ok(data.clone());
    }
    let g = gcd(data.sample_rate, to_rate);
    let up = (to_rate / g) as usize;
    let down = (data.sample_rate / g) as usize;
    let channels = data
        .channels
        .iter()
        .map(|c| RationalResampler::new(up, down, 16).resample(c))
        .collect();
    Ok(WavData {
        sample_rate: to_rate,
        channels,
    })
}

/// Which prototype filter `design` produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesignKind {
    /// Butterworth, maximally flat.
    Butterworth,
    /// Chebyshev type I (passband ripple); needs `ripple_db`.
    Chebyshev1,
    /// Chebyshev type II (stopband ripple); needs `atten_db` and uses `cutoff_hz`
    /// as the stopband edge.
    Chebyshev2,
}

/// Parameters for [`design_response`].
#[derive(Debug, Clone, Copy)]
pub struct DesignSpec {
    /// Prototype family.
    pub kind: DesignKind,
    /// Filter order (≥ 1).
    pub order: usize,
    /// Sample rate in Hz.
    pub sample_rate: f32,
    /// Cutoff (or stopband edge for Chebyshev II) in Hz.
    pub cutoff_hz: f32,
    /// Passband ripple in dB (Chebyshev I).
    pub ripple_db: f32,
    /// Minimum stopband attenuation in dB (Chebyshev II).
    pub atten_db: f32,
    /// Number of frequency points from DC to Nyquist (≥ 2).
    pub points: usize,
}

/// Magnitude response of a designed low-pass as `(frequency_hz, dB)` pairs from
/// DC to Nyquist.
///
/// # Errors
/// Returns an error if the design parameters are rejected by the filter design.
pub fn design_response(spec: &DesignSpec) -> Result<Vec<(f64, f64)>> {
    let sections = match spec.kind {
        DesignKind::Butterworth => butterworth(
            Response::LowPass,
            spec.order,
            spec.sample_rate,
            spec.cutoff_hz,
        ),
        DesignKind::Chebyshev1 => {
            chebyshev1_lowpass(spec.order, spec.sample_rate, spec.cutoff_hz, spec.ripple_db)
        }
        DesignKind::Chebyshev2 => {
            chebyshev2_lowpass(spec.order, spec.sample_rate, spec.cutoff_hz, spec.atten_db)
        }
    }
    .map_err(|e| anyhow::anyhow!("design rejected: {e}"))?;
    let points = spec.points.max(2);
    let nyquist = f64::from(spec.sample_rate) / 2.0;
    Ok((0..points)
        .map(|i| {
            let f = nyquist * i as f64 / (points - 1) as f64;
            let mag = cascade_magnitude_response(&sections, spec.sample_rate, f as f32);
            (f, 20.0 * f64::from(mag.max(1e-12)).log10())
        })
        .collect())
}

/// Frequency response of an effect chain, measured from its impulse response.
///
/// The impulse is fed through a freshly built [`EffectChain`] and transformed
/// over `fft_size` samples (8192 is a good default). Long tails, such as
/// reverb, are truncated to that window.
///
/// # Errors
/// Returns an error if the effects cannot be built at `sample_rate`.
pub fn effect_response(
    effects: &[Effect],
    sample_rate: f32,
    fft_size: usize,
) -> Result<Vec<(f64, f64)>> {
    if fft_size < 16 || !fft_size.is_power_of_two() {
        bail!("fft size must be a power of two of at least 16");
    }
    let mut chain = EffectChain::build(sample_rate, effects)?;
    let mut impulse = vec![0.0f32; fft_size];
    impulse[0] = 1.0;
    chain.process_channel(&mut impulse);

    let mut buf: Vec<C32> = impulse.iter().map(|&x| C32::new(x, 0.0)).collect();
    let mut plan = FftPlan::new_forward(fft_size);
    plan.process_inplace(&mut buf);
    let bins = fft_size / 2 + 1;
    Ok((0..bins)
        .map(|k| {
            let f = f64::from(sample_rate) * k as f64 / fft_size as f64;
            let db = 20.0 * f64::from(buf[k].norm().max(1e-12)).log10();
            (f, db)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_produces_the_requested_length_and_channels() {
        let d = generate(&GenSpec {
            waveform: GenWaveform::Sine,
            freq_hz: 440.0,
            seconds: 0.5,
            sample_rate: 8000,
            amplitude: 0.5,
            channels: 2,
        })
        .unwrap();
        assert_eq!(d.channels.len(), 2);
        assert_eq!(d.channels[0].len(), 4000);
        let peak = d.channels[0].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.5).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn generate_rejects_bad_arguments() {
        let base = GenSpec {
            waveform: GenWaveform::Sine,
            freq_hz: 440.0,
            seconds: 1.0,
            sample_rate: 8000,
            amplitude: 0.5,
            channels: 1,
        };
        assert!(generate(&GenSpec {
            freq_hz: 5000.0,
            ..base
        })
        .is_err());
        assert!(generate(&GenSpec {
            seconds: 0.0,
            ..base
        })
        .is_err());
        assert!(generate(&GenSpec {
            amplitude: 2.0,
            ..base
        })
        .is_err());
        assert!(generate(&GenSpec {
            channels: 0,
            ..base
        })
        .is_err());
    }

    #[test]
    fn chirp_and_noise_are_bounded() {
        for w in [GenWaveform::Chirp, GenWaveform::Noise] {
            let d = generate(&GenSpec {
                waveform: w,
                freq_hz: 2000.0,
                seconds: 0.2,
                sample_rate: 8000,
                amplitude: 1.0,
                channels: 1,
            })
            .unwrap();
            assert!(d.channels[0].iter().all(|v| v.abs() <= 1.0 + 1e-6));
        }
    }

    #[test]
    fn resample_changes_rate_and_length() {
        let d = generate(&GenSpec {
            waveform: GenWaveform::Sine,
            freq_hz: 500.0,
            seconds: 1.0,
            sample_rate: 8000,
            amplitude: 0.5,
            channels: 1,
        })
        .unwrap();
        let r = resample(&d, 12_000).unwrap();
        assert_eq!(r.sample_rate, 12_000);
        assert!((r.channels[0].len() as i64 - 12_000).abs() <= 2);
    }

    #[test]
    fn resample_to_same_rate_is_identity() {
        let d = WavData {
            sample_rate: 8000,
            channels: vec![vec![0.1, 0.2, 0.3]],
        };
        let r = resample(&d, 8000).unwrap();
        assert_eq!(r.channels, d.channels);
    }

    #[test]
    fn butterworth_design_is_minus_3db_at_cutoff() {
        let resp = design_response(&DesignSpec {
            kind: DesignKind::Butterworth,
            order: 4,
            sample_rate: 48_000.0,
            cutoff_hz: 1000.0,
            ripple_db: 0.0,
            atten_db: 0.0,
            points: 513,
        })
        .unwrap();
        let (f, db) = resp
            .iter()
            .min_by(|a, b| (a.0 - 1000.0).abs().total_cmp(&(b.0 - 1000.0).abs()))
            .copied()
            .unwrap();
        assert!((f - 1000.0).abs() < 100.0);
        assert!((db + 3.0).abs() < 0.5, "{db} dB near {f} Hz");
        assert!(resp[0].1.abs() < 0.01, "dc {}", resp[0].1);
    }

    #[test]
    fn effect_response_of_a_lowpass_attenuates_high_frequencies() {
        let effects = vec![Effect::parse("biquad:lowpass:1000").unwrap()];
        let resp = effect_response(&effects, 48_000.0, 8192).unwrap();
        let at = |hz: f64| {
            resp.iter()
                .min_by(|a, b| (a.0 - hz).abs().total_cmp(&(b.0 - hz).abs()))
                .unwrap()
                .1
        };
        assert!(at(200.0) > -1.0, "passband {}", at(200.0));
        assert!(at(8000.0) < -20.0, "stopband {}", at(8000.0));
    }
}
