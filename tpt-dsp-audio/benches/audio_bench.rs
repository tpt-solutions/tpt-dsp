//! Benchmark suite for `tpt-dsp-audio` hot paths.
//!
//! Run with `cargo bench -p tpt-dsp-audio`. Tracks throughput of the
//! real-time effects (convolution reverb, multi-band EQ) and synthesis so
//! allocation or algorithmic regressions are caught early.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tpt_dsp_audio::{generate_decay_ir, ConvolutionReverb, Eq};

fn make_block(n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.03).sin()).collect()
}

fn bench_reverb(c: &mut Criterion) {
    let block = 256usize;
    let ir = generate_decay_ir(4_096, 48_000.0, 0.5);
    let mut reverb = ConvolutionReverb::new(&ir, block);
    let mut src = make_block(block);
    let mut out = vec![0.0f32; block];
    c.bench_function("convolution_reverb_256", |b| {
        b.iter(|| {
            reverb.process(black_box(&src), black_box(&mut out));
        })
    });
    let _ = &mut src;
}

fn bench_eq(c: &mut Criterion) {
    let block = 256usize;
    let bands = [(100.0, 0.0, 1.0), (1_000.0, 6.0, 1.0), (8_000.0, -3.0, 0.7)];
    let mut eq = Eq::new(48_000.0, &bands);
    let mut buf = make_block(block);
    c.bench_function("eq_3band_256", |b| {
        b.iter(|| {
            let mut b = buf.clone();
            eq.process(black_box(&mut b));
        })
    });
    let _ = &mut buf;
}

criterion_group!(benches, bench_reverb, bench_eq);
criterion_main!(benches);
