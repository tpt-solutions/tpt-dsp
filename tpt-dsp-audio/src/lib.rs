//! `tpt-dsp-audio` — synthesis, effects and real-time audio graphs.
//!
//! This crate builds on [`tpt_dsp_core`] to provide the pieces needed for a
//! browser or desktop audio engine:
//!
//! - Oscillators ([`Oscillator`], [`Wavetable`]) and synthesis engines
//!   ([`FmSynth`], [`SubtractiveVoice`]).
//! - Effects: [`Waveshaper`], [`Delay`], [`ConvolutionReverb`], [`Eq`].
//! - An [`AudioGraph`] abstraction (sources → effects → sinks) and a
//!   [`RealtimeEngine`] that drives a per-block DSP callback with fixed,
//!   allocation-free block processing (128/256-sample blocks).
//!
//! All hot-path processing operates on caller-supplied or pre-allocated
//! buffers and never allocates, so it is safe to call from an audio callback.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod delay;
mod envelope;
mod engine;
mod eq;
mod fm;
mod graph;
mod oscillator;
mod reverb;
mod subtractive;
mod waveshaper;
mod wavetable;

pub use delay::Delay;
pub use envelope::{Adsr, EnvelopeState};
pub use engine::{RealtimeEngine, BLOCK_128, BLOCK_256};
pub use eq::Eq;
pub use fm::FmSynth;
pub use graph::{
    AudioGraph, AudioNode, ClosureNode, ClosureSink, ClosureSource, Passthrough, Sink, Source,
};
pub use oscillator::{Oscillator, Waveform};
pub use reverb::{generate_decay_ir, ConvolutionReverb};
pub use subtractive::SubtractiveVoice;
pub use waveshaper::{Curve, Waveshaper};
pub use wavetable::Wavetable;
