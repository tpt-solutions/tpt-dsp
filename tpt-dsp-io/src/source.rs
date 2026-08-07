//! Driver-agnostic IQ ingestion.
//!
//! [`IqSource`] is the single interface the DSP side of an SDR receiver needs:
//! a blocking pull of complex baseband samples into a caller-owned buffer.
//! Every backend in this crate implements it ([`crate::TcpIqSource`],
//! [`crate::RtlSdrSource`], [`SyntheticIqSource`]), so a receive chain can be
//! written once and re-targeted at a different front-end by swapping the
//! source.
//!
//! # Real-time safety
//!
//! `recv` writes into a slice the caller already owns and implementations in
//! this crate allocate their scratch space up front, so the steady-state
//! receive loop performs no allocation.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use tpt_dsp_core::{exp_i, Complex32};

/// Failure modes shared by all [`IqSource`] implementations.
#[derive(Debug)]
#[non_exhaustive]
pub enum IqError {
    /// The underlying transport or device reported an I/O error.
    Io(std::io::Error),
    /// The source is permanently finished (device unplugged, peer gone).
    Closed,
    /// The backend exists but was compiled out; the payload is the cargo
    /// feature that enables it.
    FeatureDisabled(&'static str),
    /// The backend is present but cannot service the request; the payload
    /// explains why.
    Unsupported(&'static str),
}

impl core::fmt::Display for IqError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            IqError::Io(err) => write!(f, "iq transport error: {err}"),
            IqError::Closed => write!(f, "iq source closed"),
            IqError::FeatureDisabled(feature) => {
                write!(f, "tpt-dsp-io was not built with the `{feature}` feature")
            }
            IqError::Unsupported(reason) => write!(f, "iq source unavailable: {reason}"),
        }
    }
}

impl std::error::Error for IqError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IqError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IqError {
    fn from(err: std::io::Error) -> Self {
        IqError::Io(err)
    }
}

/// A synchronous, pull-based stream of complex baseband samples.
pub trait IqSource {
    /// Fill `buf` with up to `buf.len()` samples, returning how many were
    /// written.
    ///
    /// Blocks until at least one sample is available. A return value of `0`
    /// means the stream ended cleanly; anything else leaves `buf[..n]` filled
    /// with consecutive samples in arrival order, with no gap relative to the
    /// previous call.
    ///
    /// # Errors
    ///
    /// Returns [`IqError`] if the transport fails or the backend is
    /// unavailable.
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError>;
}

impl<T: IqSource + ?Sized> IqSource for &mut T {
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError> {
        (**self).recv(buf)
    }
}

impl<T: IqSource + ?Sized> IqSource for Box<T> {
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError> {
        (**self).recv(buf)
    }
}

/// An in-memory [`IqSource`] that replays a pre-generated ring of samples.
///
/// Nothing is computed or allocated in `recv`, so it can saturate a receive
/// loop at hardware sample rates — useful for pipeline examples, throughput
/// tests and benchmarks without a radio attached.
pub struct SyntheticIqSource {
    ring: Vec<Complex32>,
    pos: usize,
    delivered: u64,
    remaining: Option<u64>,
}

impl SyntheticIqSource {
    /// Build a source that endlessly repeats `ring`.
    ///
    /// # Panics
    ///
    /// Panics if `ring` is empty.
    pub fn from_ring(ring: Vec<Complex32>) -> Self {
        assert!(
            !ring.is_empty(),
            "synthetic source needs at least one sample"
        );
        Self {
            ring,
            pos: 0,
            delivered: 0,
            remaining: None,
        }
    }

    /// Build a unit-amplitude FM carrier at baseband, modulated by a single
    /// audio tone: `exp(-i·β·cos(2π·f_tone·t))` with modulation index
    /// `β = deviation / tone`.
    ///
    /// The ring holds exactly one tone period, so replay is phase-continuous
    /// across the wrap: demodulating the stream yields an unbroken sine at
    /// `tone_hz` of amplitude 1.0 when the discriminator is scaled with
    /// [`FmDemodulator::with_deviation`](tpt_dsp_core::FmDemodulator::with_deviation).
    /// `tone_hz` is quantised to `sample_rate_hz / round(sample_rate_hz /
    /// tone_hz)` so that period is a whole number of samples.
    ///
    /// # Panics
    ///
    /// Panics unless `sample_rate_hz > 0`, `tone_hz > 0`, `deviation_hz > 0`
    /// and the tone period rounds to at least two samples.
    pub fn fm_tone(sample_rate_hz: f64, tone_hz: f64, deviation_hz: f64) -> Self {
        assert!(sample_rate_hz > 0.0, "sample rate must be positive");
        assert!(tone_hz > 0.0, "tone frequency must be positive");
        assert!(deviation_hz > 0.0, "deviation must be positive");
        let period = (sample_rate_hz / tone_hz).round() as usize;
        assert!(period >= 2, "tone period must span at least two samples");

        let beta = deviation_hz / tone_hz;
        let ring = (0..period)
            .map(|n| {
                let arg = core::f64::consts::TAU * n as f64 / period as f64;
                exp_i((-beta * arg.cos()) as f32)
            })
            .collect();
        Self::from_ring(ring)
    }

    /// Stop after `samples` further samples, after which [`recv`](IqSource::recv)
    /// reports end of stream.
    #[must_use]
    pub fn with_limit(mut self, samples: u64) -> Self {
        self.remaining = Some(samples);
        self
    }

    /// Total samples handed out so far.
    #[inline]
    pub fn delivered(&self) -> u64 {
        self.delivered
    }

    /// The generated sample ring.
    #[inline]
    pub fn ring(&self) -> &[Complex32] {
        &self.ring
    }
}

impl IqSource for SyntheticIqSource {
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError> {
        let mut n = buf.len();
        if let Some(remaining) = self.remaining {
            n = n.min(remaining as usize);
        }
        for slot in buf[..n].iter_mut() {
            *slot = self.ring[self.pos];
            self.pos += 1;
            if self.pos == self.ring.len() {
                self.pos = 0;
            }
        }
        self.delivered += n as u64;
        if let Some(remaining) = self.remaining.as_mut() {
            *remaining -= n as u64;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_source_is_continuous_across_wrap() {
        let mut src = SyntheticIqSource::fm_tone(48_000.0, 1_000.0, 5_000.0);
        assert_eq!(src.ring().len(), 48);
        let mut buf = [Complex32::default(); 100];
        assert_eq!(src.recv(&mut buf).unwrap(), 100);
        for n in 0..52 {
            assert!((buf[n] - buf[n + 48]).norm() < 1e-6);
        }
        assert_eq!(src.delivered(), 100);
    }

    #[test]
    fn synthetic_source_honours_limit() {
        let mut src = SyntheticIqSource::fm_tone(48_000.0, 1_000.0, 5_000.0).with_limit(10);
        let mut buf = [Complex32::default(); 8];
        assert_eq!(src.recv(&mut buf).unwrap(), 8);
        assert_eq!(src.recv(&mut buf).unwrap(), 2);
        assert_eq!(src.recv(&mut buf).unwrap(), 0);
        assert_eq!(src.delivered(), 10);
    }

    #[test]
    fn boxed_source_forwards_recv() {
        let mut src: Box<dyn IqSource> =
            Box::new(SyntheticIqSource::fm_tone(48_000.0, 1_000.0, 5_000.0).with_limit(4));
        let mut buf = [Complex32::default(); 8];
        assert_eq!(src.recv(&mut buf).unwrap(), 4);
    }

    #[test]
    fn error_display_mentions_feature() {
        let err = IqError::FeatureDisabled("rtl-sdr");
        assert!(err.to_string().contains("rtl-sdr"));
    }
}
