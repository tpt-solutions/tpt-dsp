//! Feature, pitch, loudness and STFT benchmarks for `tpt-dsp-analysis`.
//!
//! Run with `cargo bench -p tpt-dsp-analysis --bench analysis_bench`.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use tpt_dsp_analysis::{
    spectral_flux, stft, yin_pitch, LoudnessMeter, Mfcc, RealtimeSpectrumAnalyzer, SpectrumConfig,
};

const SR: f32 = 16_000.0;

fn tone(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| (i as f32 * 220.0 * std::f32::consts::TAU / SR).sin())
        .collect()
}

fn bench_pitch_and_features(c: &mut Criterion) {
    let frame = tone(2048);
    c.bench_function("yin/2048", |b| {
        b.iter(|| yin_pitch(black_box(&frame), SR, 60.0, 1000.0, 0.15))
    });

    let prev = tone(1024);
    let cur = tone(1024);
    c.bench_function("spectral_flux/1024", |b| {
        b.iter(|| spectral_flux(black_box(&prev), black_box(&cur)))
    });
}

fn bench_mfcc(c: &mut Criterion) {
    let mut mfcc = Mfcc::new(SR, 512, 26, 13);
    let frame = tone(512);
    let mut out = [0.0f32; 13];
    c.bench_function("mfcc/512x26", |b| {
        b.iter(|| mfcc.compute(black_box(&frame), &mut out))
    });
}

fn bench_loudness(c: &mut Criterion) {
    let secs = tone(16_000);
    let mut group = c.benchmark_group("loudness");
    group.throughput(Throughput::Elements(secs.len() as u64));
    group.bench_function("push/1s", |b| {
        b.iter(|| {
            let mut m = LoudnessMeter::new(SR);
            m.push(black_box(&secs));
            m.integrated()
        })
    });
    group.finish();
}

fn bench_stft(c: &mut Criterion) {
    let x = tone(16_384);
    c.bench_function("stft/16384_fft512_hop128", |b| {
        b.iter(|| stft(black_box(&x), 512, 128))
    });
}

fn bench_spectrum(c: &mut Criterion) {
    let mut sa = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size: 1024,
        sample_rate: SR,
        ..SpectrumConfig::default()
    });
    let block = tone(1024);
    c.bench_function("spectrum_analyzer/1024", |b| {
        b.iter(|| sa.process(black_box(&block)))
    });
}

criterion_group!(
    benches,
    bench_pitch_and_features,
    bench_mfcc,
    bench_loudness,
    bench_stft,
    bench_spectrum
);
criterion_main!(benches);
