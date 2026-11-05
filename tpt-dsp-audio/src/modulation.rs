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
