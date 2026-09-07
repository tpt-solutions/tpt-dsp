//! `tpt-dsp-core` — the mathematical engine of the tpt-dsp framework.
//!
//! This crate provides pure, real-time-safe signal processing primitives:
//! complex-number arithmetic, FFT/DCT/Hilbert transforms, windowing
//! functions, biquad/FIR/IIR filters, convolution, FIR decimation, FM
//! demodulation, and lock-free ring buffers.
//!
//! # Real-time safety
//!
//! Every hot-path processing entry point operates on pre-allocated buffers
//! supplied by the caller. No heap allocation, lock or system call happens
//! inside the processing functions. The crate is `no_std` compatible: build
//! with `--no-default-features` for bare-metal (e.g. ARM Cortex-M) targets.
//!
//! # Features
//!
//! - `std` (default): enables the RustFFT-backed [`FftPlan`], crossbeam
//!   [`SpscQueue`] and the [`alloc`] feature.
//! - `alloc`: enables heap-backed convenience structs (owning FIR/IIR
//!   coefficient storage, [`HilbertTransformer`], [`FftConvolver`],
//!   [`FIRDecimator`]).
//!   All real-time *processing* stays allocation-free regardless.
//! - `simd` (**nightly only**): swaps [`crate::simd`] over to `core::simd`
//!   (portable SIMD) implementations of the complex helpers and the radix-2
//!   FFT butterfly. Off by default; the identical scalar API is always
//!   available, so enabling it never changes the public surface.
//!   `core::simd` is architecture-agnostic by construction — the same source
//!   compiles for x86/x86_64, ARM/AArch64 and (once tier support lands)
//!   RISC-V, with LLVM lowering to whatever vector ISA the target build
//!   enables. There is no runtime CPU-feature dispatch (`is_x86_feature_
//!   detected!`/`is_aarch64_feature_detected!`): wider ISAs (AVX2, AVX-512,
//!   SVE, ...) are a compile-time opt-in for the final binary (e.g.
//!   `-C target-cpu=native`), and this crate's `#![forbid(unsafe_code)]`
//!   rules out the usual unsafe target-feature-multiversioning pattern
//!   outright. See `ARCHITECTURE.md` §10 for the full rationale.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![cfg_attr(not(feature = "std"), no_std)]
// `#![feature(..)]` is only honoured in the crate root, so the `portable_simd`
// gate lives here rather than in `simd.rs`. It is enabled by the nightly-only
// `simd` feature *and* only when the toolchain actually supports it (the
// `tpt_portable_simd` cfg set by `build.rs`); a stable build with `simd` on
// falls back to the scalar module so `--all-features` keeps compiling.
#![cfg_attr(all(feature = "simd", tpt_portable_simd), feature(portable_simd))]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod bark;
mod complex;
mod convolution;
mod dct;
pub mod demod;
mod fft;
mod filters;
mod hilbert;
mod lpc;
#[cfg(feature = "alloc")]
mod lsp;
mod mdct;
mod noise_shaping;
mod pitch;
mod psychoacoustic;
#[cfg(feature = "alloc")]
pub mod resample;
mod vq;
mod ring;
mod windows;

// The vectorised module is only reachable on a toolchain that supports
// `portable_simd`; everywhere else (including stable builds with the `simd`
// feature enabled) we compile the identical scalar fallback.
#[cfg(all(feature = "simd", tpt_portable_simd))]
pub mod simd;

#[cfg(not(all(feature = "simd", tpt_portable_simd)))]
#[path = "simd_scalar.rs"]
pub mod simd;

#[cfg(feature = "std")]
mod plan;

#[cfg(feature = "std")]
mod spsc;

pub use bark::{
    aggregate_bands, bark_to_hz, erb_bandwidth, erb_rate_to_hz, fft_bin_bark_bands, hz_to_bark,
    hz_to_erb_rate,
};
pub use complex::{
    exp_i, magnitude, magnitude_squared, phase, rotate, Complex32, Complex64, C32, C64,
};
pub use convolution::convolve;
pub use dct::{dct_ii, dct_iii, dct_iv};
pub use demod::{phase_delta, phase_to_audio, FmDemodulator};
pub use fft::{
    fft, fft_inplace, fft_inplace_f32, ifft, ifft_inplace, is_power_of_two, next_power_of_two,
    twiddles,
};
pub use filters::{process_biquad, Biquad, BiquadCoeffs, BiquadType};
pub use hilbert::hilbert;
pub use lpc::{autocorrelation, levinson_durbin, predict, residual, synthesize};
pub use mdct::{imdct, mdct, sine_window as mdct_sine_window, MdctPlan};
pub use noise_shaping::{noise_shape_step, noise_shaper_is_stable};
pub use pitch::{
    amdf_pitch, autocorrelation_pitch, hz_range_to_period_samples, PitchEstimate,
    DEFAULT_VOICED_THRESHOLD, SPEECH_MAX_F0_HZ, SPEECH_MIN_F0_HZ,
};
pub use psychoacoustic::{
    classify_tonal_bins, combined_threshold_db, db_to_power, is_tonal_peak,
    noise_masking_offset_db, perceptual_weight_db, power_to_db, simultaneous_masking,
    spreading_function_db, threshold_in_quiet, tonal_masking_offset_db,
};
pub use ring::{RingBuffer, RingRead, RingWrite};
pub use simd::{complex_add_simd, complex_mul_simd, magnitude_simd};
pub use vq::{nearest_vector, nearest_vector_accelerated, vq_decode, vq_encode, DistanceMetric};
pub use windows::{windowed, WindowType};

#[cfg(feature = "alloc")]
pub use convolution::{ConvolvePlan, FftConvolver};
#[cfg(feature = "alloc")]
pub use vq::VqCodebook;
#[cfg(feature = "alloc")]
pub use filters::{Fir, FirDesign, IirCoeffs, IirFilter, IirStage};
#[cfg(feature = "alloc")]
pub use hilbert::HilbertTransformer;
#[cfg(feature = "alloc")]
pub use lpc::LpcAnalyzer;
#[cfg(feature = "alloc")]
pub use noise_shaping::NoiseShaper;
#[cfg(feature = "alloc")]
pub use lsp::{
    lpc_to_lsp, lsf_hz_to_lsp_rad, lsp_interpolate, lsp_is_stable, lsp_quantize_uniform,
    lsp_rad_to_lsf_hz, lsp_to_lpc, DEFAULT_GRID_POINTS,
};
#[cfg(feature = "alloc")]
pub use resample::FIRDecimator;

#[cfg(feature = "std")]
pub use dct::FastDctIvPlan;
#[cfg(feature = "std")]
pub use mdct::FastMdctPlan;
#[cfg(feature = "std")]
pub use plan::FftPlan;
#[cfg(feature = "std")]
pub use spsc::SpscQueue;
