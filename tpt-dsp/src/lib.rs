//! `tpt-dsp` — one dependency for the whole framework.
//!
//! This facade re-exports the workspace crates behind feature flags and
//! provides a [`prelude`] with the types most programs use.
//!
//! ```toml
//! [dependencies]
//! tpt-dsp = { version = "0.2", features = ["audio", "analysis"] }
//! ```
//!
//! | Feature    | Adds                                  | Default |
//! |------------|---------------------------------------|---------|
//! | `audio`    | synthesis, effects, graphs, engine    | yes     |
//! | `analysis` | spectra, features, STFT, loudness     | yes     |
//! | `control`  | PID, input shaping, kinematics        | yes     |
//! | `io`       | WAV, IQ streams, native audio I/O     | no      |
//!
//! The core crate is always included. It is `no_std`, so depending on the
//! facade with `default-features = false` keeps that property.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

/// Core math, filters, transforms and windows. Always available.
pub use tpt_dsp_core as core;

/// Synthesis, effects, dynamics and audio graphs.
#[cfg(feature = "audio")]
pub use tpt_dsp_audio as audio;

/// Spectral analysis, features, loudness and pitch.
#[cfg(feature = "analysis")]
pub use tpt_dsp_analysis as analysis;

/// PID, input shaping and kinematics.
#[cfg(feature = "control")]
pub use tpt_dsp_control as control;

/// WAV, IQ and native audio I/O.
#[cfg(feature = "io")]
pub use tpt_dsp_io as io;

/// The types most programs need, in one import.
///
/// ```
/// use tpt_dsp::prelude::*;
///
/// let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
/// let mut block = [0.5f32; 64];
/// lp.process(&block.clone(), &mut block);
/// ```
pub mod prelude {
    pub use tpt_dsp_core::{
        Biquad, BiquadCoeffs, BiquadType, Chain, FilterDesignError, FmDemodulator, Nco, Pll,
        Processor, Smoothed, WindowType, C32,
    };

    #[cfg(feature = "audio")]
    pub use tpt_dsp_audio::{
        Adsr, Compressor, Delay, Eq, Freeverb, Limiter, Oscillator, Oversampler,
        PolyBlepOscillator, RealtimeEngine, Waveform, Waveshaper,
    };

    #[cfg(feature = "analysis")]
    pub use tpt_dsp_analysis::{
        yin_pitch, LoudnessMeter, Mfcc, RealtimeSpectrumAnalyzer, SpectrumConfig,
    };

    #[cfg(feature = "control")]
    pub use tpt_dsp_control::{InputShaper, Pid, TrapezoidalProfile};
}

#[cfg(test)]
mod tests {
    #[test]
    fn core_is_always_reachable() {
        let mut lp = crate::core::Biquad::<f32>::design(
            crate::core::BiquadType::LowPass,
            48_000.0,
            1_000.0,
            0.707,
            0.0,
        );
        assert!(lp.tick(0.0).abs() < 1e-9);
    }

    #[cfg(feature = "audio")]
    #[test]
    fn audio_is_reachable() {
        let mut osc = crate::audio::Oscillator::new(48_000.0, 440.0);
        assert!(osc.tick().is_finite());
    }

    #[cfg(feature = "control")]
    #[test]
    fn prelude_pid_works() {
        use crate::prelude::*;
        let mut pid = Pid::new(1.0, 0.0, 0.0, 0.01);
        pid.set_setpoint(1.0);
        assert!((pid.update(0.0) - 1.0).abs() < 1e-6);
    }
}

/// A compiled end-to-end example (WAV in, filter, analyse, WAV out). Only
/// documentation; it needs the `audio`, `analysis` and `io` features.
#[cfg(all(feature = "audio", feature = "analysis", feature = "io"))]
pub mod walkthrough;
