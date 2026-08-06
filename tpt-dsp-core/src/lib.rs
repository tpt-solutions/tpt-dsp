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
pub mod demod;
mod fft;
mod filters;
mod hilbert;
#[cfg(feature = "alloc")]
pub mod resample;
mod ring;
mod windows;

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
pub use fft::{fft, fft_inplace, ifft, ifft_inplace, is_power_of_two, next_power_of_two, twiddles};
pub use filters::{process_biquad, Biquad, BiquadCoeffs, BiquadType};
pub use hilbert::hilbert;
pub use ring::{RingBuffer, RingRead, RingWrite};
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
