//! `tpt-dsp-core` — the mathematical engine of the tpt-dsp framework.
//!
//! This crate provides pure, real-time-safe signal processing primitives:
//! complex-number arithmetic, FFT/DCT/Hilbert transforms, windowing
//! functions, biquad/FIR/IIR filters, convolution, and lock-free ring
//! buffers.
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
//!   coefficient storage, [`HilbertTransformer`], [`FftConvolver`]).
//!   All real-time *processing* stays allocation-free regardless.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

#[cfg(feature = "alloc")]
extern crate alloc;

mod complex;
mod convolution;
mod dct;
mod fft;
mod filters;
mod hilbert;
mod ring;
mod windows;

#[cfg(feature = "std")]
mod plan;

#[cfg(feature = "std")]
mod spsc;

pub use complex::{exp_i, magnitude, magnitude_squared, phase, rotate, C32, C64, Complex32, Complex64};
pub use convolution::{convolve, ConvolvePlan};
pub use dct::{dct_ii, dct_iii, dct_iv};
pub use fft::{fft, fft_inplace, ifft, ifft_inplace, is_power_of_two, next_power_of_two, twiddles};
pub use filters::{
    process_biquad, Biquad, BiquadCoeffs, BiquadType, IirStage,
};
pub use hilbert::hilbert;
pub use ring::{RingBuffer, RingRead, RingWrite};
pub use windows::{windowed, WindowType};

#[cfg(feature = "alloc")]
pub use filters::{Fir, FirDesign, IirFilter, IirCoeffs};
#[cfg(feature = "alloc")]
pub use hilbert::HilbertTransformer;
#[cfg(feature = "alloc")]
pub use convolution::FftConvolver;

#[cfg(feature = "std")]
pub use plan::FftPlan;
#[cfg(feature = "std")]
pub use spsc::SpscQueue;
