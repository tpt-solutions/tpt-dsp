//! Transform, window and lock-free buffer benchmarks for `tpt-dsp-core`.
//!
//! Run with `cargo bench -p tpt-dsp-core --bench dsp_bench`.
//!
//! The `complex/simd_*` and `fft/radix2_f32_simd` groups exercise
//! [`tpt_dsp_core::simd`], which is scalar by default. Compare against the
//! vectorised path with
//! `cargo +nightly bench -p tpt-dsp-core --features simd --bench dsp_bench`.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tpt_dsp_core::{
    complex_add_simd, complex_mul_simd, dct_ii, dct_iii, dct_iv, exp_i, fft, fft_inplace,
    fft_inplace_f32, hilbert, ifft_inplace, magnitude, magnitude_simd, magnitude_squared, phase,
    rotate, twiddles, windowed, FftPlan, FmDemodulator, RingBuffer, SpscQueue, WindowType, C32,
    C64,
};

const FFT_SIZES: [usize; 5] = [128, 256, 1024, 4096, 16384];
const DCT_SIZES: [usize; 3] = [64, 256, 1024];
const HILBERT_SIZES: [usize; 3] = [256, 1024, 4096];

fn signal_f32(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 * 0.01).sin() + 0.3 * (i as f32 * 0.13).cos())
        .collect()
}

fn signal_f64(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| (i as f64 * 0.01).sin() + 0.3 * (i as f64 * 0.13).cos())
        .collect()
}

fn complex_f32(n: usize) -> Vec<C32> {
    signal_f32(n).iter().map(|&x| C32::new(x, 0.0)).collect()
}

fn bench_fft_radix2(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/radix2_f32");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut spectrum = vec![C32::default(); n];
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                fft(
                    black_box(&input),
                    black_box(&mut spectrum),
                    black_box(&mut scratch),
                )
            })
        });
    }
    group.finish();
}

fn bench_fft_radix2_inplace(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/radix2_inplace_f32");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut work = input.clone();
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                work.copy_from_slice(&input);
                fft_inplace(black_box(&mut work), black_box(&mut scratch))
            })
        });
    }
    group.finish();
}

fn bench_fft_radix2_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/radix2_f64");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input: Vec<C64> = signal_f64(n).iter().map(|&x| C64::new(x, 0.0)).collect();
        let mut work = input.clone();
        let mut scratch = vec![C64::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                work.copy_from_slice(&input);
                fft_inplace(black_box(&mut work), black_box(&mut scratch))
            })
        });
    }
    group.finish();
}

fn bench_ifft_radix2(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/radix2_inverse_f32");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut work = input.clone();
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                work.copy_from_slice(&input);
                ifft_inplace(black_box(&mut work), black_box(&mut scratch))
            })
        });
    }
    group.finish();
}

fn bench_fft_plan(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/rustfft_plan_f32");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut out = vec![C32::default(); n];
        let mut plan = FftPlan::new_forward(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| plan.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_fft_plan_nonpow2(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/rustfft_plan_nonpow2_f32");
    for &n in [768usize, 1000, 4200].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut out = vec![C32::default(); n];
        let mut plan = FftPlan::new_forward(n);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| plan.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_twiddles(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/twiddles_f32");
    for &n in [1024usize, 4096].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| twiddles(black_box(n), black_box(&mut scratch)))
        });
    }
    group.finish();
}

fn bench_dct(c: &mut Criterion) {
    let mut group = c.benchmark_group("dct");
    for &n in DCT_SIZES.iter() {
        let input = signal_f32(n);
        let mut out = vec![0.0f32; n];
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("dct_ii_f32", n), &n, |b, _| {
            b.iter(|| dct_ii(black_box(&input), black_box(&mut out)))
        });
        group.bench_with_input(BenchmarkId::new("dct_iii_f32", n), &n, |b, _| {
            b.iter(|| dct_iii(black_box(&input), black_box(&mut out)))
        });
        group.bench_with_input(BenchmarkId::new("dct_iv_f32", n), &n, |b, _| {
            b.iter(|| dct_iv(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_hilbert(c: &mut Criterion) {
    let mut group = c.benchmark_group("hilbert_f32");
    for &n in HILBERT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = signal_f32(n);
        let mut out = vec![0.0f32; n];
        let mut work = vec![C32::default(); n];
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                hilbert(
                    black_box(&input),
                    black_box(&mut out),
                    black_box(&mut work),
                    black_box(&mut scratch),
                )
            })
        });
    }
    group.finish();
}

fn bench_windows(c: &mut Criterion) {
    let n = 4096usize;
    let mut group = c.benchmark_group("window");
    group.throughput(Throughput::Elements(n as u64));
    let mut out = vec![0.0f32; n];
    for (name, kind) in [
        ("hann", WindowType::Hann),
        ("hamming", WindowType::Hamming),
        ("blackman", WindowType::Blackman),
    ] {
        group.bench_function(BenchmarkId::new(name, n), |b| {
            b.iter(|| windowed(black_box(kind), black_box(n), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_complex_ops(c: &mut Criterion) {
    let n = 4096usize;
    let mut group = c.benchmark_group("complex");
    group.throughput(Throughput::Elements(n as u64));
    let input = complex_f32(n);
    let mut out = vec![0.0f32; n];

    group.bench_function("magnitude_squared", |b| {
        b.iter(|| {
            for (o, &z) in out.iter_mut().zip(input.iter()) {
                *o = magnitude_squared(black_box(z));
            }
        })
    });
    group.bench_function("magnitude", |b| {
        b.iter(|| {
            for (o, &z) in out.iter_mut().zip(input.iter()) {
                *o = magnitude(black_box(z));
            }
        })
    });
    group.bench_function("phase", |b| {
        b.iter(|| {
            for (o, &z) in out.iter_mut().zip(input.iter()) {
                *o = phase(black_box(z));
            }
        })
    });
    group.bench_function("exp_i", |b| {
        b.iter(|| {
            for (o, &z) in out.iter_mut().zip(input.iter()) {
                *o = exp_i(black_box(z.re)).re;
            }
        })
    });
    group.bench_function("rotate", |b| {
        b.iter(|| {
            for (o, &z) in out.iter_mut().zip(input.iter()) {
                *o = rotate(black_box(z), black_box(0.25f32)).im;
            }
        })
    });
    group.finish();
}

fn bench_fm_demod(c: &mut Criterion) {
    let mut group = c.benchmark_group("demod/fm_phase_delta_f32");
    for &n in [256usize, 4096].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input: Vec<C32> = (0..n).map(|i| exp_i(0.031 * i as f32)).collect();
        let mut out = vec![0.0f32; n];
        let mut fm = FmDemodulator::new(1.0f32);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| fm.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_ring_buffer(c: &mut Criterion) {
    let n = 1024usize;
    let mut group = c.benchmark_group("buffers");
    group.throughput(Throughput::Elements(n as u64));

    let mut storage = vec![0.0f32; n + 1];
    let mut ring = RingBuffer::new(&mut storage);
    group.bench_function("ring_push_pop_1024", |b| {
        b.iter(|| {
            for i in 0..n {
                let _ = ring.push(black_box(i as f32));
            }
            for _ in 0..n {
                black_box(ring.pop());
            }
        })
    });

    let queue = SpscQueue::<f32>::bounded(n);
    group.bench_function("spsc_try_send_recv_1024", |b| {
        b.iter(|| {
            for i in 0..n {
                let _ = queue.try_send(black_box(i as f32));
            }
            for _ in 0..n {
                black_box(queue.try_recv().ok());
            }
        })
    });
    group.finish();
}

fn bench_simd_helpers(c: &mut Criterion) {
    let n = 4096usize;
    let mut group = c.benchmark_group("complex");
    group.throughput(Throughput::Elements(n as u64));

    let a = complex_f32(n);
    let b: Vec<C32> = signal_f32(n).iter().map(|&x| C32::new(0.5, x)).collect();
    let mut out_c = vec![C32::default(); n];
    let mut out_f = vec![0.0f32; n];

    group.bench_function("simd_complex_mul", |bch| {
        bch.iter(|| complex_mul_simd(black_box(&a), black_box(&b), black_box(&mut out_c)))
    });
    group.bench_function("simd_complex_add", |bch| {
        bch.iter(|| complex_add_simd(black_box(&a), black_box(&b), black_box(&mut out_c)))
    });
    group.bench_function("simd_magnitude", |bch| {
        bch.iter(|| magnitude_simd(black_box(&a), black_box(&mut out_f)))
    });
    group.finish();
}

fn bench_fft_f32_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("fft/radix2_f32_simd");
    for &n in FFT_SIZES.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = complex_f32(n);
        let mut work = input.clone();
        let mut scratch = vec![C32::default(); n];
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                work.copy_from_slice(&input);
                fft_inplace_f32(black_box(&mut work), black_box(&mut scratch))
            })
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_fft_radix2,
    bench_fft_radix2_inplace,
    bench_fft_radix2_f64,
    bench_ifft_radix2,
    bench_fft_plan,
    bench_fft_plan_nonpow2,
    bench_twiddles,
    bench_dct,
    bench_hilbert,
    bench_windows,
    bench_complex_ops,
    bench_simd_helpers,
    bench_fft_f32_dispatch,
    bench_fm_demod,
    bench_ring_buffer
);
criterion_main!(benches);
