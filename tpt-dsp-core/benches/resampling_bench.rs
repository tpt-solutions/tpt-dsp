//! Sample-rate-conversion benchmarks for `tpt-dsp-core`.
//!
//! Compares [`tpt_dsp_core::FIRDecimator`] against the `rubato` crate, a
//! widely used pure-Rust resampler, on the same workload. `rubato` is a
//! benchmark-only dev-dependency and is never linked into the library.
//!
//! `libsamplerate` (C) and JUCE (C++) cannot be linked from this pure-Rust
//! workspace without out-of-scope FFI/C++ build plumbing, so `rubato` stands
//! in as the external reference. See `BENCHMARKS.md` for the rationale and
//! for the quality caveats that apply when reading these numbers.
//!
//! Run with `cargo bench -p tpt-dsp-core --bench resampling_bench`.

use audioadapter_buffers::owned::InterleavedOwned;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rubato::{
    calculate_cutoff, Async, Fft, FixedAsync, FixedSync, PolynomialDegree, Resampler,
    SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::hint::black_box;
use tpt_dsp_core::{FIRDecimator, Fir, FirDesign};

const CHUNK: usize = 1024;
const SINC_LEN: usize = 128;
const OVERSAMPLING: usize = 256;

fn signal(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let t = i as f32;
            0.6 * (t * 0.011).sin() + 0.3 * (t * 0.21).sin() + 0.1 * (t * 0.47).cos()
        })
        .collect()
}

fn sinc_params() -> SincInterpolationParameters {
    let window = WindowFunction::Blackman2;
    SincInterpolationParameters {
        sinc_len: SINC_LEN,
        f_cutoff: Some(calculate_cutoff::<f32>(SINC_LEN, window)),
        interpolation: SincInterpolationType::Cubic,
        oversampling_factor: OVERSAMPLING,
        window,
    }
}

fn bench_rubato(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    id: BenchmarkId,
    mut resampler: Box<dyn Resampler<f32>>,
) {
    let frames_in = resampler.input_frames_next();
    let input = InterleavedOwned::new_from(signal(frames_in), 1, frames_in).unwrap();
    let mut output = InterleavedOwned::new(0.0f32, 1, resampler.output_frames_max());
    group.throughput(Throughput::Elements(frames_in as u64));
    group.bench_function(id, |b| {
        b.iter(|| {
            resampler
                .process_into_buffer(black_box(&input), black_box(&mut output), None)
                .unwrap()
        })
    });
}

fn bench_decimate_2x(c: &mut Criterion) {
    let mut group = c.benchmark_group("srcv/decimate_2x_48k_to_24k_f32");
    let input = signal(CHUNK);

    for &taps in [63usize, 127, 255].iter() {
        let mut dec = FIRDecimator::<f32>::design(2, 0.2, taps);
        let mut out = vec![0.0f32; CHUNK / 2];
        group.throughput(Throughput::Elements(CHUNK as u64));
        group.bench_function(BenchmarkId::new("tpt/fir_decimator", taps), |b| {
            b.iter(|| dec.process(black_box(&input), black_box(&mut out)))
        });
    }

    for &taps in [63usize, 127, 255].iter() {
        let mut fir = FirDesign::LowPass(0.2).design::<f32>(taps);
        let mut filtered = vec![0.0f32; CHUNK];
        let mut out = vec![0.0f32; CHUNK / 2];
        group.throughput(Throughput::Elements(CHUNK as u64));
        group.bench_function(BenchmarkId::new("tpt/fir_filter_then_drop", taps), |b| {
            b.iter(|| {
                fir.process(black_box(&input), black_box(&mut filtered));
                for (o, chunk) in out.iter_mut().zip(filtered.chunks_exact(2)) {
                    *o = chunk[0];
                }
            })
        });
    }

    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/fft_sync", CHUNK),
        Box::new(Fft::<f32>::new(48_000, 24_000, CHUNK, 2, FixedSync::Input).unwrap()),
    );
    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/sinc_async_cubic", SINC_LEN),
        Box::new(
            Async::<f32>::new_sinc(0.5, 1.1, &sinc_params(), CHUNK, 1, FixedAsync::Input).unwrap(),
        ),
    );
    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/poly_async_cubic", CHUNK),
        Box::new(
            Async::<f32>::new_poly(
                0.5,
                1.1,
                PolynomialDegree::Cubic,
                CHUNK,
                1,
                FixedAsync::Input,
            )
            .unwrap(),
        ),
    );
    group.finish();
}

fn bench_decimate_factors(c: &mut Criterion) {
    let mut group = c.benchmark_group("srcv/tpt_fir_decimator_127taps_f32");
    let input = signal(CHUNK);
    for &factor in [2usize, 4, 8, 16].iter() {
        let mut dec = FIRDecimator::<f32>::design_default(factor, 127);
        let mut out = vec![0.0f32; CHUNK / factor];
        group.throughput(Throughput::Elements(CHUNK as u64));
        group.bench_with_input(BenchmarkId::from_parameter(factor), &factor, |b, _| {
            b.iter(|| dec.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_decimate_f64(c: &mut Criterion) {
    let mut group = c.benchmark_group("srcv/tpt_fir_decimator_2x_f64");
    let input: Vec<f64> = signal(CHUNK).iter().map(|&x| x as f64).collect();
    for &taps in [63usize, 127, 255].iter() {
        let mut dec = FIRDecimator::<f64>::design(2, 0.2, taps);
        let mut out = vec![0.0f64; CHUNK / 2];
        group.throughput(Throughput::Elements(CHUNK as u64));
        group.bench_with_input(BenchmarkId::from_parameter(taps), &taps, |b, _| {
            b.iter(|| dec.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_arbitrary_ratio(c: &mut Criterion) {
    let mut group = c.benchmark_group("srcv/resample_48k_to_44k1_f32_rubato_only");
    let ratio = 44_100.0 / 48_000.0;
    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/fft_sync", CHUNK),
        Box::new(Fft::<f32>::new(48_000, 44_100, CHUNK, 2, FixedSync::Input).unwrap()),
    );
    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/sinc_async_cubic", SINC_LEN),
        Box::new(
            Async::<f32>::new_sinc(ratio, 1.1, &sinc_params(), CHUNK, 1, FixedAsync::Input)
                .unwrap(),
        ),
    );
    bench_rubato(
        &mut group,
        BenchmarkId::new("rubato/poly_async_cubic", CHUNK),
        Box::new(
            Async::<f32>::new_poly(
                ratio,
                1.1,
                PolynomialDegree::Cubic,
                CHUNK,
                1,
                FixedAsync::Input,
            )
            .unwrap(),
        ),
    );
    group.finish();
}

fn bench_fir_reference(c: &mut Criterion) {
    let mut group = c.benchmark_group("srcv/anti_alias_filter_only_f32");
    let input = signal(CHUNK);
    let mut out = vec![0.0f32; CHUNK];
    for &taps in [63usize, 127, 255].iter() {
        let mut fir: Fir<f32> = FirDesign::LowPass(0.2).design(taps);
        group.throughput(Throughput::Elements(CHUNK as u64));
        group.bench_with_input(BenchmarkId::from_parameter(taps), &taps, |b, _| {
            b.iter(|| fir.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_decimate_2x,
    bench_decimate_factors,
    bench_decimate_f64,
    bench_arbitrary_ratio,
    bench_fir_reference
);
criterion_main!(benches);
