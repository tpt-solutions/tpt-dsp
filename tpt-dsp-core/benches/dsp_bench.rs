//! Benchmark suite for `tpt-dsp-core`.
//!
//! Run with `cargo bench -p tpt-dsp-core`. These micro-benchmarks track the
//! throughput of the hot-path primitives (FFT, FIR, convolution, biquad) so
//! regressions in the real-time path are caught early.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use num_complex::Complex;
use tpt_dsp_core::{convolve, Biquad, BiquadType, FirDesign, IirFilter};

fn make_signal(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 * 0.01).sin() + 0.3 * (i as f32 * 0.13).cos())
        .collect()
}

fn bench_fft(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft");
    for &n in &[256usize, 1024, 4096] {
        group.throughput(Throughput::Elements(n as u64));
        let input: Vec<Complex<f32>> = make_signal(n)
            .iter()
            .map(|&x| Complex::new(x, 0.0))
            .collect();
        let mut spectrum = vec![Complex::new(0.0f32, 0.0); n];
        let mut scratch = vec![Complex::new(0.0f32, 0.0); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                tpt_dsp_core::fft(
                    black_box(&input),
                    black_box(&mut spectrum),
                    black_box(&mut scratch),
                );
            })
        });
    }
    group.finish();
}

fn bench_fir(c: &mut Criterion) {
    let fir = FirDesign::LowPass(0.05).design::<f32>(127);
    let mut src = make_signal(1024);
    let mut out = vec![0.0f32; 1024];
    c.bench_function("fir_127_lowpass_1024", |b| {
        b.iter(|| {
            let mut f = fir.clone();
            f.process(black_box(&src), black_box(&mut out));
        })
    });
    // keep the borrow alive
    let _ = &mut src;
}

fn bench_iir(c: &mut Criterion) {
    let s1 = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
    let s2 = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 4_000.0, 0.707, 0.0);
    let mut filter = IirFilter::new(vec![s1, s2]);
    let mut src = make_signal(1024);
    let mut out = vec![0.0f32; 1024];
    c.bench_function("iir_2stage_1024", |b| {
        b.iter(|| {
            filter.reset();
            filter.process(black_box(&src), black_box(&mut out));
        })
    });
    let _ = &mut src;
}

fn bench_convolve(c: &mut Criterion) {
    let n = 1024usize;
    let signal = make_signal(n);
    let kernel: Vec<f32> = (0..64).map(|i| (i as f32 * 0.1).sin().exp()).collect();
    let mut out = vec![0.0f32; n + kernel.len() - 1];
    c.bench_function("convolve_direct_1024x64", |b| {
        b.iter(|| {
            convolve(black_box(&signal), black_box(&kernel), black_box(&mut out));
        })
    });
}

fn bench_biquad(c: &mut Criterion) {
    let mut bq = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
    let mut src = make_signal(256);
    let mut out = vec![0.0f32; 256];
    c.bench_function("biquad_process_256", |b| {
        b.iter(|| {
            bq.process(black_box(&src), black_box(&mut out));
        })
    });
    let _ = &mut src;
}

criterion_group!(
    benches,
    bench_fft,
    bench_fir,
    bench_iir,
    bench_convolve,
    bench_biquad
);
criterion_main!(benches);
