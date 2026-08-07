//! Filter and convolution benchmarks for `tpt-dsp-core`.
//!
//! Run with `cargo bench -p tpt-dsp-core --bench filter_bench`.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tpt_dsp_core::{
    convolve, process_biquad, Biquad, BiquadCoeffs, BiquadType, ConvolvePlan, FftConvolver, Fir,
    FirDesign, IirFilter,
};

const FS: f32 = 48_000.0;
const BLOCKS: [usize; 4] = [64, 128, 256, 1024];

fn signal(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 * 0.01).sin() + 0.3 * (i as f32 * 0.13).cos())
        .collect()
}

fn bench_biquad_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("biquad/lowpass_f32");
    for &n in BLOCKS.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = signal(n);
        let mut out = vec![0.0f32; n];
        let mut bq = Biquad::<f32>::design(BiquadType::LowPass, FS, 1_000.0, 0.707, 0.0);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| bq.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_biquad_free_fn(c: &mut Criterion) {
    let n = 256usize;
    let coeffs = BiquadCoeffs::<f32>::design(BiquadType::HighPass, FS, 200.0, 0.707, 0.0);
    let input = signal(n);
    let mut out = vec![0.0f32; n];
    let mut state = [0.0f32; 4];
    let mut group = c.benchmark_group("biquad/process_biquad_f32");
    group.throughput(Throughput::Elements(n as u64));
    group.bench_function(BenchmarkId::from_parameter(n), |b| {
        b.iter(|| {
            process_biquad(
                black_box(&coeffs),
                black_box(&mut state),
                black_box(&input),
                black_box(&mut out),
            )
        })
    });
    group.finish();
}

fn bench_biquad_tick(c: &mut Criterion) {
    let mut bq = Biquad::<f32>::design(BiquadType::BandPass, FS, 1_000.0, 2.0, 0.0);
    let mut group = c.benchmark_group("biquad/tick_f32");
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_sample", |b| {
        b.iter(|| black_box(bq.tick(black_box(0.5f32))))
    });
    group.finish();
}

fn bench_biquad_design(c: &mut Criterion) {
    let mut group = c.benchmark_group("biquad/design_f32");
    for (name, kind) in [
        ("lowpass", BiquadType::LowPass),
        ("highpass", BiquadType::HighPass),
        ("bandpass", BiquadType::BandPass),
        ("notch", BiquadType::Notch),
        ("allpass", BiquadType::AllPass),
        ("peaking", BiquadType::Peaking),
        ("lowshelf", BiquadType::LowShelf),
        ("highshelf", BiquadType::HighShelf),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| {
                BiquadCoeffs::<f32>::design(
                    black_box(kind),
                    black_box(FS),
                    black_box(1_000.0),
                    black_box(0.707),
                    black_box(6.0),
                )
            })
        });
    }
    group.finish();
}

fn bench_iir_cascade(c: &mut Criterion) {
    let n = 256usize;
    let input = signal(n);
    let mut out = vec![0.0f32; n];
    let mut group = c.benchmark_group("iir/cascade_256_f32");
    for &stages in [1usize, 2, 4, 8].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let sections: Vec<Biquad<f32>> = (0..stages)
            .map(|k| {
                Biquad::<f32>::design(BiquadType::Peaking, FS, 250.0 * (k + 1) as f32, 1.0, 3.0)
            })
            .collect();
        let mut filter = IirFilter::new(sections);
        group.bench_with_input(BenchmarkId::from_parameter(stages), &stages, |b, _| {
            b.iter(|| filter.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_fir(c: &mut Criterion) {
    let n = 1024usize;
    let input = signal(n);
    let mut out = vec![0.0f32; n];
    let mut group = c.benchmark_group("fir/lowpass_1024_f32");
    for &taps in [31usize, 63, 127, 255].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let mut fir = FirDesign::LowPass(0.05).design::<f32>(taps);
        group.bench_with_input(BenchmarkId::from_parameter(taps), &taps, |b, _| {
            b.iter(|| fir.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_fir_design(c: &mut Criterion) {
    let mut group = c.benchmark_group("fir/design_f32");
    for &taps in [63usize, 255].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(taps), &taps, |b, _| {
            b.iter(|| {
                let fir: Fir<f32> = FirDesign::LowPass(black_box(0.05)).design(black_box(taps));
                black_box(fir.len())
            })
        });
    }
    group.finish();
}

fn bench_convolve_direct(c: &mut Criterion) {
    let n = 1024usize;
    let input = signal(n);
    let mut group = c.benchmark_group("convolution/direct_1024_f32");
    for &klen in [16usize, 64, 256].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let kernel: Vec<f32> = (0..klen).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut out = vec![0.0f32; n + klen - 1];
        group.bench_with_input(BenchmarkId::from_parameter(klen), &klen, |b, _| {
            b.iter(|| convolve(black_box(&input), black_box(&kernel), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_convolve_plan(c: &mut Criterion) {
    let n = 1024usize;
    let input = signal(n);
    let mut group = c.benchmark_group("convolution/fft_plan_1024_f32");
    for &klen in [16usize, 64, 256].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let kernel: Vec<f32> = (0..klen).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut plan = ConvolvePlan::<f32>::new(&kernel, n);
        let mut out = vec![0.0f32; n + klen - 1];
        group.bench_with_input(BenchmarkId::from_parameter(klen), &klen, |b, _| {
            b.iter(|| plan.convolve(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_fft_convolver(c: &mut Criterion) {
    let kernel: Vec<f32> = (0..4_096)
        .map(|i| (i as f32 * 0.017).sin() * (-(i as f32) / 900.0).exp())
        .collect();
    let mut group = c.benchmark_group("convolution/overlap_add_f32");
    for &block in [128usize, 256, 512, 1024].iter() {
        group.throughput(Throughput::Elements(block as u64));
        let input = signal(block);
        let mut out = vec![0.0f32; block];
        let mut conv = FftConvolver::<f32>::new(&kernel, block);
        group.bench_with_input(BenchmarkId::from_parameter(block), &block, |b, _| {
            b.iter(|| conv.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_biquad_block,
    bench_biquad_free_fn,
    bench_biquad_tick,
    bench_biquad_design,
    bench_iir_cascade,
    bench_fir,
    bench_fir_design,
    bench_convolve_direct,
    bench_convolve_plan,
    bench_fft_convolver
);
criterion_main!(benches);
