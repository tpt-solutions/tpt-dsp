//! IQ (In-phase / Quadrature) sample parsing for SDR / RF streams.
//!
//! RTL-SDR and similar dongles emit raw interleaved I/Q samples in a variety
//! of byte layouts. These helpers convert those byte buffers into
//! [`Complex32`] samples without any allocation, so they are safe to call from
//! a high-throughput receive loop.

use tpt_dsp_core::Complex32;

/// The byte layout of an interleaved I/Q stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IqFormat {
    /// 8-bit unsigned samples, offset-binary (`0x80` → 0.0). Interleaved I0,Q0,I1,Q1,…
    U8,
    /// 16-bit signed little-endian, interleaved I0,Q0,I1,Q1,…
    I16Le,
    /// 16-bit signed big-endian, interleaved I0,Q0,I1,Q1,…
    I16Be,
    /// 32-bit float little-endian, interleaved I0,Q0,I1,Q1,…
    F32Le,
}

impl IqFormat {
    /// Number of bytes consumed per complex sample.
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            IqFormat::U8 => 2,
            IqFormat::I16Le | IqFormat::I16Be => 4,
            IqFormat::F32Le => 8,
        }
    }
}

/// Parse as many complex samples as `bytes` allows into `out`.
///
/// Returns the number of [`Complex32`] samples written. If `bytes` ends
/// mid-sample the trailing partial sample is ignored.
pub fn parse_iq(format: IqFormat, bytes: &[u8], out: &mut [Complex32]) -> usize {
    let bps = format.bytes_per_sample();
    let pairs = bytes.len() / bps;
    let n = pairs.min(out.len());
    for (o, chunk) in out.iter_mut().take(n).zip(bytes.chunks_exact(bps)) {
        let (i_val, q_val) = match format {
            IqFormat::U8 => {
                let i8 = chunk[0] as f32;
                let q8 = chunk[1] as f32;
                ((i8 - 128.0) / 128.0, (q8 - 128.0) / 128.0)
            }
            IqFormat::I16Le => {
                let i_raw = i16::from_le_bytes([chunk[0], chunk[1]]) as f32;
                let q_raw = i16::from_le_bytes([chunk[2], chunk[3]]) as f32;
                (i_raw / 32768.0, q_raw / 32768.0)
            }
            IqFormat::I16Be => {
                let i_raw = i16::from_be_bytes([chunk[0], chunk[1]]) as f32;
                let q_raw = i16::from_be_bytes([chunk[2], chunk[3]]) as f32;
                (i_raw / 32768.0, q_raw / 32768.0)
            }
            IqFormat::F32Le => {
                let i_raw = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                let q_raw = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                (i_raw, q_raw)
            }
        };
        *o = Complex32::new(i_val, q_val);
    }
    n
}

/// A streaming IQ parser that buffers bytes until it can emit complete
/// complex samples. Useful when the underlying transport delivers arbitrary
/// chunk sizes.
pub struct IqStream {
    format: IqFormat,
    buffer: Vec<u8>,
}

impl IqStream {
    /// Create a parser for the given format with an initial buffer capacity.
    pub fn new(format: IqFormat, capacity: usize) -> Self {
        Self {
            format,
            buffer: Vec::with_capacity(capacity.max(64)),
        }
    }

    /// Feed raw bytes; return the number of complete samples available via
    /// [`drain`](Self::drain).
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Parse and remove all complete samples currently buffered, writing them to
    /// `out`. Returns the number written.
    pub fn drain(&mut self, out: &mut [Complex32]) -> usize {
        let written = parse_iq(self.format, &self.buffer, out);
        let consumed = written * self.format.bytes_per_sample();
        self.buffer.drain(..consumed);
        written
    }

    /// Bytes currently buffered (possibly a partial trailing sample).
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn u8_roundtrip() {
        let bytes = [128u8, 128, 255, 0, 0, 255]; // (0,0), (+1,-1), (-1,+1)
        let mut out = [Complex32::default(); 3];
        let n = parse_iq(IqFormat::U8, &bytes, &mut out);
        assert_eq!(n, 3);
        assert!((out[0].re - 0.0).abs() < 1e-6 && (out[0].im - 0.0).abs() < 1e-6);
        // U8 maps 0→-1, 128→0, 255→+127/128 (full symmetric [-1, 1) range).
        assert!((out[1].re - 127.0 / 128.0).abs() < 1e-6 && (out[1].im + 1.0).abs() < 1e-6);
        assert!((out[2].re + 1.0).abs() < 1e-6 && (out[2].im - 127.0 / 128.0).abs() < 1e-6);
    }

    #[test]
    fn i16_le_roundtrip() {
        let i = 16384i16; // 0.5
        let q = -32768i16; // -1.0
        let mut bytes = i.to_le_bytes().to_vec();
        bytes.extend_from_slice(&q.to_le_bytes());
        let mut out = [Complex32::default(); 1];
        let n = parse_iq(IqFormat::I16Le, &bytes, &mut out);
        assert_eq!(n, 1);
        assert!((out[0].re - 0.5).abs() < 1e-6);
        assert!((out[0].im + 1.0).abs() < 1e-6);
    }

    #[test]
    fn partial_trailing_sample_ignored() {
        // U8 needs 2 bytes/sample; 5 bytes → 2 complete samples.
        let bytes = [128u8, 128, 200, 100, 50];
        let mut out = [Complex32::default(); 4];
        let n = parse_iq(IqFormat::U8, &bytes, &mut out);
        assert_eq!(n, 2);
    }

    #[test]
    fn stream_reassembles_split_chunks() {
        let mut stream = IqStream::new(IqFormat::U8, 16);
        stream.feed(&[128u8, 128, 255]); // partial second sample
        let mut out = [Complex32::default(); 4];
        assert_eq!(stream.drain(&mut out), 1); // only first complete
        stream.feed(&[0]); // now second sample complete
        let n = stream.drain(&mut out);
        assert_eq!(n, 1);
        // Second drained sample is (255, 0) → re = 127/128, im = -1.0.
        assert!((out[0].re - 127.0 / 128.0).abs() < 1e-6);
    }
}
