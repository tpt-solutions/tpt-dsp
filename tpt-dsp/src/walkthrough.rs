//! A compiled walkthrough: WAV in, filter, analyse, WAV out.
//!
//! Each step uses only the public API of the facade, so this example is
//! compiled and run by `cargo test --all-features`. It follows the same path
//! as the `tpt-dsp-cli filter` and `spectrum` subcommands.
//!
//! ```
//! use tpt_dsp::analysis::{rms, yin_pitch};
//! use tpt_dsp::audio::{Oscillator, Waveform};
//! use tpt_dsp::core::{Biquad, BiquadType};
//! use tpt_dsp::io::{read_wav_f32_path, write_wav_f32_path, SampleFormat, WavSpec};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let dir = std::env::temp_dir();
//! let input = dir.join("tpt_dsp_walkthrough_in.wav");
//! let output = dir.join("tpt_dsp_walkthrough_out.wav");
//!
//! // 1. Make a test signal: a 220 Hz tone with a 3 kHz hiss on top.
//! let rate = 16_000u32;
//! let mut osc = Oscillator::with_waveform(rate as f32, 220.0, Waveform::Sine);
//! let mut hiss = Oscillator::with_waveform(rate as f32, 3_000.0, Waveform::Sawtooth);
//! let samples: Vec<f32> = (0..rate * 2)
//!     .map(|_| 0.4 * osc.tick() + 0.2 * hiss.tick())
//!     .collect();
//! let spec = WavSpec {
//!     channels: 1,
//!     sample_rate: rate,
//!     bits_per_sample: 32,
//!     sample_format: SampleFormat::Float,
//! };
//! write_wav_f32_path(&input, &spec, &samples)?;
//!
//! // 2. Read it back and low-pass it at 1 kHz.
//! let data = read_wav_f32_path(&input)?;
//! let mut lp = Biquad::<f32>::design(BiquadType::LowPass, rate as f32, 1_000.0, 0.707, 0.0);
//! let filtered: Vec<f32> = data.interleaved.iter().map(|&x| lp.tick(x)).collect();
//!
//! // 3. Analyse: the fundamental survives, and the hiss is mostly gone.
//! let frame = &filtered[8_000..8_000 + 2048];
//! let f0 = yin_pitch(frame, rate as f32, 80.0, 800.0, 0.15).expect("voiced");
//! assert!((f0 - 220.0).abs() < 3.0, "pitch {f0}");
//! assert!(rms(&filtered[8_000..]) < rms(&samples[8_000..]));
//!
//! // 4. Write the result.
//! write_wav_f32_path(&output, &spec, &filtered)?;
//! # std::fs::remove_file(&input).ok();
//! # std::fs::remove_file(&output).ok();
//! # Ok(())
//! # }
//! ```
