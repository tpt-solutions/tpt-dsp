//! IQ (In-phase / Quadrature) sample parsing for SDR / RF streams.
//!
//! RTL-SDR and similar dongles emit raw interleaved I/Q samples in a variety
//! of byte layouts. These helpers convert those byte buffers into
//! [`Complex32`] samples without any allocation, so they are safe to call from
//! a high-throughput receive loop.

use tpt_dsp_core::Complex32;

/// Bytes occupied by the widest sample layout in [`IqFormat`].
pub const MAX_BYTES_PER_SAMPLE: usize = 8;

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

/// Outcome of [`IqReassembler::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reassembled {
    /// Complete samples written to the head of the output slice.
    pub samples: usize,
    /// Input bytes taken by the reassembler: either parsed, or retained
    /// internally as the leading fragment of the next sample. Bytes beyond
    /// this count are untouched and must be offered again.
    pub consumed: usize,
}

/// A zero-allocation reassembler for byte streams whose chunk boundaries do
/// not align with sample boundaries.
///
/// Transports such as TCP split their payload at arbitrary offsets, so a
/// complex sample can straddle two reads. [`IqReassembler`] keeps the trailing
/// fragment of a chunk in a fixed-size internal buffer and completes it from
/// the front of the next chunk, so no byte is dropped and the I/Q phase of the
/// stream is never inverted.
///
/// Unlike [`IqStream`] the bulk of the data is parsed straight out of the
/// caller's buffer: only the sub-sample remainder (at most
/// [`MAX_BYTES_PER_SAMPLE`] - 1 bytes) is ever copied, and no allocation
/// happens on any path, which makes it usable from a real-time receive loop.
///
/// ```
/// # use tpt_dsp_io::{IqFormat, IqReassembler};
/// # use tpt_dsp_core::Complex32;
/// let mut r = IqReassembler::new(IqFormat::U8);
/// let mut out = [Complex32::default(); 4];
/// assert_eq!(r.push(&[128, 128, 255], &mut out).samples, 1);
/// assert_eq!(r.pending_bytes(), 1);
/// assert_eq!(r.push(&[0], &mut out).samples, 1);
/// ```
#[derive(Debug, Clone)]
pub struct IqReassembler {
    format: IqFormat,
    partial: [u8; MAX_BYTES_PER_SAMPLE],
    partial_len: usize,
}

impl IqReassembler {
    /// Create a reassembler for `format`.
    pub fn new(format: IqFormat) -> Self {
        Self {
            format,
            partial: [0u8; MAX_BYTES_PER_SAMPLE],
            partial_len: 0,
        }
    }

    /// The format being reassembled.
    #[inline]
    pub fn format(&self) -> IqFormat {
        self.format
    }

    /// Bytes of an incomplete sample currently held over from a previous push.
    #[inline]
    pub fn pending_bytes(&self) -> usize {
        self.partial_len
    }

    /// Discard the held-over fragment, e.g. after a reconnect.
    pub fn reset(&mut self) {
        self.partial_len = 0;
    }

    /// Parse `bytes` into `out`, completing any sample left dangling by the
    /// previous call.
    ///
    /// Fewer bytes than supplied are consumed when `out` fills up; call again
    /// with the remainder (`&bytes[result.consumed..]`) after draining `out`.
    pub fn push(&mut self, bytes: &[u8], out: &mut [Complex32]) -> Reassembled {
        let bps = self.format.bytes_per_sample();
        debug_assert!(bps <= MAX_BYTES_PER_SAMPLE);
        if out.is_empty() {
            return Reassembled::default();
        }

        let mut samples = 0;
        let mut consumed = 0;

        if self.partial_len > 0 {
            let take = (bps - self.partial_len).min(bytes.len());
            self.partial[self.partial_len..self.partial_len + take].copy_from_slice(&bytes[..take]);
            self.partial_len += take;
            consumed += take;
            if self.partial_len < bps {
                return Reassembled { samples, consumed };
            }
            samples += parse_iq(self.format, &self.partial[..bps], out);
            self.partial_len = 0;
        }

        let written = parse_iq(self.format, &bytes[consumed..], &mut out[samples..]);
        consumed += written * bps;
        samples += written;

        let tail = bytes.len() - consumed;
        if tail > 0 && tail < bps {
            self.partial[..tail].copy_from_slice(&bytes[consumed..]);
            self.partial_len = tail;
            consumed = bytes.len();
        }

        Reassembled { samples, consumed }
    }
}

/// A streaming IQ parser that buffers bytes until it can emit complete
/// complex samples. Useful when the underlying transport delivers arbitrary
/// chunk sizes.
///
/// # Memory growth
///
/// [`feed`](Self::feed) appends to an internal byte buffer that is only
/// reclaimed by [`drain`](Self::drain). If the caller keeps feeding bytes
/// without ever draining, the buffer grows without bound — there is no
/// back-pressure or built-in cap. [`feed`](Self::feed) is therefore only safe
/// for transports whose bytes are always promptly drained (the bounded
/// [`IqReassembler`] is the recommended zero-allocation path for streaming
/// IQ such as the `tcp.rs` server, which does not use this type).
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
    ///
    /// Appends to the internal buffer, which is unaffected by this call —
    /// stale bytes accumulate until [`drain`](Self::drain) is invoked, so an
    /// application that never drains will grow the buffer without bound.
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

    fn u8_pattern(samples: usize) -> (Vec<u8>, Vec<Complex32>) {
        let mut bytes = Vec::with_capacity(samples * 2);
        let mut expect = Vec::with_capacity(samples);
        for i in 0..samples {
            let i_byte = (i % 251) as u8;
            let q_byte = ((i * 7 + 3) % 251) as u8;
            bytes.push(i_byte);
            bytes.push(q_byte);
            expect.push(Complex32::new(
                (i_byte as f32 - 128.0) / 128.0,
                (q_byte as f32 - 128.0) / 128.0,
            ));
        }
        (bytes, expect)
    }

    fn drive(format: IqFormat, bytes: &[u8], chunk_sizes: &[usize]) -> Vec<Complex32> {
        let mut r = IqReassembler::new(format);
        let mut out = [Complex32::default(); 3];
        let mut got = Vec::new();
        let mut pos = 0;
        let mut next = 0;
        while pos < bytes.len() {
            let take = chunk_sizes[next % chunk_sizes.len()].min(bytes.len() - pos);
            next += 1;
            let chunk = &bytes[pos..pos + take];
            let mut offset = 0;
            while offset < chunk.len() {
                let res = r.push(&chunk[offset..], &mut out);
                got.extend_from_slice(&out[..res.samples]);
                if res.consumed == 0 {
                    break;
                }
                offset += res.consumed;
            }
            pos += take;
        }
        got
    }

    #[test]
    fn reassembler_recovers_samples_split_mid_sample() {
        let (bytes, expect) = u8_pattern(500);
        // Odd chunk sizes guarantee reads that end between the I and Q byte.
        let got = drive(IqFormat::U8, &bytes, &[1, 3, 5, 7, 11]);
        assert_eq!(got.len(), expect.len());
        assert_eq!(got, expect);
    }

    #[test]
    fn reassembler_handles_wide_formats() {
        let mut bytes = Vec::new();
        let mut expect = Vec::new();
        for i in 0..64 {
            let re = i as f32 * 0.25;
            let im = -(i as f32) * 0.5;
            bytes.extend_from_slice(&re.to_le_bytes());
            bytes.extend_from_slice(&im.to_le_bytes());
            expect.push(Complex32::new(re, im));
        }
        // 3-byte chunks split 8-byte samples at every possible offset.
        assert_eq!(drive(IqFormat::F32Le, &bytes, &[3]), expect);
        // The same 512 bytes read as 4-byte I16Le samples: 128 of them.
        assert_eq!(drive(IqFormat::I16Le, &bytes, &[5, 1, 2]).len(), 128);
    }

    #[test]
    fn reassembler_reports_unconsumed_bytes_when_output_is_full() {
        let mut r = IqReassembler::new(IqFormat::U8);
        let mut out = [Complex32::default(); 2];
        let res = r.push(&[1, 2, 3, 4, 5, 6, 7], &mut out);
        assert_eq!(res.samples, 2);
        assert_eq!(res.consumed, 4);
        assert_eq!(r.pending_bytes(), 0);
        let res = r.push(&[5, 6, 7], &mut out);
        assert_eq!(res.samples, 1);
        assert_eq!(res.consumed, 3);
        assert_eq!(r.pending_bytes(), 1);
        r.reset();
        assert_eq!(r.pending_bytes(), 0);
    }
}
