// SPDX-License-Identifier: MIT OR Apache-2.0
//! `tpt-dsp-wasm` — a web-native guitar effects pedal.
//!
//! This crate is the WebAssembly front end for the `tpt-dsp` workspace. It
//! exposes a [`Pedalboard`] to JavaScript whose signal chain is
//! `distortion → delay → reverb → EQ`, built entirely from `tpt-dsp-audio`
//! primitives ([`Waveshaper`](tpt_dsp_audio::Waveshaper),
//! [`Delay`](tpt_dsp_audio::Delay),
//! [`ConvolutionReverb`](tpt_dsp_audio::ConvolutionReverb) and
//! [`Eq`](tpt_dsp_audio::Eq)) wired together behind the
//! [`AudioNode`](tpt_dsp_audio::AudioNode) graph trait.
//!
//! The real-time contract mirrors a Web Audio render quantum: exactly
//! [`BLOCK`] (128) samples in, 128 samples out, no heap allocation between
//! construction and teardown. See
//! [`Pedalboard::process_block_128`] for the allocation-free entry point and
//! the `www/` directory for the AudioWorklet glue that drives it.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod pedalboard;
mod web;

pub use pedalboard::{DistortionCurve, Pedalboard, BLOCK, EQ_BAND_COUNT};
#[cfg(feature = "async")]
pub use web::open_microphone;
pub use web::{
    connect_stream, create_pedal_node, processor_name, register_worklet, PROCESSOR_NAME,
};
