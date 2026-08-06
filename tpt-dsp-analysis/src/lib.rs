//! `tpt-dsp-analysis` — spectrum analysis, time-series statistics and
//! feature extraction for telemetry, RF and biomedical streams.
//!
//! - Time series: [`MovingAverage`], [`RunningMean`], [`Ema`],
//!   [`OutlierDetector`] ([`timeseries`]).
//! - Features: [`rms`], [`zero_crossing_rate`], [`spectral_centroid`]
//!   ([`features`]).
//! - Spectrum: [`SpectrumAnalyzer`], [`find_peaks`], [`dominant_frequency`]
//!   ([`spectrum`]); full windowed-FFT analysis with time averaging and
//!   interpolated peaks via [`RealtimeSpectrumAnalyzer`]; waterfall
//!   generation via [`Spectrogram`] ([`spectrogram`]).
//! - Async: tokio / async-std / futures streaming adapters
//!   (`async_adapters`).
//!
//! # Features
//!
//! - `async` (default): shorthand for `async-tokio`.
//! - `async-tokio`: tokio channel adapters (`async_adapters::tokio`) plus the
//!   runtime-agnostic futures `Stream` / `Sink` adapters.
//! - `async-std`: the same adapters for async-std channels
//!   (`async_adapters::async_std`).
//!
//! Both runtime features may be enabled at once. With
//! `--no-default-features` no async runtime, executor or futures dependency
//! is compiled in.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod features;
mod spectrogram;
mod spectrum;
mod timeseries;

#[cfg(any(feature = "async-tokio", feature = "async-std"))]
pub mod async_adapters;

pub use features::{rms, spectral_centroid, spectral_centroid_normalized, zero_crossing_rate};
pub use spectrogram::Spectrogram;
pub use spectrum::{
    db_to_linear, dominant_frequency, find_peaks, linear_to_db, parabolic_interpolate, peak_bin,
    Averaging, RealtimeSpectrumAnalyzer, SpectralPeak, SpectrumAnalyzer, SpectrumConfig,
    DEFAULT_DB_FLOOR,
};
pub use timeseries::{Ema, MovingAverage, OutlierDetector, RunningMean};

#[cfg(any(feature = "async-tokio", feature = "async-std"))]
pub use async_adapters::{process_stream_in_place, process_stream_into_sink};

#[cfg(feature = "async-tokio")]
pub use async_adapters::tokio::{process_channel, process_channel_in_place, process_stream};
