//! RTL-SDR (RTL2832U) front-end.
//!
//! # This build ships a stub
//!
//! [`RtlSdrSource`] implements [`IqSource`] but never returns samples: every
//! constructor and every `recv` fails with a descriptive [`IqError`]. The
//! `rtl-sdr` cargo feature selects which message is produced, so an
//! application can compile and wire up the source today and get a real device
//! the moment a backend is linked in.
//!
//! No driver crate is depended upon because none of the candidates can be
//! added without breaking a plain `cargo build` / `cargo test` of this
//! workspace:
//!
//! - `rtl-sdr` (0.1.5) is an FFI binding to the native `librtlsdr` C library.
//!   It compiles as an rlib but any binary or test that links it fails when
//!   the C library is absent (`cannot open input file 'rtlsdr.lib'` on MSVC),
//!   which would break `cargo test --all-features` on every machine without a
//!   system librtlsdr.
//! - `librtlsdr-rs` (0.3) is a pure-Rust port but is GPL-2.0-or-later, which
//!   is incompatible with this workspace's MIT/Apache-2.0 licensing, and it
//!   requires a far newer MSRV than the declared 1.74.
//! - `rs-rtl` / `rtlsdr_mt` pull a USB stack (`nusb`, `libusb`) into the
//!   dependency tree for a device that CI can never enumerate.
//!
//! # Wiring a real device
//!
//! Add the chosen driver as an optional dependency behind the `rtl-sdr`
//! feature, then replace [`RtlSdrSource::open`] with device setup (open by
//! [`RtlSdrConfig::device_index`], apply centre frequency, sample rate and
//! gain, reset the buffer) and [`IqSource::recv`] with a synchronous bulk read
//! that decodes the dongle's native `IqFormat::U8` payload through an
//! [`IqReassembler`](crate::IqReassembler). Nothing else in the pipeline
//! changes.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use tpt_dsp_core::Complex32;

use crate::source::{IqError, IqSource};

/// Tuner settings for an RTL-SDR dongle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtlSdrConfig {
    /// Index of the device to open, in enumeration order.
    pub device_index: u32,
    /// Centre (tuner) frequency in hertz.
    pub center_frequency_hz: u32,
    /// Requested sample rate in hertz; 2.4 MS/s is the usual FM setting.
    pub sample_rate_hz: u32,
    /// Tuner gain in tenths of a decibel, or `None` for the automatic gain
    /// control the dongle applies by default.
    pub tuner_gain_tenth_db: Option<i32>,
}

impl Default for RtlSdrConfig {
    fn default() -> Self {
        Self {
            device_index: 0,
            center_frequency_hz: 100_100_000,
            sample_rate_hz: 2_400_000,
            tuner_gain_tenth_db: None,
        }
    }
}

#[cfg(not(feature = "rtl-sdr"))]
fn unavailable() -> IqError {
    IqError::FeatureDisabled("rtl-sdr")
}

#[cfg(feature = "rtl-sdr")]
fn unavailable() -> IqError {
    IqError::Unsupported(
        "the `rtl-sdr` feature is enabled but this build links no RTL-SDR driver; \
         see the RtlSdrSource documentation for how to wire one in",
    )
}

/// An RTL-SDR USB front-end.
///
/// **Stub in this build.** [`open`](Self::open) always fails, and so does
/// [`recv`](IqSource::recv), with an error naming either the disabled
/// `rtl-sdr` cargo feature or the missing driver. No driver crate is depended
/// upon because the FFI binding (`rtl-sdr`) needs the native `librtlsdr` to
/// link, the pure-Rust port (`librtlsdr-rs`) is GPL-2.0-or-later and raises
/// the MSRV, and the USB-stack alternatives cannot enumerate a device in CI —
/// any of them would break the default build or `cargo test --all-features`.
/// To wire a real dongle in, add the driver behind the `rtl-sdr` feature and
/// fill in `open` (device index, centre frequency, sample rate, gain) plus
/// `recv` (bulk read decoded as `IqFormat::U8` through an
/// [`IqReassembler`](crate::IqReassembler)); the rest of the pipeline is
/// unchanged.
#[derive(Debug)]
pub struct RtlSdrSource {
    config: RtlSdrConfig,
}

impl RtlSdrSource {
    /// Open the configured device.
    ///
    /// # Errors
    ///
    /// Always fails in this build: [`IqError::FeatureDisabled`] without the
    /// `rtl-sdr` feature, [`IqError::Unsupported`] with it.
    pub fn open(config: RtlSdrConfig) -> Result<Self, IqError> {
        let _ = config;
        Err(unavailable())
    }

    /// The configuration this source was created with.
    #[inline]
    pub fn config(&self) -> &RtlSdrConfig {
        &self.config
    }
}

impl IqSource for RtlSdrSource {
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError> {
        let _ = buf;
        Err(unavailable())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_reports_the_missing_backend() {
        let err = RtlSdrSource::open(RtlSdrConfig::default()).unwrap_err();
        assert!(err.to_string().contains("rtl-sdr"), "{err}");
    }

    #[test]
    fn stub_recv_fails_without_touching_the_buffer() {
        let mut source = RtlSdrSource {
            config: RtlSdrConfig::default(),
        };
        assert_eq!(source.config().sample_rate_hz, 2_400_000);
        let mut buf = [Complex32::new(1.0, 2.0); 4];
        let err = source.recv(&mut buf).unwrap_err();
        assert!(err.to_string().contains("rtl-sdr"), "{err}");
        assert_eq!(buf[0], Complex32::new(1.0, 2.0));
    }
}
