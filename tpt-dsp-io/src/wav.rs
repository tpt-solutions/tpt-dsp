//! Minimal built-in RIFF/WAVE (`wav`) reader and writer.
//!
//! This module replaces the external `hound` dependency (Apache-2.0-only) so
//! that the default build of the framework carries only MIT/Apache-2.0
//! licensed code. It covers what a DSP pipeline needs:
//!
//! - Reading PCM integer data at 8/16/24/32 bits per sample (8-bit is unsigned
//!   per the spec; 24-bit is sign-extended) and IEEE float at 32/64 bits,
//!   including `WAVE_FORMAT_EXTENSIBLE` headers.
//! - Writing 32-bit IEEE float files with correct RIFF chunk sizes.
//!
//! Samples are normalised to `f32` in `[-1, 1]` on read; integer formats are
//! scaled by `2^(bits-1)` exactly like the rest of the framework expects.
//!
//! The reader loads the whole `data` chunk into memory; this is a file-format
//! utility module, not a streaming path, so that keeps the API simple.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

/// Sample encoding declared by a [`WavSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// Linear PCM integers (8-bit unsigned, 16/24/32-bit two's complement).
    Int,
    /// IEEE 754 floats (32 or 64 bit).
    Float,
}

/// Header metadata for a WAV file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    /// Number of interleaved channels (>= 1).
    pub channels: u16,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Bits per sample (8/16/24/32 for `Int`, 32/64 for `Float`).
    pub bits_per_sample: u16,
    /// How samples are encoded.
    pub sample_format: SampleFormat,
}

/// Decoded WAV audio: header plus interleaved `f32` samples in `[-1, 1]`.
#[derive(Debug, Clone)]
pub struct WavData {
    /// Parsed header metadata.
    pub spec: WavSpec,
    /// Interleaved samples, `frames * channels` long, in `[-1, 1]`.
    pub interleaved: Vec<f32>,
}

/// Errors produced while reading or writing WAV files.
#[derive(Debug)]
pub enum WavError {
    /// Underlying I/O failure.
    Io(std::io::Error),
    /// The byte stream is not a WAV file this reader understands.
    Invalid(&'static str),
    /// A header field is outside the supported range.
    Unsupported(String),
}

impl fmt::Display for WavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WavError::Io(e) => write!(f, "i/o error: {e}"),
            WavError::Invalid(msg) => write!(f, "invalid wav file: {msg}"),
            WavError::Unsupported(msg) => write!(f, "unsupported wav feature: {msg}"),
        }
    }
}

impl std::error::Error for WavError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WavError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for WavError {
    fn from(e: std::io::Error) -> Self {
        WavError::Io(e)
    }
}

/// Read a WAV file from disk into normalised `f32` samples.
///
/// # Errors
/// Returns [`WavError`] if the file is missing, malformed, or uses an
/// unsupported encoding.
pub fn read_wav_f32_path(path: &Path) -> Result<WavData, WavError> {
    let file = File::open(path)?;
    read_wav_f32_reader(std::io::BufReader::new(file))
}

/// Read a WAV file from any reader positioned at the start of the stream.
///
/// # Errors
/// Returns [`WavError`] if the stream is malformed or unsupported.
pub fn read_wav_f32_reader<R: Read + Seek>(mut reader: R) -> Result<WavData, WavError> {
    let mut riff = [0u8; 12];
    reader.read_exact(&mut riff)?;
    if &riff[0..4] != b"RIFF" || &riff[8..12] != b"WAVE" {
        return Err(WavError::Invalid("missing RIFF/WAVE magic"));
    }

    let mut spec: Option<WavSpec> = None;
    let mut data: Option<Vec<u8>> = None;

    loop {
        let mut header = [0u8; 8];
        if read_fill(&mut reader, &mut header)? == 0 {
            break; // clean EOF after the last chunk
        }
        let id = [header[0], header[1], header[2], header[3]];
        let size = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

        if &id == b"data" {
            let mut bytes = vec![0u8; size];
            reader.read_exact(&mut bytes)?;
            data = Some(bytes);
        } else if &id == b"fmt " {
            let mut chunk = vec![0u8; size];
            reader.read_exact(&mut chunk)?;
            spec = Some(parse_fmt(&chunk)?);
        } else {
            // Unknown chunk: skip it, honouring the even-padding rule.
            let skip = size + (size & 1);
            reader.seek(SeekFrom::Current(skip as i64))?;
        }
    }

    let spec = spec.ok_or(WavError::Invalid("missing `fmt ` chunk"))?;
    let raw = data.ok_or(WavError::Invalid("missing `data` chunk"))?;
    let interleaved = decode_samples(&spec, &raw)?;
    Ok(WavData { spec, interleaved })
}

/// Write interleaved `f32` samples as a 32-bit IEEE float WAV file to disk.
///
/// `interleaved` must hold exactly `frames * spec.channels` samples; extra
/// frames are truncated to the shortest channel by the caller if needed.
///
/// # Errors
/// Returns [`WavError`] if the file cannot be created or written.
pub fn write_wav_f32_path(
    path: &Path,
    spec: &WavSpec,
    interleaved: &[f32],
) -> Result<(), WavError> {
    let file = File::create(path)?;
    write_wav_f32_writer(std::io::BufWriter::new(file), spec, interleaved)
}

/// Write interleaved `f32` samples as 32-bit IEEE float WAV to any writer.
///
/// # Errors
/// Returns [`WavError`] if the underlying writer fails or `spec` is invalid.
pub fn write_wav_f32_writer<W: Write>(
    mut writer: W,
    spec: &WavSpec,
    interleaved: &[f32],
) -> Result<(), WavError> {
    if spec.channels == 0 {
        return Err(WavError::Invalid("zero channels"));
    }
    if spec.sample_format != SampleFormat::Float || spec.bits_per_sample != 32 {
        return Err(WavError::Unsupported(
            "the writer emits 32-bit IEEE float only".into(),
        ));
    }
    let data_len = (interleaved.len() * 4) as u32;
    writer.write_all(b"RIFF")?;
    writer.write_all(&(36 + data_len).to_le_bytes())?;
    writer.write_all(b"WAVE")?;
    writer.write_all(b"fmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&3u16.to_le_bytes())?; // IEEE float
    writer.write_all(&spec.channels.to_le_bytes())?;
    writer.write_all(&spec.sample_rate.to_le_bytes())?;
    let byte_rate = spec.sample_rate * spec.channels as u32 * 4;
    writer.write_all(&byte_rate.to_le_bytes())?;
    writer.write_all(&(spec.channels * 4).to_le_bytes())?; // block align
    writer.write_all(&32u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&data_len.to_le_bytes())?;
    for sample in interleaved {
        writer.write_all(&sample.to_le_bytes())?;
    }
    writer.flush()?;
    Ok(())
}

fn parse_fmt(chunk: &[u8]) -> Result<WavSpec, WavError> {
    if chunk.len() < 16 {
        return Err(WavError::Invalid("`fmt ` chunk too short"));
    }
    let audio_format = u16::from_le_bytes([chunk[0], chunk[1]]);
    let channels = u16::from_le_bytes([chunk[2], chunk[3]]);
    let sample_rate = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
    let bits_per_sample = u16::from_le_bytes([chunk[14], chunk[15]]);

    // WAVE_FORMAT_EXTENSIBLE: the real format code lives in the first two
    // bytes of the SubFormat GUID at offset 24.
    let effective_format = if audio_format == 0xFFFE {
        if chunk.len() < 26 {
            return Err(WavError::Invalid("extensible `fmt ` too short"));
        }
        u16::from_le_bytes([chunk[24], chunk[25]])
    } else {
        audio_format
    };

    let sample_format = match effective_format {
        1 => SampleFormat::Int,
        3 => SampleFormat::Float,
        other => {
            return Err(WavError::Unsupported(format!(
                "format code {other} (only PCM=1 / IEEE float=3)"
            )));
        }
    };
    if channels == 0 {
        return Err(WavError::Invalid("zero channels"));
    }
    if sample_rate == 0 {
        return Err(WavError::Invalid("zero sample rate"));
    }
    Ok(WavSpec {
        channels,
        sample_rate,
        bits_per_sample,
        sample_format,
    })
}

fn decode_samples(spec: &WavSpec, raw: &[u8]) -> Result<Vec<f32>, WavError> {
    let bytes = (spec.bits_per_sample as usize) / 8;
    if bytes == 0 || spec.bits_per_sample % 8 != 0 {
        return Err(WavError::Unsupported(format!(
            "bits per sample {}",
            spec.bits_per_sample
        )));
    }
    let frames = raw.len() / (bytes * spec.channels as usize);
    let mut out = Vec::with_capacity(frames * spec.channels as usize);
    match spec.sample_format {
        SampleFormat::Float => match spec.bits_per_sample {
            32 => {
                for chunk in raw.chunks_exact(4) {
                    out.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
            }
            64 => {
                for chunk in raw.chunks_exact(8) {
                    let v = f64::from_le_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                        chunk[7],
                    ]);
                    out.push(v as f32);
                }
            }
            other => return Err(WavError::Unsupported(format!("float width {other}"))),
        },
        SampleFormat::Int => match spec.bits_per_sample {
            8 => {
                for b in raw {
                    out.push((i16::from(*b) - 128) as f32 / 128.0);
                }
            }
            16 => {
                for chunk in raw.chunks_exact(2) {
                    let v = i16::from_le_bytes([chunk[0], chunk[1]]);
                    out.push(f32::from(v) / 32768.0);
                }
            }
            24 => {
                for chunk in raw.chunks_exact(3) {
                    let v = i32::from(chunk[0])
                        | (i32::from(chunk[1]) << 8)
                        | (i32::from(chunk[2]) << 16);
                    let v = (v << 8) >> 8; // sign-extend 24 -> 32 bits
                    out.push(v as f32 / 8_388_608.0);
                }
            }
            32 => {
                for chunk in raw.chunks_exact(4) {
                    let v = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    out.push(v as f32 / 2_147_483_648.0);
                }
            }
            other => return Err(WavError::Unsupported(format!("pcm width {other}"))),
        },
    }
    Ok(out)
}

/// Read exactly `buf.len()` bytes; returns how many were read (only ever 0 at
/// a clean EOF before anything was consumed).
fn read_fill<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FLOAT_SPEC: WavSpec = WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };

    #[test]
    fn float_roundtrip_is_exact() {
        let samples: Vec<f32> = (0..256).map(|i| (i as f32 * 0.02).sin()).collect();
        let mut buf = Cursor::new(Vec::new());
        write_wav_f32_writer(&mut buf, &FLOAT_SPEC, &samples).unwrap();
        buf.set_position(0);
        let back = read_wav_f32_reader(buf).unwrap();
        assert_eq!(back.spec, FLOAT_SPEC);
        assert_eq!(back.interleaved.len(), samples.len());
        assert_eq!(back.interleaved, samples);
    }

    #[test]
    fn path_roundtrip_matches_hound_semantics() {
        let dir = std::env::temp_dir();
        let path = dir.join("tpt_dsp_io_wav_test.wav");
        let samples: Vec<f32> = (0..100).map(|i| (i as f32 * 0.01).sin()).collect();
        write_wav_f32_path(&path, &FLOAT_SPEC, &samples).unwrap();
        let back = read_wav_f32_path(&path).unwrap();
        assert_eq!(back.spec, FLOAT_SPEC);
        assert_eq!(back.interleaved, samples);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn pcm16_roundtrip_normalises_to_unit_range() {
        let values: Vec<i16> = vec![i16::MIN, -16384, 0, 16384, i16::MAX];
        let mut bytes = Vec::new();
        for v in &values {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        let decoded = decode_samples(
            &WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            },
            &bytes,
        )
        .unwrap();
        assert_eq!(decoded[0], -1.0);
        assert_eq!(decoded[4], 32767.0 / 32768.0);
    }

    #[test]
    fn pcm24_sign_extension_works() {
        let decoded = decode_samples(
            &WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 24,
                sample_format: SampleFormat::Int,
            },
            &[0x00, 0x00, 0x80, 0xFF, 0xFF, 0x7F],
        )
        .unwrap();
        assert_eq!(decoded[0], -1.0); // 0x800000 = i24 MIN
        assert_eq!(decoded[1], (0x7FFFFF as f32) / 8_388_608.0); // i24 MAX
    }

    #[test]
    fn pcm8_unsigned_maps_to_unit_range() {
        let decoded = decode_samples(
            &WavSpec {
                channels: 1,
                sample_rate: 44_100,
                bits_per_sample: 8,
                sample_format: SampleFormat::Int,
            },
            &[0, 128, 255],
        )
        .unwrap();
        assert_eq!(decoded, vec![-1.0, 0.0, 127.0 / 128.0]);
    }

    #[test]
    fn unknown_chunks_are_skipped_and_padding_honoured() {
        let samples: Vec<f32> = vec![0.25, -0.5];
        let mut buf = Cursor::new(Vec::new());
        write_wav_f32_writer(
            &mut buf,
            &WavSpec {
                channels: 1,
                ..FLOAT_SPEC
            },
            &samples,
        )
        .unwrap();
        let mut bytes = buf.into_inner();
        // Inject a 3-byte (odd-sized) unknown chunk between `fmt ` and `data`
        // right after the 36-byte standard header.
        let mut injected = Vec::new();
        injected.extend_from_slice(&bytes[..36]);
        injected.extend_from_slice(b"JUNK");
        injected.extend_from_slice(&3u32.to_le_bytes());
        injected.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        injected.push(0x00); // even padding byte
        injected.extend_from_slice(&bytes[36..]);
        bytes = injected;
        // Fix the RIFF size (original + 8 header + 3 payload + 1 pad).
        let riff_len = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&riff_len.to_le_bytes());
        let back = read_wav_f32_reader(Cursor::new(bytes)).unwrap();
        assert_eq!(back.interleaved, samples);
    }
    #[test]
    fn extensible_float_header_is_accepted() {
        // Build a WAVE_FORMAT_EXTENSIBLE float32 file from scratch.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes()); // patched below
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(b"fmt ");
        bytes.extend_from_slice(&40u32.to_le_bytes()); // fmt chunk size
        bytes.extend_from_slice(&0xFFFEu16.to_le_bytes()); // extensible tag
        bytes.extend_from_slice(&1u16.to_le_bytes()); // channels
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&(48_000u32 * 4).to_le_bytes()); // byte rate
        bytes.extend_from_slice(&4u16.to_le_bytes()); // block align
        bytes.extend_from_slice(&32u16.to_le_bytes()); // bits per sample
        bytes.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        bytes.extend_from_slice(&32u16.to_le_bytes()); // valid bits
        bytes.extend_from_slice(&0u32.to_le_bytes()); // channel mask
        let guid: [u8; 16] = [
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ];
        bytes.extend_from_slice(&guid);
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        let riff_len = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&riff_len.to_le_bytes());
        let back = read_wav_f32_reader(Cursor::new(bytes)).unwrap();
        assert_eq!(back.spec.sample_format, SampleFormat::Float);
        assert_eq!(back.interleaved, vec![0.5]);
    }

    #[test]
    fn truncated_file_is_rejected() {
        let samples = [0.5f32, -0.5];
        let mut buf = Cursor::new(Vec::new());
        write_wav_f32_writer(
            &mut buf,
            &WavSpec {
                channels: 1,
                ..FLOAT_SPEC
            },
            &samples,
        )
        .unwrap();
        let mut bytes = buf.into_inner();
        bytes.truncate(bytes.len() - 3);
        assert!(matches!(
            read_wav_f32_reader(Cursor::new(bytes)),
            Err(WavError::Io(_))
        ));
    }

    #[test]
    fn non_wav_magic_is_rejected() {
        let err = read_wav_f32_reader(Cursor::new(b"NOTAWAVEFILE".to_vec())).unwrap_err();
        assert!(matches!(err, WavError::Invalid(_)));
    }

    #[test]
    fn writer_rejects_non_float_spec() {
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut buf = Cursor::new(Vec::new());
        assert!(write_wav_f32_writer(&mut buf, &spec, &[0.0]).is_err());
    }
}
