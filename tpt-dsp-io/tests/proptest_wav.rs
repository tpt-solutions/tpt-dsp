//! Property tests for the WAV reader and writer.
//!
//! - A written float file reads back exactly, for any channel count and length.
//! - The reader never panics on arbitrary bytes: it returns `Ok` or an error.
//!   This is a cheap fuzz target for the parser.

use std::io::Cursor;

use proptest::prelude::*;
use tpt_dsp_io::{read_wav_f32_reader, write_wav_f32_writer, SampleFormat, WavSpec};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Any float buffer whose length is a whole number of frames round-trips
    /// exactly.
    #[test]
    fn float_wav_round_trips_exactly(
        channels in 1u16..=4,
        frames in 0usize..=300,
        rate in 1u32..=192_000,
        seed in prop::collection::vec(any::<f32>().prop_filter("finite", |v| v.is_finite()), 1..64),
    ) {
        let n = frames * channels as usize;
        let samples: Vec<f32> = (0..n).map(|i| seed[i % seed.len()]).collect();
        let spec = WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: SampleFormat::Float,
        };
        let mut buf = Cursor::new(Vec::new());
        write_wav_f32_writer(&mut buf, &spec, &samples).unwrap();
        buf.set_position(0);
        let back = read_wav_f32_reader(buf).unwrap();
        prop_assert_eq!(back.spec, spec);
        prop_assert_eq!(back.interleaved, samples);
    }

    /// Arbitrary bytes never panic the reader.
    #[test]
    fn reader_never_panics_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let _ = read_wav_f32_reader(Cursor::new(bytes));
    }

    /// Arbitrary bytes after a valid RIFF/WAVE header never panic the reader
    /// either. This reaches the chunk and format parsers more often than the
    /// previous property does.
    #[test]
    fn reader_never_panics_after_a_riff_header(
        body in prop::collection::vec(any::<u8>(), 0..512),
    ) {
        let mut bytes = Vec::with_capacity(12 + body.len());
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(b"WAVE");
        bytes.extend_from_slice(&body);
        let _ = read_wav_f32_reader(Cursor::new(bytes));
    }
}
