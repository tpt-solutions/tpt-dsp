# Cookbook

Short, runnable recipes built on the public API. Each uses only functions the
test suites exercise. Sample rates are in Hz.

## 1. Low-pass a signal at 1 kHz

```rust,ignore
use tpt_dsp_core::{Biquad, BiquadType};

let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let mut block = [0.0f32; 256];
lp.process(&block.clone(), &mut block);
```

For a steeper roll-off, use a cascade:

```rust,ignore
use tpt_dsp_core::{butterworth, cascade_magnitude_response, Response};

let sections = butterworth(Response::LowPass, 6, 48_000.0, 1_000.0).unwrap();
let gain_at_cutoff = cascade_magnitude_response(&sections, 48_000.0, 1_000.0);
// About 0.707 (−3 dB) at the cutoff.
```

## 2. A guitar pedal: distortion, delay, reverb, EQ

The web pedalboard in `tpt-dsp-wasm` is this chain. In Rust, the same order is:

```rust,ignore
use tpt_dsp_audio::{Delay, Eq, Freeverb};

// Build once, outside the callback.
let mut delay = Delay::new(48_000);          // up to one second
delay.set_delay_seconds(0.35, 48_000.0);
delay.set_feedback(0.3);
delay.set_mix(0.25);

let mut reverb = Freeverb::new(48_000.0, 0.6, 0.4, 0.2);
let mut eq = Eq::new(48_000.0, &[(200.0, 2.0, 1.0), (3_000.0, -3.0, 1.2)]);

// In the callback (128 samples per block):
// shaped → delay.process → reverb.process → eq.process(&mut block)
```

## 3. Oversample a distortion stage

Hard clipping at the base rate creates aliasing. Run it at 4×:

```rust,ignore
use tpt_dsp_audio::Oversampler;

let mut os = Oversampler::new(4, 33);       // latency: (33 − 1) / 4 = 8 samples
let mut block = [0.0f32; 128];
os.process(&mut block, |x| (x * 4.0).clamp(-1.0, 1.0));
```

## 4. Compress and limit a mix

```rust,ignore
use tpt_dsp_audio::{Compressor, Limiter};

let mut comp = Compressor::new(48_000.0, -18.0, 3.0, 0.005, 0.12);
comp.set_makeup_gain_db(3.0);
let mut limit = Limiter::new(48_000.0, -1.0, 0.08);
// per sample: y = limit.tick(comp.tick(x));
```

## 5. Track an FM station at baseband

```rust,ignore
use tpt_dsp_core::{FmDemodulator, C32};

let mut fm = FmDemodulator::<f32>::with_deviation(240_000.0, 75_000.0);
// For each complex IQ sample z (already decimated to 240 kHz):
// let audio = fm.tick(z);
```

## 6. Measure the pitch of a monophonic note

```rust,ignore
use tpt_dsp_analysis::yin_pitch;

let frame: Vec<f32> = read_frame();          // 2048 samples at 16 kHz
if let Some(f0) = yin_pitch(&frame, 16_000.0, 60.0, 1_000.0, 0.15) {
    println!("{f0:.1} Hz");
}
```

`yin_pitch` allocates per call, so run it on an analysis thread, not the
callback.

## 7. Loudness of a program

```rust,ignore
use tpt_dsp_analysis::LoudnessMeter;

let mut meter = LoudnessMeter::new(48_000.0);
meter.push(&samples);
if let Some(lufs) = meter.integrated() {
    println!("{lufs:.1} LUFS");
}
```

The meter follows ITU-R BS.1770 (see `loudness.rs`). On the test signals it agrees
with pyloudnorm's `"DeMan"` class to about 0.01 LU. It does not use pyloudnorm's
default `"K-weighting"` class, which is an older design. Check against a reference
meter before any compliance use.

## 8. A smooth motion profile for a stepper

```rust,ignore
use tpt_dsp_control::TrapezoidalProfile;

let profile = TrapezoidalProfile::try_new(0.0, 100.0, 50.0, 200.0).unwrap();
let t_total = profile.duration();
for step in 0..100 {
    let (pos, vel, acc) = profile.at(step as f32 * t_total / 100.0);
    // send `pos` to the drive
}
```

## 9. Hold a setpoint with a PID loop

```rust,ignore
use tpt_dsp_control::Pid;

let mut pid = Pid::try_new(2.0, 0.5, 0.05, 0.001).unwrap();
pid.set_setpoint(10.0);
pid.set_output_limits(-1.0, 1.0);
pid.set_derivative_filter(0.01);
// each control tick: let u = pid.update(measured);
```

The derivative acts on the measurement, so a setpoint step does not kick the
output.

## 10. Glide a parameter without zipper noise

```rust,ignore
use tpt_dsp_core::Smoothed;

let mut gain = Smoothed::new(0.5f32, 0.02, 48_000.0);   // 20 ms glide
gain.set_target(0.8);
// per sample: let g = gain.advance();
```
