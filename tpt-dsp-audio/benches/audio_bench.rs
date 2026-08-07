//! Effect, graph and engine benchmarks for `tpt-dsp-audio`.
//!
//! Run with `cargo bench -p tpt-dsp-audio --bench audio_bench`.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tpt_dsp_audio::{
    generate_decay_ir, AudioGraph, AudioNode, ClosureNode, ClosureSink, ClosureSource,
    ConvolutionReverb, Curve, Delay, Eq, Oscillator, Passthrough, RealtimeEngine, Waveform,
    Waveshaper, BLOCK_128, BLOCK_256,
};

const FS: f32 = 48_000.0;
const BLOCKS: [usize; 4] = [64, 128, 256, 512];

fn block(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (i as f32 * 0.03).sin() + 0.2 * (i as f32 * 0.31).cos())
        .collect()
}

fn bench_reverb(c: &mut Criterion) {
    let ir = generate_decay_ir(4_096, FS, 0.5);
    let mut group = c.benchmark_group("effects/convolution_reverb");
    for &n in BLOCKS.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = block(n);
        let mut out = vec![0.0f32; n];
        let mut reverb = ConvolutionReverb::new(&ir, n);
        reverb.set_wet(0.4);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| reverb.process(black_box(&input), black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_eq(c: &mut Criterion) {
    let three = [(100.0, 3.0, 0.7), (1_000.0, 6.0, 1.0), (8_000.0, -3.0, 0.7)];
    let ten: Vec<(f32, f32, f32)> = (0..10)
        .map(|k| {
            (
                31.25 * 2.0f32.powi(k),
                if k % 2 == 0 { 3.0 } else { -3.0 },
                1.0,
            )
        })
        .collect();

    let mut group = c.benchmark_group("effects/eq");
    for &n in BLOCKS.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let source = block(n);
        let mut buf = source.clone();

        let mut eq3 = Eq::new(FS, &three);
        group.bench_with_input(BenchmarkId::new("3band", n), &n, |b, _| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                eq3.process(black_box(&mut buf))
            })
        });

        let mut eq10 = Eq::new(FS, &ten);
        group.bench_with_input(BenchmarkId::new("10band", n), &n, |b, _| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                eq10.process(black_box(&mut buf))
            })
        });

        let mut shelved = Eq::with_shelves(FS, (120.0, 4.0), &three, (10_000.0, -2.0));
        group.bench_with_input(BenchmarkId::new("shelves_3band", n), &n, |b, _| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                shelved.process(black_box(&mut buf))
            })
        });
    }
    group.finish();
}

fn bench_delay(c: &mut Criterion) {
    let mut group = c.benchmark_group("effects/delay");
    for &n in BLOCKS.iter() {
        group.throughput(Throughput::Elements(n as u64));
        let source = block(n);
        let mut buf = source.clone();
        let mut delay = Delay::new(FS as usize);
        delay.set_delay_seconds(0.25, FS);
        delay.set_feedback(0.4);
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                delay.process(black_box(&mut buf))
            })
        });
    }
    group.finish();
}

fn bench_waveshaper(c: &mut Criterion) {
    let n = 256usize;
    let source = block(n);
    let mut buf = source.clone();
    let mut group = c.benchmark_group("effects/waveshaper_256");
    group.throughput(Throughput::Elements(n as u64));
    for (name, curve) in [
        ("tanh", Curve::Tanh),
        ("hardclip", Curve::HardClip),
        ("cubic", Curve::Cubic),
        ("polynomial", Curve::Polynomial([0.0, 1.0, 0.2, -0.3])),
    ] {
        let mut ws = Waveshaper::new(curve, 4.0, 0.8);
        group.bench_function(name, |b| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                ws.process(black_box(&mut buf))
            })
        });
    }
    group.finish();
}

fn bench_pedalboard_chain(c: &mut Criterion) {
    let ir = generate_decay_ir(2_048, FS, 0.3);
    let mut group = c.benchmark_group("effects/pedalboard_chain");
    for &n in [BLOCK_128, BLOCK_256].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let source = block(n);
        let mut buf = source.clone();
        let mut wet = vec![0.0f32; n];

        let mut ws = Waveshaper::new(Curve::Tanh, 6.0, 0.9);
        let mut eq = Eq::new(FS, &[(120.0, 4.0, 0.8), (2_500.0, -4.0, 1.2)]);
        let mut delay = Delay::new(FS as usize);
        delay.set_delay_seconds(0.18, FS);
        let mut reverb = ConvolutionReverb::new(&ir, n);
        reverb.set_wet(0.3);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                buf.copy_from_slice(&source);
                ws.process(black_box(&mut buf));
                eq.process(black_box(&mut buf));
                delay.process(black_box(&mut buf));
                reverb.process(black_box(&buf), black_box(&mut wet));
            })
        });
    }
    group.finish();
}

fn bench_graph(c: &mut Criterion) {
    let mut group = c.benchmark_group("graph");
    for &n in [BLOCK_128, BLOCK_256].iter() {
        group.throughput(Throughput::Elements(n as u64));

        let mut osc = Oscillator::with_waveform(FS, 220.0, Waveform::Sawtooth);
        let nodes: Vec<Box<dyn AudioNode>> = vec![
            Box::new(Passthrough),
            Box::new(ClosureNode(|input: &[f32], out: &mut [f32]| {
                for (o, x) in out.iter_mut().zip(input.iter()) {
                    *o = x * 0.5;
                }
            })),
            Box::new(Passthrough),
        ];
        let mut graph = AudioGraph::new(
            n,
            Box::new(ClosureSource(move |out: &mut [f32]| osc.process(out))),
            nodes,
            Box::new(ClosureSink(|input: &[f32]| {
                black_box(input);
            })),
        );
        group.bench_with_input(BenchmarkId::new("source_3nodes_sink", n), &n, |b, _| {
            b.iter(|| black_box(graph.tick().len()))
        });
    }
    group.finish();
}

fn bench_engine(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine/realtime_callback");
    for &n in [BLOCK_128, BLOCK_256].iter() {
        group.throughput(Throughput::Elements(n as u64));
        let input = block(n);
        let mut eq = Eq::new(FS, &[(1_000.0, 6.0, 1.0)]);
        let mut scratch = vec![0.0f32; n];
        let mut engine = RealtimeEngine::new(n, move |src: &[f32], out: &mut [f32]| {
            scratch.copy_from_slice(src);
            eq.process(&mut scratch);
            out.copy_from_slice(&scratch);
        });
        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| black_box(engine.process_with(black_box(&input)).len()))
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_reverb,
    bench_eq,
    bench_delay,
    bench_waveshaper,
    bench_pedalboard_chain,
    bench_graph,
    bench_engine
);
criterion_main!(benches);
