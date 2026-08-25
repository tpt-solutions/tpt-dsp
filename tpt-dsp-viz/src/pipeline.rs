//! Signal capture / generation and spectrum analysis pipeline.
//!
//! A producer thread streams [`SpectrumFrame`]s to the UI over a
//! [`crossbeam_channel::Sender`]. Each frame is one analysed block: the
//! one-sided dB magnitude spectrum plus the metadata the UI needs to label and
//! colour-map it.
//!
//! Two producers are provided: [`run_synthetic`] drives a deterministic
//! multi-tone + noise demo signal so the app runs with no hardware attached,
//! and [`run_audio_input`] (under the `audio` feature) captures from the
//! default system audio input device via `cpal`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossbeam_channel::Sender;
use tpt_dsp_analysis::{RealtimeSpectrumAnalyzer, SpectrumConfig};

/// Target display rate for the synthetic source, in frames per second.
const SYNTHETIC_FPS: u32 = 60;

/// One analysed spectrum frame delivered to the UI.
#[derive(Debug, Clone)]
pub struct SpectrumFrame {
    /// One-sided magnitude spectrum in dB (`fft_size / 2 + 1` bins).
    pub db: Vec<f32>,
    /// Sample rate in Hz the block was analysed at.
    pub sample_rate: f32,
    /// FFT length used for the transform.
    pub fft_size: usize,
    /// Lower dB clamp used when colour-mapping the frame.
    pub floor_db: f32,
    /// Upper dB clamp used when colour-mapping the frame.
    pub ceil_db: f32,
}

/// Analyse one block with `analyzer`, returning a [`SpectrumFrame`].
///
/// Allocation-free except for the per-frame `Vec<f32>` of dB values sent to the
/// UI — the same known tradeoff as the `tpt-dsp-analysis` async adapters, since
/// the UI consumes each frame once. `block.len()` must equal
/// `analyzer.fft_size()`.
pub fn analyze_block(analyzer: &mut RealtimeSpectrumAnalyzer, block: &[f32]) -> SpectrumFrame {
    analyzer.process(block);
    let cfg = analyzer.config();
    SpectrumFrame {
        db: analyzer.magnitude_db().to_vec(),
        sample_rate: cfg.sample_rate,
        fft_size: cfg.fft_size,
        floor_db: cfg.floor_db,
        // A full-scale sine (reference 1.0) reads 0 dB, so 0 dB is a natural top.
        ceil_db: 0.0,
    }
}

/// Deterministic multi-tone + noise demo signal generator.
///
/// The same seed always produces the same samples, so the visualizer can run
/// with no hardware attached and the output can be regression-tested for
/// determinism and bounded amplitude.
pub struct SyntheticGenerator {
    sample_rate: f32,
    fft_size: usize,
    tones: Vec<Tone>,
    noise_amplitude: f32,
    lcg_state: u64,
    sample_index: u64,
}

#[derive(Debug, Clone, Copy)]
struct Tone {
    frequency: f32,
    amplitude: f32,
}

impl SyntheticGenerator {
    /// Create a generator at `sample_rate` producing `fft_size`-sample blocks.
    pub fn new(sample_rate: f32, fft_size: usize) -> Self {
        let tones = vec![
            Tone {
                frequency: sample_rate * 0.07,
                amplitude: 0.60,
            },
            Tone {
                frequency: sample_rate * 0.21,
                amplitude: 0.35,
            },
            Tone {
                frequency: sample_rate * 0.43,
                amplitude: 0.18,
            },
        ];
        Self {
            sample_rate,
            fft_size,
            tones,
            noise_amplitude: 0.02,
            lcg_state: 0x9E37_79B9_7F4A_7C15,
            sample_index: 0,
        }
    }

    /// Produce the next block of `fft_size` samples.
    ///
    /// Each sample is the sum of the deterministic tones plus bounded, seeded
    /// noise, so the whole block is reproducible across runs.
    pub fn next_block(&mut self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.fft_size);
        for _ in 0..self.fft_size {
            let n = self.sample_index as f32;
            let mut s = 0.0f32;
            for tone in &self.tones {
                let phase = core::f32::consts::TAU * tone.frequency * n / self.sample_rate;
                s += tone.amplitude * phase.sin();
            }
            s += self.noise_amplitude * self.next_noise();
            out.push(s);
            self.sample_index += 1;
        }
        out
    }

    /// Bounded amplitude of the generated signal: the tone sum plus noise peak.
    pub fn max_amplitude(&self) -> f32 {
        let tone_sum: f32 = self.tones.iter().map(|t| t.amplitude).sum();
        tone_sum + self.noise_amplitude
    }

    fn next_noise(&mut self) -> f32 {
        // Deterministic 64-bit LCG; high bits feed a uniform value in [-1, 1).
        self.lcg_state = self
            .lcg_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let bits = (self.lcg_state >> 32) as u32;
        (bits as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Run the synthetic generator on a paced background thread, analysing each
/// block and sending the resulting frame until `stop` is set.
///
/// Frames are paced to [`SYNTHETIC_FPS`] so the waterfall scrolls at a steady,
/// watchable rate regardless of the block duration.
pub fn run_synthetic(config: SpectrumConfig, sender: Sender<SpectrumFrame>, stop: Arc<AtomicBool>) {
    let mut analyzer = RealtimeSpectrumAnalyzer::new(config);
    let mut gen = SyntheticGenerator::new(config.sample_rate, config.fft_size);
    let block_duration = config.fft_size as f64 / config.sample_rate as f64;
    let frame_interval = 1.0 / SYNTHETIC_FPS as f64;
    while !stop.load(Ordering::SeqCst) {
        let block = gen.next_block();
        let frame = analyze_block(&mut analyzer, &block);
        if sender.send(frame).is_err() {
            break; // receiver dropped (window closed)
        }
        let sleep = frame_interval - block_duration;
        if sleep > 0.0 {
            thread::sleep(Duration::from_secs_f64(sleep));
        }
    }
}

/// Live audio capture state shared with the `cpal` input callback.
#[cfg(feature = "audio")]
struct AudioCapture {
    analyzer: RealtimeSpectrumAnalyzer,
    accumulator: Vec<f32>,
    sender: Sender<SpectrumFrame>,
}

#[cfg(feature = "audio")]
impl AudioCapture {
    /// Downmix one interleaved, multi-channel chunk to mono and analyse full
    /// `fft_size` windows as they accumulate.
    fn ingest_interleaved(&mut self, samples: impl Iterator<Item = f32>, channels: usize) {
        let n = self.analyzer.fft_size();
        let frames: Vec<f32> = samples.collect();
        for frame in frames.chunks(channels) {
            let mono: f32 = frame.iter().sum::<f32>() / channels as f32;
            self.accumulator.push(mono);
        }
        while self.accumulator.len() >= n {
            let frame = analyze_block(&mut self.analyzer, &self.accumulator[..n]);
            self.accumulator.drain(..n);
            if self.sender.send(frame).is_err() {
                return; // receiver dropped
            }
        }
    }
}

/// Run live capture from the default system audio input device on a background
/// thread, analysing windows and sending frames until `stop` is set.
///
/// Enabled by the `audio` feature. Any native input format is converted to
/// `f32` by the built-in backend; multi-channel streams are downmixed to mono
/// by averaging.
#[cfg(feature = "audio")]
pub fn run_audio_input(
    config: SpectrumConfig,
    sender: Sender<SpectrumFrame>,
    stop: Arc<AtomicBool>,
) {
    use std::sync::mpsc;

    if !tpt_dsp_io::has_default_input() {
        eprintln!("tpt-dsp-viz: no default audio input device; falling back to synthetic");
        run_synthetic(config, sender, stop);
        return;
    }

    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    {
        let stop_flag = Arc::clone(&stop);
        thread::spawn(move || {
            while !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(50));
            }
            let _ = stop_tx.send(());
        });
    }

    let capture = AudioCapture {
        analyzer: RealtimeSpectrumAnalyzer::new(SpectrumConfig {
            sample_rate: config.sample_rate,
            ..config
        }),
        accumulator: Vec::new(),
        sender,
    };
    let state = Arc::new(std::sync::Mutex::new(capture));
    let callback_state = Arc::clone(&state);
    let result = tpt_dsp_io::run_input(
        move |interleaved: &[f32], channels: usize| {
            callback_state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .ingest_interleaved(interleaved.iter().copied(), channels);
        },
        &stop_rx,
    );
    if let Err(e) = result {
        eprintln!("tpt-dsp-viz: audio input failed ({e}); stopping capture");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tpt_dsp_analysis::{peak_bin, Averaging};

    #[test]
    fn analyze_block_finds_sine_peak() {
        let n = 1024;
        let sr = 48_000.0;
        let cfg = SpectrumConfig {
            fft_size: n,
            sample_rate: sr,
            averaging: Averaging::None,
            ..SpectrumConfig::default()
        };
        let mut analyzer = RealtimeSpectrumAnalyzer::new(cfg);
        let bin = 64.0;
        let block: Vec<f32> = (0..n)
            .map(|i| (core::f32::consts::TAU * bin * i as f32 / n as f32).sin())
            .collect();
        let frame = analyze_block(&mut analyzer, &block);

        assert_eq!(frame.db.len(), n / 2 + 1);
        let peak = peak_bin(&frame.db);
        assert_eq!(peak, 64);
        let freq = peak as f32 * frame.sample_rate / frame.fft_size as f32;
        assert!((freq - 3000.0).abs() < 1.0, "peak at {freq} Hz");
        assert!(frame.floor_db < frame.ceil_db);
    }

    #[test]
    fn synthetic_is_deterministic() {
        let mut a = SyntheticGenerator::new(48_000.0, 1024);
        let mut b = SyntheticGenerator::new(48_000.0, 1024);
        assert_eq!(a.next_block(), b.next_block());
        assert_eq!(a.next_block(), b.next_block());
        assert_eq!(a.next_block(), b.next_block());
    }

    #[test]
    fn synthetic_is_bounded() {
        let mut g = SyntheticGenerator::new(48_000.0, 1024);
        let limit = g.max_amplitude() + 1e-4;
        for _ in 0..20 {
            for &s in &g.next_block() {
                assert!(s.abs() <= limit, "sample {s} exceeds bound {limit}");
            }
        }
    }

    #[test]
    fn synthetic_block_is_correct_length() {
        let mut g = SyntheticGenerator::new(44_100.0, 512);
        assert_eq!(g.next_block().len(), 512);
    }
}
