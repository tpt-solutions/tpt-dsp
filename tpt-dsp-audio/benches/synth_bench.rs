//! Synthesis benchmarks for `tpt-dsp-audio`.
//!
//! Run with `cargo bench -p tpt-dsp-audio --bench synth_bench`.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use tpt_dsp_audio::{Adsr, FmSynth, Oscillator, SubtractiveVoice, Waveform, Wavetable};

const FS: f32 = 48_000.0;
const BLOCK: usize = 256;

fn bench_oscillator(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth/oscillator_256");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let mut out = vec![0.0f32; BLOCK];
    for (name, waveform) in [
        ("sine", Waveform::Sine),
        ("sawtooth", Waveform::Sawtooth),
        ("square", Waveform::Square),
        ("triangle", Waveform::Triangle),
    ] {
        let mut osc = Oscillator::with_waveform(FS, 440.0, waveform);
        group.bench_function(name, |b| b.iter(|| osc.process(black_box(&mut out))));
    }
    group.finish();
}

fn bench_wavetable(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth/wavetable_256");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let mut out = vec![0.0f32; BLOCK];
    for &size in [256usize, 1024, 4096].iter() {
        let mut wt = Wavetable::from_waveform(size, FS, 440.0, Waveform::Sawtooth);
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
            b.iter(|| wt.process(black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_fm(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth/fm_2op_256");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let mut out = vec![0.0f32; BLOCK];
    for &index in [0.5f32, 3.0].iter() {
        let mut fm = FmSynth::new(FS, 440.0, 660.0, index);
        group.bench_with_input(BenchmarkId::from_parameter(index as u32), &index, |b, _| {
            b.iter(|| fm.process(black_box(&mut out)))
        });
    }
    group.finish();
}

fn bench_subtractive(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth/subtractive_voice_256");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let mut out = vec![0.0f32; BLOCK];
    let mut voice = SubtractiveVoice::new(FS);
    voice.note_on(220.0);
    group.bench_function("single_voice", |b| {
        b.iter(|| voice.process(black_box(&mut out)))
    });

    let mut voices: Vec<SubtractiveVoice> = (0..8)
        .map(|k| {
            let mut v = SubtractiveVoice::new(FS);
            v.note_on(110.0 * (k + 1) as f32);
            v
        })
        .collect();
    let mut mix = vec![0.0f32; BLOCK];
    group.throughput(Throughput::Elements((BLOCK * 8) as u64));
    group.bench_function("8_voice_polyphony", |b| {
        b.iter(|| {
            for m in mix.iter_mut() {
                *m = 0.0;
            }
            for v in voices.iter_mut() {
                v.process(&mut out);
                for (m, s) in mix.iter_mut().zip(out.iter()) {
                    *m += s * 0.125;
                }
            }
            black_box(mix[0])
        })
    });
    group.finish();
}

fn bench_envelope(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth/adsr_256");
    group.throughput(Throughput::Elements(BLOCK as u64));
    let mut out = vec![0.0f32; BLOCK];
    let mut env = Adsr::new(FS, 0.01, 0.2, 0.7, 0.3);
    env.note_on();
    group.bench_function("sustain", |b| b.iter(|| env.process(black_box(&mut out))));
    group.finish();
}

criterion_group!(
    benches,
    bench_oscillator,
    bench_wavetable,
    bench_fm,
    bench_subtractive,
    bench_envelope
);
criterion_main!(benches);
