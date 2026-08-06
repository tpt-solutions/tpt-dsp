//! `tpt-dsp-analysis` — spectrum analysis, time-series statistics and
//! feature extraction for telemetry, RF and biomedical streams.
//!
//! - Time series: [`MovingAverage`], [`RunningMean`], [`Ema`],
//!   [`OutlierDetector`] ([`timeseries`]).
//! - Features: [`rms`], [`zero_crossing_rate`], [`spectral_centroid`]
//!   ([`features`]).
//! - Spectrum: [`SpectrumAnalyzer`], [`find_peaks`], [`dominant_frequency`]
//!   ([`spectrum`]); waterfall generation via [`Spectrogram`] ([`spectrogram`]).
//! - Async: tokio / futures streaming adapters ([`async_adapters`], behind
//!   the default `async` feature).
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod async_adapters;
mod features;
mod spectrogram;
mod spectrum;
mod timeseries;

pub use async_adapters::{process_channel, process_stream};
pub use features::{rms, spectral_centroid, spectral_centroid_normalized, zero_crossing_rate};
pub use spectrogram::Spectrogram;
pub use spectrum::{dominant_frequency, find_peaks, peak_bin, SpectrumAnalyzer};
pub use timeseries::{Ema, MovingAverage, OutlierDetector, RunningMean};
