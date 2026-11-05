//! Low-frequency oscillators and the tremolo effect built on them.
//!
//! An [`Lfo`] is a slow [`Oscillator`] whose output is in `[-1, 1]` (or
//! `[0, 1]` in unipolar mode), intended as a modulation source. [`Tremolo`]
//! multiplies the signal by an LFO-controlled gain.

use crate::util::{is_valid_sample_rate, sanitize_frequency};
use crate::{Oscillator, Waveform};

/// A low-frequency modulation source.
#[derive(Debug, Clone)]
pub struct Lfo {
    osc: Oscillator,
    unipolar: bool,
}

impl Lfo {
    /// Create an LFO at `rate_hz` with the given waveform. Typical rates are
    /// 0.1–20 Hz. Non-finite or negative rates become 0 (a constant output).
    pub fn new(sample_rate: f32, rate_hz: f32, waveform: Waveform) -> Self {
        Self {
            osc: Oscillator::with_waveform(sample_rate, sanitize_frequency(rate_hz), waveform),
            unipolar: false,
        }
    }

    /// Output in `[0, 1]` instead of `[-1, 1]` when `true`.
    pub fn set_unipolar(&mut self, unipolar: bool) {
        self.unipolar = unipolar;
    }

    /// Set the rate in Hz (non-finite or negative values become 0).
    pub fn set_rate(&mut self, rate_hz: f32) {
        self.osc.set_frequency(rate_hz);
    }

    /// Set the sample rate in Hz (invalid values are ignored).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if is_valid_sample_rate(sample_rate) {
            self.osc.set_sample_rate(sample_rate);
        }
    }

    /// Reset the phase to zero.
    pub fn reset(&mut self) {
        self.osc.reset();
    }

    /// Advance one sample and return the modulation value.
    pub fn tick(&mut self) -> f32 {
        let v = self.osc.tick();
        if self.unipolar {
            0.5 * (v + 1.0)
        } else {
            v
        }
    }
}

/// Tremolo: amplitude modulation by an LFO.
///
/// The gain swings between `1 − depth` and `1`, so `depth = 0` is transparent
/// and `depth = 1` fully mutes at the bottom of each cycle.
#[derive(Debug, Clone)]
pub struct Tremolo {
    lfo: Lfo,
    depth: f32,
}

impl Tremolo {
    /// Create a tremolo at `rate_hz` with depth in `[0, 1]` (clamped; NaN
    /// becomes 0).
    pub fn new(sample_rate: f32, rate_hz: f32, depth: f32) -> Self {
        let mut lfo = Lfo::new(sample_rate, rate_hz, Waveform::Sine);
        lfo.set_unipolar(true);
        Self {
            lfo,
            depth: clamp_unit(depth),
        }
    }

    /// Set the depth in `[0, 1]` (NaN becomes 0).
    pub fn set_depth(&mut self, depth: f32) {
        self.depth = clamp_unit(depth);
    }

    /// Set the LFO rate in Hz.
    pub fn set_rate(&mut self, rate_hz: f32) {
        self.lfo.set_rate(rate_hz);
    }

    /// Reset the LFO phase.
    pub fn reset(&mut self) {
        self.lfo.reset();
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let m = self.lfo.tick();
        x * (1.0 - self.depth * (1.0 - m))
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

fn clamp_unit(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unipolar_lfo_stays_in_unit_range() {
        let mut l = Lfo::new(48_000.0, 5.0, Waveform::Triangle);
        l.set_unipolar(true);
        for _ in 0..48_000 {
            let v = l.tick();
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn zero_depth_tremolo_is_transparent() {
        let mut t = Tremolo::new(48_000.0, 5.0, 0.0);
        for i in 0..1000 {
            let x = (i as f32 * 0.05).sin();
            assert_eq!(t.tick(x), x);
        }
    }

    #[test]
    fn full_depth_tremolo_reaches_silence_and_unity() {
        let mut t = Tremolo::new(48_000.0, 10.0, 1.0);
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for _ in 0..48_000 {
            let g = t.tick(1.0);
            min = min.min(g);
            max = max.max(g);
        }
        assert!(min < 0.01, "min gain {min}");
        assert!((max - 1.0).abs() < 0.01, "max gain {max}");
    }

    #[test]
    fn invalid_rate_gives_constant_output() {
        let mut l = Lfo::new(48_000.0, f32::NAN, Waveform::Sine);
        let first = l.tick();
        for _ in 0..100 {
            assert_eq!(l.tick(), first);
        }
    }
}

/// Interpolated delay line used by the modulated-delay effects.
#[derive(Debug, Clone)]
struct ModDelay {
    buffer: Vec<f32>,
    pos: usize,
}

impl ModDelay {
    fn new(max_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; max_samples.max(4)],
            pos: 0,
        }
    }

    /// Read `delay` samples back (fractional, linearly interpolated).
    fn read(&self, delay: f32) -> f32 {
        let len = self.buffer.len();
        let max = (len - 2) as f32;
        let d = delay.clamp(1.0, max);
        let read_pos = self.pos as f32 - d;
        let base = read_pos.floor();
        let frac = read_pos - base;
        let i0 = (base as isize).rem_euclid(len as isize) as usize;
        let i1 = (i0 + 1) % len;
        self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac
    }

    /// Write one sample and advance.
    fn write(&mut self, x: f32) {
        self.buffer[self.pos] = x;
        self.pos = (self.pos + 1) % self.buffer.len();
    }

    fn reset(&mut self) {
        self.buffer.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }
}

/// Chorus: a short delay swept by an LFO, mixed with the dry signal.
#[derive(Debug, Clone)]
pub struct Chorus {
    line: ModDelay,
    lfo: Lfo,
    base_samples: f32,
    depth_samples: f32,
    mix: f32,
}

impl Chorus {
    /// Create a chorus. The delay centres at 15 ms with `depth_ms` of sweep;
    /// the LFO runs at `rate_hz`. `mix` is clamped to `[0, 1]`.
    pub fn new(sample_rate: f32, rate_hz: f32, depth_ms: f32, mix: f32) -> Self {
        let sr = if is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            48_000.0
        };
        let base = 0.015 * sr;
        let depth = (depth_ms.max(0.0) * 0.001 * sr).clamp(0.0, base - 2.0);
        let max = (base + depth + 4.0).ceil() as usize;
        Self {
            line: ModDelay::new(max),
            lfo: Lfo::new(sr, rate_hz, Waveform::Sine),
            base_samples: base,
            depth_samples: depth,
            mix: clamp_unit(mix),
        }
    }

    /// Set the wet/dry mix in `[0, 1]` (NaN becomes 0).
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = clamp_unit(mix);
    }

    /// Clear the delay line.
    pub fn reset(&mut self) {
        self.line.reset();
        self.lfo.reset();
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let m = self.lfo.tick();
        let wet = self.line.read(self.base_samples + self.depth_samples * m);
        self.line.write(x);
        x * (1.0 - self.mix) + wet * self.mix
    }
}

/// Flanger: a very short delay swept by an LFO, with feedback.
#[derive(Debug, Clone)]
pub struct Flanger {
    line: ModDelay,
    lfo: Lfo,
    base_samples: f32,
    depth_samples: f32,
    feedback: f32,
    mix: f32,
}

impl Flanger {
    /// Create a flanger. The delay sweeps about 1 ms, ± `depth_ms`. `feedback`
    /// is clamped to `[-0.95, 0.95]` and `mix` to `[0, 1]`.
    pub fn new(sample_rate: f32, rate_hz: f32, depth_ms: f32, feedback: f32, mix: f32) -> Self {
        let sr = if is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            48_000.0
        };
        let base = 0.001 * sr + 2.0;
        let depth = (depth_ms.max(0.0) * 0.001 * sr).clamp(0.0, base - 2.0);
        let max = (base + depth + 4.0).ceil() as usize;
        Self {
            line: ModDelay::new(max),
            lfo: Lfo::new(sr, rate_hz, Waveform::Sine),
            base_samples: base,
            depth_samples: depth,
            feedback: if feedback.is_finite() {
                feedback.clamp(-0.95, 0.95)
            } else {
                0.0
            },
            mix: clamp_unit(mix),
        }
    }

    /// Clear the delay line.
    pub fn reset(&mut self) {
        self.line.reset();
        self.lfo.reset();
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let m = self.lfo.tick();
        let wet = self.line.read(self.base_samples + self.depth_samples * m);
        self.line.write(x + self.feedback * wet);
        x * (1.0 - self.mix) + wet * self.mix
    }
}

/// Phaser: a cascade of LFO-swept all-pass stages mixed with the dry signal,
/// which produces moving notches.
#[derive(Debug, Clone)]
pub struct Phaser {
    stages: Vec<tpt_dsp_core::Biquad<f32>>,
    lfo: Lfo,
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
    mix: f32,
    countdown: u32,
}

/// Samples between all-pass coefficient refreshes.
const PHASER_COEFF_INTERVAL: u32 = 32;

impl Phaser {
    /// Create a phaser with `stages` all-pass sections (clamped to `1..=12`)
    /// sweeping between `min_hz` and `max_hz` at `rate_hz`.
    pub fn new(
        sample_rate: f32,
        rate_hz: f32,
        stages: usize,
        min_hz: f32,
        max_hz: f32,
        mix: f32,
    ) -> Self {
        let sr = if is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            48_000.0
        };
        let n = stages.clamp(1, 12);
        let initial = tpt_dsp_core::BiquadCoeffs::design(
            tpt_dsp_core::BiquadType::AllPass,
            sr,
            1000.0,
            0.7,
            0.0,
        );
        let stage_vec = (0..n)
            .map(|_| tpt_dsp_core::Biquad::from_coeffs(initial))
            .collect();
        let lo = min_hz.clamp(20.0, sr * 0.45);
        let hi = max_hz.clamp(20.0, sr * 0.45);
        Self {
            stages: stage_vec,
            lfo: Lfo::new(sr, rate_hz, Waveform::Sine),
            sample_rate: sr,
            min_hz: lo.min(hi),
            max_hz: lo.max(hi),
            mix: clamp_unit(mix),
            countdown: 0,
        }
    }

    /// Clear the all-pass state.
    pub fn reset(&mut self) {
        for s in self.stages.iter_mut() {
            s.reset();
        }
        self.lfo.reset();
        self.countdown = 0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        if self.countdown == 0 {
            let m = 0.5 * (self.lfo.tick() + 1.0);
            // Exponential sweep: equal musical steps across the range.
            let f = self.min_hz * (self.max_hz / self.min_hz).powf(m);
            let c = tpt_dsp_core::BiquadCoeffs::design(
                tpt_dsp_core::BiquadType::AllPass,
                self.sample_rate,
                f,
                0.7,
                0.0,
            );
            for s in self.stages.iter_mut() {
                s.set_coeffs(c);
            }
            self.countdown = PHASER_COEFF_INTERVAL;
        }
        self.countdown -= 1;
        let mut y = x;
        for s in self.stages.iter_mut() {
            y = s.tick(y);
        }
        x * (1.0 - self.mix) + y * self.mix
    }
}

#[cfg(test)]
mod effect_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * 440.0 * core::f32::consts::TAU / SR).sin())
            .collect()
    }

    #[test]
    fn chorus_with_zero_mix_is_dry() {
        let mut c = Chorus::new(SR, 0.5, 2.0, 0.0);
        for x in sine(2000) {
            assert!((c.tick(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn chorus_changes_the_signal_when_wet() {
        let mut c = Chorus::new(SR, 0.5, 2.0, 0.5);
        let input = sine(24_000);
        let diff: f32 = input
            .iter()
            .skip(4000)
            .map(|&x| (c.tick(x) - x).abs())
            .sum();
        assert!(diff > 10.0, "barely wet: {diff}");
    }

    #[test]
    fn flanger_feedback_stays_bounded() {
        let mut f = Flanger::new(SR, 0.25, 0.5, 0.95, 0.5);
        for x in sine(48_000) {
            let y = f.tick(x);
            assert!(y.is_finite() && y.abs() < 10.0, "{y}");
        }
    }

    #[test]
    fn phaser_with_zero_mix_is_dry() {
        let mut p = Phaser::new(SR, 0.3, 4, 200.0, 2000.0, 0.0);
        for x in sine(1000) {
            assert!((p.tick(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn phaser_wet_output_is_finite() {
        let mut p = Phaser::new(SR, 0.3, 4, 200.0, 2000.0, 0.5);
        let mut energy = 0.0f32;
        for x in sine(48_000) {
            let y = p.tick(x);
            assert!(y.is_finite());
            energy += y * y;
        }
        assert!(energy > 0.0);
    }
}
