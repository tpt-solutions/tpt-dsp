//! `tpt-dsp-io` — pure Rust hardware I/O for tpt-dsp.
//!
//! - `iq` — parse raw interleaved I/Q byte streams (RTL-SDR, etc.) into
//!   `tpt_dsp_core::Complex32` samples. Hardware-independent, always available.
//! - `source` — the driver-agnostic [`IqSource`] trait plus [`SyntheticIqSource`],
//!   an in-memory generator for tests, examples and benchmarks.
//! - `tcp` — [`TcpIqSource`], a blocking [`IqSource`] over any reader, and
//!   (feature `tcp`) the async [`serve_iq`] server.
//! - `rtlsdr` — [`RtlSdrSource`]; a documented stub unless a driver is wired in
//!   behind the `rtl-sdr` feature.
//! - `audio` (feature `audio`) — cpal-based real-time output stream.
//! - `serial` (feature `serial`) — serial-port byte reader.
//!
//! # Ingesting samples
//!
//! Everything that produces baseband implements [`IqSource`], so a receive
//! chain is written once against `recv` and re-targeted by swapping the
//! source. Byte-oriented transports run through [`IqReassembler`], which
//! carries a sample split across two reads into the next read instead of
//! discarding it. See `examples/sdr_pipeline.rs` for decimation and FM
//! demodulation on top of a source.
//!
//! The `audio`/`serial` modules are feature-gated and not part of the default
//! public surface; import their items directly (e.g. `run_output`).
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod iq;
mod rtlsdr;
mod source;
mod tcp;

#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "serial")]
mod serial;

pub use iq::{parse_iq, IqFormat, IqReassembler, IqStream, Reassembled, MAX_BYTES_PER_SAMPLE};
pub use rtlsdr::{RtlSdrConfig, RtlSdrSource};
pub use source::{IqError, IqSource, SyntheticIqSource};
pub use tcp::TcpIqSource;

#[cfg(feature = "audio")]
pub use audio::{list_output_devices, run_output};
#[cfg(feature = "serial")]
pub use serial::SerialReader;
#[cfg(feature = "tcp")]
pub use tcp::serve_iq;
