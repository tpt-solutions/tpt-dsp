//! `tpt-dsp-io` — pure Rust hardware I/O for tpt-dsp.
//!
//! - [`iq`] — parse raw interleaved I/Q byte streams (RTL-SDR, etc.) into
//!   [`Complex32`] samples. Hardware-independent, always available.
//! - `audio` (feature `audio`) — cpal-based real-time output stream.
//! - `serial` (feature `serial`) — serial-port byte reader.
//! - `tcp` (feature `tcp`) — async TCP server for streaming IQ data.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod iq;

#[cfg(feature = "audio")]
mod audio;
#[cfg(feature = "serial")]
mod serial;
#[cfg(feature = "tcp")]
mod tcp;

pub use iq::{parse_iq, IqFormat, IqStream};

#[cfg(feature = "audio")]
pub use audio::{list_output_devices, run_output};
#[cfg(feature = "serial")]
pub use serial::SerialReader;
#[cfg(feature = "tcp")]
pub use tcp::serve_iq;
