//! TCP ingestion of raw interleaved I/Q byte streams (rtl_tcp and friends).
//!
//! - [`TcpIqSource`] — a blocking [`IqSource`] over any [`Read`]er, by default
//!   a [`std::net::TcpStream`]. Always available; no async runtime.
//! - [`serve_iq`] — a single-connection async server that pushes parsed frames
//!   to a callback. Enabled by the `tcp` feature (pulls in `tokio`).
//!
//! TCP is a byte stream: a segment boundary can fall between the I and the Q
//! half of a sample. Both entry points therefore run their bytes through an
//! [`IqReassembler`], which carries the trailing fragment of one read into the
//! next. Dropping that fragment instead would not only lose a sample, it would
//! shift the stream by one byte and swap I with Q for everything that follows.

use std::io::Read;

use tpt_dsp_core::Complex32;

use crate::iq::{IqFormat, IqReassembler};
use crate::source::{IqError, IqSource};

const DEFAULT_CAPACITY: usize = 8192;

/// A blocking [`IqSource`] that decodes interleaved I/Q bytes from a reader.
///
/// Sample reassembly is preserved across reads and across `recv` calls, so
/// samples are delivered complete and in order no matter how the transport
/// chops up the stream. The byte buffer is allocated once at construction and
/// the decode path is allocation-free.
///
/// ```no_run
/// # use tpt_dsp_io::{IqFormat, IqSource, TcpIqSource};
/// # use tpt_dsp_core::Complex32;
/// # fn main() -> std::io::Result<()> {
/// let mut source = TcpIqSource::connect("127.0.0.1:1234", IqFormat::U8)?;
/// let mut buf = vec![Complex32::default(); 4096];
/// while let Ok(n) = source.recv(&mut buf) {
///     if n == 0 {
///         break;
///     }
/// }
/// # Ok(())
/// # }
/// ```
pub struct TcpIqSource<R = std::net::TcpStream> {
    reader: R,
    reassembler: IqReassembler,
    bytes: Vec<u8>,
    start: usize,
    end: usize,
}

/// The greeting `rtl_tcp` sends before its sample stream.
///
/// `rtl_tcp` writes the magic `RTL0`, then the tuner type and the number of
/// tuner gain steps as big-endian `u32`s, and then unsigned 8-bit I/Q pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtlTcpInfo {
    /// Tuner type code reported by the dongle (librtlsdr's tuner enum).
    pub tuner: u32,
    /// Number of tuner gain steps.
    pub gain_count: u32,
}

/// Read and check the 12-byte `rtl_tcp` greeting from `reader`.
///
/// # Errors
///
/// Returns an error if the stream ends before the greeting is complete, or if
/// it does not start with the magic `RTL0`.
pub fn read_rtl_tcp_greeting(reader: &mut impl Read) -> std::io::Result<RtlTcpInfo> {
    let mut buf = [0u8; 12];
    reader.read_exact(&mut buf)?;
    if &buf[..4] != b"RTL0" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not an rtl_tcp stream: missing RTL0 greeting",
        ));
    }
    Ok(RtlTcpInfo {
        tuner: u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]),
        gain_count: u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]),
    })
}

impl TcpIqSource<std::net::TcpStream> {
    /// Connect to a TCP server streaming raw I/Q in `format`.
    ///
    /// # Errors
    ///
    /// Returns the connection error.
    pub fn connect(addr: impl std::net::ToSocketAddrs, format: IqFormat) -> std::io::Result<Self> {
        Ok(Self::new(std::net::TcpStream::connect(addr)?, format))
    }

    /// Connect to an `rtl_tcp` server (the RTL-SDR network protocol).
    ///
    /// Reads the greeting first, so the first sample is the first I/Q pair.
    /// The stream is unsigned 8-bit pairs, as [`IqFormat::U8`]. Start the
    /// server with `rtl_tcp -a <addr> -p <port>` (librtlsdr).
    ///
    /// # Errors
    ///
    /// Returns the connection error, or an error for a missing or invalid
    /// greeting.
    pub fn connect_rtl_tcp(
        addr: impl std::net::ToSocketAddrs,
    ) -> std::io::Result<(Self, RtlTcpInfo)> {
        let mut stream = std::net::TcpStream::connect(addr)?;
        let info = read_rtl_tcp_greeting(&mut stream)?;
        Ok((Self::new(stream, IqFormat::U8), info))
    }
}

impl<R: Read> TcpIqSource<R> {
    /// Wrap an existing reader with a default 8 KiB read buffer.
    pub fn new(reader: R, format: IqFormat) -> Self {
        Self::with_capacity(reader, format, DEFAULT_CAPACITY)
    }

    /// Wrap an existing reader, sizing the read buffer explicitly.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is smaller than one sample.
    pub fn with_capacity(reader: R, format: IqFormat, capacity: usize) -> Self {
        assert!(
            capacity >= format.bytes_per_sample(),
            "read buffer must hold at least one sample"
        );
        Self {
            reader,
            reassembler: IqReassembler::new(format),
            bytes: vec![0u8; capacity],
            start: 0,
            end: 0,
        }
    }

    /// The format being decoded.
    #[inline]
    pub fn format(&self) -> IqFormat {
        self.reassembler.format()
    }

    /// Bytes held back for a sample that is still incomplete.
    #[inline]
    pub fn pending_bytes(&self) -> usize {
        self.reassembler.pending_bytes() + (self.end - self.start)
    }

    /// Borrow the underlying reader.
    #[inline]
    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    /// Mutably borrow the underlying reader.
    #[inline]
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Consume the source and return the reader, discarding buffered bytes.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: Read> IqSource for TcpIqSource<R> {
    fn recv(&mut self, buf: &mut [Complex32]) -> Result<usize, IqError> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut written = 0;
        loop {
            while self.start < self.end && written < buf.len() {
                let res = self
                    .reassembler
                    .push(&self.bytes[self.start..self.end], &mut buf[written..]);
                if res.consumed == 0 {
                    break;
                }
                self.start += res.consumed;
                written += res.samples;
            }
            if self.start >= self.end {
                self.start = 0;
                self.end = 0;
            }
            if written > 0 {
                return Ok(written);
            }
            let n = self.reader.read(&mut self.bytes)?;
            if n == 0 {
                return Ok(0);
            }
            self.start = 0;
            self.end = n;
        }
    }
}

/// Serve IQ frames over TCP.
///
/// Accepts a single connection on `listener`, reads raw bytes, parses them into
/// complex samples with `format`, and invokes `on_frame` with each batch.
/// Samples split across reads are reassembled, so `on_frame` always observes
/// the complete stream in order. Returns when the connection closes or an I/O
/// error occurs.
///
/// # Errors
///
/// Returns the underlying async I/O error.
#[cfg(feature = "tcp")]
pub async fn serve_iq(
    listener: tokio::net::TcpListener,
    format: IqFormat,
    mut on_frame: impl FnMut(&[Complex32]),
) -> std::io::Result<()> {
    use tokio::io::AsyncReadExt;

    let (mut socket, _addr) = listener.accept().await?;
    let mut reassembler = IqReassembler::new(format);
    let mut bytes = vec![0u8; DEFAULT_CAPACITY];
    let mut out = vec![Complex32::default(); DEFAULT_CAPACITY / format.bytes_per_sample()];

    loop {
        let n = socket.read(&mut bytes).await?;
        if n == 0 {
            break; // clean EOF
        }
        let mut offset = 0;
        while offset < n {
            let res = reassembler.push(&bytes[offset..n], &mut out);
            if res.samples > 0 {
                on_frame(&out[..res.samples]);
            }
            if res.consumed == 0 {
                break;
            }
            offset += res.consumed;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reader that hands out the payload in a repeating cycle of chunk
    /// sizes, mimicking TCP segmentation that ignores sample boundaries.
    struct ChunkedReader {
        data: Vec<u8>,
        pos: usize,
        sizes: Vec<usize>,
        next: usize,
    }

    impl ChunkedReader {
        fn new(data: Vec<u8>, sizes: &[usize]) -> Self {
            Self {
                data,
                pos: 0,
                sizes: sizes.to_vec(),
                next: 0,
            }
        }
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.data.len() {
                return Ok(0);
            }
            let want = self.sizes[self.next % self.sizes.len()];
            self.next += 1;
            let n = want.min(buf.len()).min(self.data.len() - self.pos);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    pub(super) fn u8_pattern(samples: usize) -> (Vec<u8>, Vec<Complex32>) {
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

    fn collect_all(source: &mut impl IqSource, chunk: usize) -> Vec<Complex32> {
        let mut buf = vec![Complex32::default(); chunk];
        let mut got = Vec::new();
        loop {
            let n = source.recv(&mut buf).unwrap();
            if n == 0 {
                return got;
            }
            got.extend_from_slice(&buf[..n]);
        }
    }

    #[test]
    fn rtl_tcp_greeting_is_parsed_and_skipped() {
        let mut payload = b"RTL0".to_vec();
        payload.extend_from_slice(&5u32.to_be_bytes());
        payload.extend_from_slice(&29u32.to_be_bytes());
        let (bytes, expect) = u8_pattern(300);
        payload.extend_from_slice(&bytes);

        let mut reader = std::io::Cursor::new(payload);
        let info = read_rtl_tcp_greeting(&mut reader).unwrap();
        assert_eq!(
            info,
            RtlTcpInfo {
                tuner: 5,
                gain_count: 29
            }
        );
        // The first sample after the greeting is the first I/Q pair.
        let mut source = TcpIqSource::new(reader, IqFormat::U8);
        assert_eq!(collect_all(&mut source, 13), expect);
    }

    #[test]
    fn rtl_tcp_greeting_rejects_other_streams() {
        let mut reader = std::io::Cursor::new(
            b"HTTP/1.1 200 OK
"
            .to_vec(),
        );
        let err = read_rtl_tcp_greeting(&mut reader).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        // A stream that ends inside the greeting is an error, not a panic.
        let mut short = std::io::Cursor::new(b"RTL0\0\0".to_vec());
        assert!(read_rtl_tcp_greeting(&mut short).is_err());
    }

    #[test]
    fn source_reassembles_samples_split_across_reads() {
        let (bytes, expect) = u8_pattern(1000);
        // Odd read sizes put a boundary between the I and the Q byte.
        let reader = ChunkedReader::new(bytes, &[1, 3, 7, 13, 31]);
        let mut source = TcpIqSource::with_capacity(reader, IqFormat::U8, 64);
        let got = collect_all(&mut source, 7);
        assert_eq!(got.len(), expect.len(), "no sample may be dropped");
        assert_eq!(got, expect, "samples must stay complete and in order");
        assert_eq!(source.pending_bytes(), 0);
    }

    #[test]
    fn source_reassembles_wide_samples_and_survives_tiny_reads() {
        let mut bytes = Vec::new();
        let mut expect = Vec::new();
        for i in 0..256 {
            let re = i as f32 * 0.125;
            let im = 1.0 - i as f32 * 0.25;
            bytes.extend_from_slice(&re.to_le_bytes());
            bytes.extend_from_slice(&im.to_le_bytes());
            expect.push(Complex32::new(re, im));
        }
        // One byte per read splits every 8-byte sample seven times.
        let reader = ChunkedReader::new(bytes, &[1]);
        let mut source = TcpIqSource::new(reader, IqFormat::F32Le);
        assert_eq!(collect_all(&mut source, 16), expect);
    }

    #[test]
    fn source_drains_buffer_before_reading_again() {
        let (bytes, expect) = u8_pattern(64);
        let reader = ChunkedReader::new(bytes, &[128]);
        let mut source = TcpIqSource::new(reader, IqFormat::U8);
        let mut buf = [Complex32::default(); 3];
        let mut got = Vec::new();
        loop {
            let n = source.recv(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            assert!(n <= 3);
            got.extend_from_slice(&buf[..n]);
        }
        assert_eq!(got, expect);
    }

    #[test]
    fn source_reports_eof_as_zero() {
        let reader = ChunkedReader::new(Vec::new(), &[8]);
        let mut source = TcpIqSource::new(reader, IqFormat::U8);
        let mut buf = [Complex32::default(); 4];
        assert_eq!(source.recv(&mut buf).unwrap(), 0);
    }

    #[test]
    fn connect_streams_from_a_real_socket() {
        use std::io::Write;

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (bytes, expect) = u8_pattern(500);
        let server = std::thread::spawn(move || {
            let (mut stream, _peer) = listener.accept().unwrap();
            // Odd-sized flushed writes so segments can end mid-sample.
            for chunk in bytes.chunks(3) {
                stream.write_all(chunk).unwrap();
                stream.flush().unwrap();
            }
        });

        let mut source = TcpIqSource::connect(addr, IqFormat::U8).unwrap();
        let got = collect_all(&mut source, 16);
        server.join().unwrap();
        assert_eq!(got, expect);
    }
}

#[cfg(all(test, feature = "tcp"))]
mod async_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWriteExt;
    use tokio::net::{TcpListener, TcpStream};

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap()
    }

    #[test]
    fn tcp_iq_roundtrip_parses_samples() {
        runtime().block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            // (0,0), (+1,-1), (-1,+1) as U8 I/Q.
            let bytes = [128u8, 128, 255, 0, 0, 255];

            let collected: Arc<Mutex<Vec<Complex32>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&collected);
            let server = tokio::spawn(async move {
                serve_iq(listener, IqFormat::U8, move |samples| {
                    sink.lock().unwrap().extend_from_slice(samples);
                })
                .await
                .ok();
            });

            let mut client = TcpStream::connect(addr).await.unwrap();
            client.write_all(&bytes).await.unwrap();
            client.flush().await.unwrap();
            // Let the server read, then close the connection.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(client);
            let _ = server.await;

            let got = collected.lock().unwrap();
            assert_eq!(got.len(), 3, "parsed samples: {got:?}");
            // U8 maps 255→+127/128, 0→-1.0.
            assert!((got[1].re - 127.0 / 128.0).abs() < 1e-6 && (got[1].im + 1.0).abs() < 1e-6);
        });
    }

    #[test]
    fn serve_iq_reassembles_writes_split_mid_sample() {
        runtime().block_on(async {
            let samples = 300;
            let (bytes, expect) = super::tests::u8_pattern(samples);

            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let collected: Arc<Mutex<Vec<Complex32>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&collected);
            let server = tokio::spawn(async move {
                serve_iq(listener, IqFormat::U8, move |frame| {
                    sink.lock().unwrap().extend_from_slice(frame);
                })
                .await
                .ok();
            });

            let mut client = TcpStream::connect(addr).await.unwrap();
            // Odd-sized flushed writes force the server to read half a sample.
            let sizes = [1usize, 3, 7, 5, 11];
            let mut pos = 0;
            let mut next = 0;
            while pos < bytes.len() {
                let take = sizes[next % sizes.len()].min(bytes.len() - pos);
                next += 1;
                client.write_all(&bytes[pos..pos + take]).await.unwrap();
                client.flush().await.unwrap();
                pos += take;
                tokio::task::yield_now().await;
            }
            drop(client);
            let _ = server.await;

            let got = collected.lock().unwrap();
            assert_eq!(got.len(), samples, "every sample must survive");
            assert_eq!(*got, expect, "samples must stay complete and in order");
        });
    }
}
