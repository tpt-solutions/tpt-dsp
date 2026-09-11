//! Transform, window and lock-free buffer benchmarks for `tpt-dsp-core`.
//!
//! Run with `cargo bench -p tpt-dsp-core --bench dsp_bench`.
//!
//! The `complex/simd_*` and `fft/radix2_f32_simd` groups exercise
//! [`tpt_dsp_core::simd`], which is scalar by default. Compare against the
//! vectorised path with
//! `cargo +nightly bench -p tpt-dsp-core --features simd --bench dsp_bench`.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use tpt_dsp_core::{
    amdf_pitch, autocorrelation, autocorrelation_pitch, classify_tonal_bins, complex_add_simd,
    complex_mul_simd, dct_ii, dct_iii, dct_iv, exp_i, fft, fft_bin_bark_bands, fft_inplace,
    fft_inplace_f32, hilbert, hz_range_to_period_samples, hz_to_bark, ifft_inplace, imdct,
    levinson_durbin, magnitude, magnitude_simd, magnitude_squared, mdct, nearest_vector,
    nearest_vector_accelerated, noise_shape_step, phase, predict, residual, rotate,
    simultaneous_masking, synthesize, twiddles, windowed, DistanceMetric, FastDctIvPlan,
    FastMdctPlan, FftPlan, FmDemodulator, RingBuffer, SpscQueue, WindowType, C32, C64,
};

const FFT_SIZES: [usize; 5] = [128, 256, 1024, 4096, 16384];
const DCT_SIZES: [usize; 3] = [64, 256, 1024];
const HILBERT_SIZES: [usize; 3] = [256, 1024, 4096];
// Matches Pulse/Aura's MDCT half-sizes (todo.md §5.1's "Initial target
// sizes"), i.e. the block sizes that actually matter for this bench.
const MDCT_SIZES: [usize; 6] = [128, 256, 512, 1024, 2048, 4096];
// Matches Aura's LPC order range (default 16, max 32 — see
// `codecs/aura/src/lib.rs`), against its default block size (1024).
const LPC_ORDERS: [usize; 3] = [8, 16, 32];
const LPC_BLOCK_SIZE: usize = 1024;
// (entries, dim) pairs matching todo.md's own target codebook sizes: Vox
// §31 ("1024 entries") and Whisper §34 ("4096-entry dictionary"). Vector
// dimension isn't pinned by either spec yet, so these are representative
// stand-ins (a CELP-style excitation vector / a small pixel patch).
const VQ_CODEBOOKS: [(usize, usize); 2] = [(1024, 10), (4096, 16)];

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
        let mut fast_plan = FastDctIvPlan::new(n);
        group.bench_with_input(BenchmarkId::new("dct_iv_fast_f32", n), &n, |b, _| {
            b.iter(|| fast_plan.forward(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

/// Direct O(N²) `mdct`/`imdct` vs the FFT-backed `FastMdctPlan` (todo.md
/// §5.1 "Benchmark scalar vs optimized"), at Pulse/Aura's actual block
/// sizes.
fn bench_mdct(c: &mut Criterion) {
    let mut group = c.benchmark_group("mdct");
    for &n in MDCT_SIZES.iter() {
        let two_n = 2 * n;
        let input = signal_f32(two_n);
        let spec_in = signal_f32(n);
        let mut spec_out = vec![0.0f32; n];
        let mut time_out = vec![0.0f32; two_n];
        let mut fast_plan = FastMdctPlan::new(n);

        group.throughput(Throughput::Elements(two_n as u64));
        group.bench_with_input(BenchmarkId::new("forward_direct_f32", n), &n, |b, _| {
            b.iter(|| mdct(black_box(&input), black_box(&mut spec_out)))
        });
        group.bench_with_input(BenchmarkId::new("forward_fast_f32", n), &n, |b, _| {
            b.iter(|| fast_plan.forward(black_box(&input), black_box(&mut spec_out)))
        });
        group.bench_with_input(BenchmarkId::new("inverse_direct_f32", n), &n, |b, _| {
            b.iter(|| imdct(black_box(&spec_in), black_box(&mut time_out)))
        });
        group.bench_with_input(BenchmarkId::new("inverse_fast_f32", n), &n, |b, _| {
            b.iter(|| fast_plan.inverse(black_box(&spec_in), black_box(&mut time_out)))
        });
    }
    group.finish();
}

/// `autocorrelation` + `levinson_durbin` (LPC analysis) and `predict`/
/// `residual`/`synthesize` (todo.md §6 "Add benchmarks"), at Aura's actual
/// LPC orders and block size.
fn bench_lpc(c: &mut Criterion) {
    let input = signal_f64(LPC_BLOCK_SIZE);

    let mut group = c.benchmark_group("lpc/analysis_f64");
    for &order in LPC_ORDERS.iter() {
        let mut r = vec![0.0f64; order + 1];
        let mut coeffs = vec![0.0f64; order];
        let mut scratch = vec![0.0f64; order];
        group.throughput(Throughput::Elements(LPC_BLOCK_SIZE as u64));
        group.bench_with_input(
            BenchmarkId::new("autocorrelation", order),
            &order,
            |b, &order| {
                b.iter(|| autocorrelation(black_box(&input), black_box(order), black_box(&mut r)))
            },
        );
        // Autocorrelation of a real signal decays toward zero at higher
        // lags, so `r` from the loop above is a realistic (not synthetic)
        // input for Levinson-Durbin.
        autocorrelation(&input, order, &mut r);
        group.bench_with_input(
            BenchmarkId::new("levinson_durbin", order),
            &order,
            |b, _| {
                b.iter(|| {
                    levinson_durbin(
                        black_box(&r),
                        black_box(&mut coeffs),
                        black_box(&mut scratch),
                    )
                })
            },
        );
    }
    group.finish();

    let mut group = c.benchmark_group("lpc/predict_residual_synthesize_f64");
    let order = 16usize;
    let coeffs: Vec<f64> = (0..order).map(|i| 0.9 / (i as f64 + 1.0)).collect();
    let mut res = vec![0.0f64; LPC_BLOCK_SIZE];
    let mut back = vec![0.0f64; LPC_BLOCK_SIZE];
    group.throughput(Throughput::Elements(LPC_BLOCK_SIZE as u64));
    group.bench_function(BenchmarkId::new("predict", order), |b| {
        b.iter(|| predict(black_box(&input[..order]), black_box(&coeffs)))
    });
    group.bench_function(BenchmarkId::new("residual", order), |b| {
        b.iter(|| residual(black_box(&input), black_box(&coeffs), black_box(&mut res)))
    });
    residual(&input, &coeffs, &mut res);
    group.bench_function(BenchmarkId::new("synthesize", order), |b| {
        b.iter(|| synthesize(black_box(&res), black_box(&coeffs), black_box(&mut back)))
    });
    group.finish();
}

/// `autocorrelation_pitch`/`amdf_pitch` (todo.md §7 "Benchmark"), at a
/// Vox-realistic 16kHz/40ms speech frame searched over the full
/// SPEECH_MIN/MAX_F0_HZ range.
fn bench_pitch(c: &mut Criterion) {
    let sample_rate = 16_000.0f64;
    let frame = signal_f64(640); // 40ms @ 16kHz
    let (min_p, max_p) = hz_range_to_period_samples(sample_rate, 50.0, 500.0);

    let mut group = c.benchmark_group("pitch_f64");
    group.throughput(Throughput::Elements(frame.len() as u64));
    group.bench_function("autocorrelation_pitch", |b| {
        b.iter(|| {
            autocorrelation_pitch(
                black_box(&frame),
                black_box(min_p),
                black_box(max_p),
                black_box(0.3),
            )
        })
    });
    group.bench_function("amdf_pitch", |b| {
        b.iter(|| {
            amdf_pitch(
                black_box(&frame),
                black_box(min_p),
                black_box(max_p),
                black_box(0.3),
            )
        })
    });
    group.finish();
}

fn query_f32(dim: usize) -> Vec<f32> {
    (0..dim)
        .map(|i| (i as f32 * 0.037).cos() + 0.2 * (i as f32 * 0.21).sin())
        .collect()
}

/// Linear vs. early-exit-accelerated `nearest_vector` (todo.md §8
/// "Benchmark linear search"), at Vox's and Whisper's target codebook
/// sizes.
fn bench_vq(c: &mut Criterion) {
    let mut group = c.benchmark_group("vq_f32");
    for &(entries, dim) in VQ_CODEBOOKS.iter() {
        let codebook = signal_f32(entries * dim);
        let query = query_f32(dim);
        let label = format!("{entries}x{dim}");
        group.throughput(Throughput::Elements(entries as u64));
        group.bench_with_input(
            BenchmarkId::new("nearest_linear", &label),
            &(entries, dim),
            |b, _| {
                b.iter(|| {
                    nearest_vector(
                        black_box(&codebook),
                        black_box(dim),
                        black_box(&query),
                        black_box(DistanceMetric::SquaredEuclidean),
                    )
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("nearest_accelerated", &label),
            &(entries, dim),
            |b, _| {
                b.iter(|| {
                    nearest_vector_accelerated(
                        black_box(&codebook),
                        black_box(dim),
                        black_box(&query),
                        black_box(DistanceMetric::SquaredEuclidean),
                    )
                })
            },
        );
    }
    group.finish();
}

/// `simultaneous_masking` (the O(bands²) core of the psychoacoustic
/// model) and `classify_tonal_bins`, at realistic sizes: ~25 Bark bands
/// (the full audible range) and a 1024-bin power spectrum (matching
/// Aura's default MDCT block half-size). todo.md §10.2 "benchmark".
fn bench_psychoacoustic(c: &mut Criterion) {
    let sample_rate = 48_000.0f32;
    let num_bands = 25;
    let band_bark: Vec<f32> = (0..num_bands).map(|i| i as f32).collect();
    let band_energy: Vec<f32> = (0..num_bands).map(|i| 1.0 + i as f32 * 10.0).collect();
    let band_tonal: Vec<bool> = (0..num_bands).map(|i| i % 3 == 0).collect();
    let mut excitation = vec![0.0f32; num_bands];

    let mut group = c.benchmark_group("psychoacoustic_f32");
    group.throughput(Throughput::Elements((num_bands * num_bands) as u64));
    group.bench_function("simultaneous_masking_25_bands", |b| {
        b.iter(|| {
            for slot in excitation.iter_mut() {
                *slot = 0.0;
            }
            simultaneous_masking(
                black_box(&band_energy),
                black_box(&band_bark),
                black_box(&band_tonal),
                black_box(&band_bark),
                black_box(&mut excitation),
            )
        })
    });

    let power_spectrum = signal_f32(1024)
        .iter()
        .map(|&x| x * x + 1.0)
        .collect::<Vec<f32>>();
    let mut tonal = vec![false; 1024];
    group.throughput(Throughput::Elements(1024));
    group.bench_function("classify_tonal_bins_1024", |b| {
        b.iter(|| {
            classify_tonal_bins(
                black_box(&power_spectrum),
                black_box(7.0),
                black_box(&mut tonal),
            )
        })
    });

    let mut bands = vec![0usize; 1024 / 2 + 1];
    group.bench_function("fft_bin_bark_bands_1024", |b| {
        b.iter(|| {
            fft_bin_bark_bands(
                black_box(sample_rate),
                black_box(1024),
                black_box(&mut bands),
            )
        })
    });
    group.bench_function("hz_to_bark", |b| {
        b.iter(|| hz_to_bark(black_box(1234.5f32)))
    });
    group.finish();
}

/// `noise_shape_step` (todo.md §11 "Add benchmark"), at Aura-realistic
/// orders (a plain first-order shaper and a higher-order one matching
/// its LPC order range).
fn bench_noise_shaping(c: &mut Criterion) {
    let quantize = |x: f32| x.round();
    let mut group = c.benchmark_group("noise_shaping_f32");
    for &order in &[1usize, 16] {
        let feedback = vec![0.5f32; order];
        let mut history = vec![0.0f32; order];
        group.bench_with_input(
            BenchmarkId::new("noise_shape_step", order),
            &order,
            |b, _| {
                b.iter(|| {
                    noise_shape_step(
                        black_box(0.3f32),
                        black_box(&feedback),
                        black_box(&mut history),
                        quantize,
                    )
                })
            },
        );
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
    bench_mdct,
    bench_lpc,
    bench_pitch,
    bench_vq,
    bench_psychoacoustic,
    bench_noise_shaping,
    bench_hilbert,
    bench_windows,
    bench_complex_ops,
    bench_simd_helpers,
    bench_fft_f32_dispatch,
    bench_fm_demod,
    bench_ring_buffer
);
criterion_main!(benches);
