//! PID, kinematics and input-shaping benchmarks for `tpt-dsp-control`.
//!
//! Run with `cargo bench -p tpt-dsp-control --bench control_bench`.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use tpt_dsp_control::{InputShaper, JerkLimiter, Pid, TrapezoidalProfile};

fn bench_pid(c: &mut Criterion) {
    let mut pid = Pid::new(2.0, 0.5, 0.05, 0.001);
    pid.set_setpoint(10.0);
    pid.set_output_limits(-100.0, 100.0);
    let mut group = c.benchmark_group("pid");
    group.throughput(Throughput::Elements(1));
    group.bench_function("update", |b| {
        let mut y = 0.0f32;
        b.iter(|| {
            let u = pid.update(black_box(y));
            y += 0.001 * u;
            u
        })
    });
    group.finish();
}

fn bench_profile(c: &mut Criterion) {
    let profile = TrapezoidalProfile::new(0.0, 100.0, 50.0, 200.0);
    c.bench_function("trapezoid/at", |b| {
        let mut t = 0.0f32;
        let dur = profile.duration();
        b.iter(|| {
            t = (t + 0.001) % dur;
            profile.at(black_box(t))
        })
    });
}

fn bench_jerk(c: &mut Criterion) {
    let mut jl = JerkLimiter::new(100.0, 20.0, 10.0, 0.001);
    c.bench_function("jerk_limiter/update", |b| {
        b.iter(|| jl.update(black_box(5.0)))
    });
}

fn bench_shaper(c: &mut Criterion) {
    let mut shaper = InputShaper::new_zvd(20.0, 0.05, 1000.0);
    c.bench_function("input_shaper/tick", |b| {
        b.iter(|| shaper.tick(black_box(1.0)))
    });
}

criterion_group!(benches, bench_pid, bench_profile, bench_jerk, bench_shaper);
criterion_main!(benches);
