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

mod complex;
mod convolution;
mod dct;
pub mod demod;
mod fft;
mod filters;
mod hilbert;
#[cfg(feature = "alloc")]
pub mod resample;
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
pub use ring::{RingBuffer, RingRead, RingWrite};
pub use simd::{complex_add_simd, complex_mul_simd, magnitude_simd};
pub use windows::{windowed, WindowType};

#[cfg(feature = "alloc")]
pub use convolution::{ConvolvePlan, FftConvolver};
#[cfg(feature = "alloc")]
pub use filters::{Fir, FirDesign, IirCoeffs, IirFilter, IirStage};
#[cfg(feature = "alloc")]
pub use hilbert::HilbertTransformer;
#[cfg(feature = "alloc")]
pub use resample::FIRDecimator;

#[cfg(feature = "std")]
pub use plan::FftPlan;
#[cfg(feature = "std")]
pub use spsc::SpscQueue;
