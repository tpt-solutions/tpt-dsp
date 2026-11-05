//! IQ parsing and WAV round-trip benchmarks for `tpt-dsp-io`.
//!
//! Run with `cargo bench -p tpt-dsp-io --bench io_bench`.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::io::Cursor;
use tpt_dsp_core::Complex32;
use tpt_dsp_io::{
    parse_iq, read_wav_f32_reader, write_wav_f32_writer, IqFormat, SampleFormat, WavSpec,
};

const SAMPLES: usize = 1 << 16;

fn bench_parse_iq(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_iq");
    group.throughput(Throughput::Elements(SAMPLES as u64));
    let cases: [(&str, IqFormat, usize); 2] =
        [("u8", IqFormat::U8, 2), ("i16le", IqFormat::I16Le, 4)];
    for (name, format, bytes_per_sample) in cases {
        let bytes = vec![0x80u8; SAMPLES * bytes_per_sample];
        let mut out = vec![Complex32::new(0.0, 0.0); SAMPLES];
        group.bench_with_input(BenchmarkId::new(name, SAMPLES), &bytes, |b, bytes| {
            b.iter(|| parse_iq(format, black_box(bytes), &mut out))
        });
    }
    group.finish();
}

fn bench_wav_round_trip(c: &mut Criterion) {
    let samples: Vec<f32> = (0..SAMPLES).map(|i| (i as f32 * 0.01).sin()).collect();
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: SampleFormat::Float,
    };
    let mut group = c.benchmark_group("wav");
    group.throughput(Throughput::Elements(SAMPLES as u64));
    group.bench_function("write_f32", |b| {
        b.iter(|| {
            let mut buf = Vec::with_capacity(SAMPLES * 4 + 64);
            write_wav_f32_writer(&mut buf, &spec, black_box(&samples)).unwrap();
            buf
        })
    });
    let mut encoded = Vec::new();
    write_wav_f32_writer(&mut encoded, &spec, &samples).unwrap();
    group.bench_function("read_f32", |b| {
        b.iter(|| read_wav_f32_reader(Cursor::new(black_box(&encoded))).unwrap())
    });
    group.finish();
}

criterion_group!(benches, bench_parse_iq, bench_wav_round_trip);
criterion_main!(benches);
