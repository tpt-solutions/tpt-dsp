//! Delay effect with feedback and wet/dry mix.
//!
//! A single-tap delay using a pre-allocated circular buffer. The delay time
//! may be fractional (read with linear interpolation) and changes glide
//! smoothly toward the requested target, so moving the time does not click.
//! The feedback gain controls how much of the delayed signal is fed back into
//! the buffer, producing repeating echoes. All state is fixed at construction,
//! so processing is allocation-free and real-time safe.

/// Per-sample one-pole coefficient for the delay-time glide. At 48 kHz this
/// reaches ~63% of a time change in roughly 40 ms.
const DELAY_GLIDE: f32 = 0.0005;

/// Magnitudes below this are flushed to zero in the feedback path, so decaying
/// echoes can't sink into subnormal arithmetic (which is very slow on many
/// CPUs). It is about -200 dBFS, far below audibility.
const FLUSH_THRESHOLD: f32 = 1.0e-20;

/// A feedback delay line.
pub struct Delay {
    buffer: Vec<f32>,
    pos: usize,
    /// Requested delay in samples.
    target_delay: f32,
    /// Delay actually in use; glides toward `target_delay`.
    current_delay: f32,
    feedback: f32,
    mix: f32,
}

impl Delay {
    /// Create a delay with capacity for `max_delay_samples` of latency.
    ///
    /// # Panics
    ///
    /// Panics if `max_delay_samples` is less than 2.
    pub fn new(max_delay_samples: usize) -> Self {
        assert!(max_delay_samples >= 2, "delay capacity must be at least 2");
        let initial = ((max_delay_samples / 2).max(1)) as f32;
        Self {
            buffer: vec![0.0; max_delay_samples],
            pos: 0,
            target_delay: initial,
            current_delay: initial,
            feedback: 0.3,
            mix: 0.5,
        }
    }

    /// Largest delay the buffer can hold, in samples.
    fn max_delay(&self) -> f32 {
        (self.buffer.len() - 1) as f32
    }

    /// Set the delay time in whole samples (clamped to `1..=capacity - 1`).
    /// The change glides in over a few tens of milliseconds.
    pub fn set_delay_samples(&mut self, samples: usize) {
        self.set_delay_fractional(samples as f32);
    }

    /// Set a fractional delay time in samples (clamped to
    /// `1..=capacity - 1`). Non-finite values are ignored. The change glides
    /// in over a few tens of milliseconds.
    pub fn set_delay_fractional(&mut self, samples: f32) {
        if samples.is_finite() {
            self.target_delay = samples.clamp(1.0, self.max_delay());
        }
    }

    /// Set the delay time in seconds at the given sample rate. Invalid rates
    /// or times are ignored.
    pub fn set_delay_seconds(&mut self, seconds: f32, sample_rate: f32) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            self.set_delay_fractional(seconds * sample_rate);
        }
    }

    /// Set the feedback gain (`0` = single echo, near `1` = infinite repeat).
    /// Clamped to `[0, 0.99]`; NaN becomes `0`.
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = clamp_or_zero(feedback, 0.0, 0.99);
    }

    /// Set the wet/dry mix (`0` = dry, `1` = fully wet). NaN becomes `0`.
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = clamp_or_zero(mix, 0.0, 1.0);
    }

    /// Clear the delay memory and snap the delay time to its target (no glide).
    pub fn reset(&mut self) {
        for s in self.buffer.iter_mut() {
            *s = 0.0;
        }
        self.pos = 0;
        self.current_delay = self.target_delay;
    }

    /// Process one sample: `out = dry·x + wet·delayed`.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let len = self.buffer.len();

        self.current_delay += (self.target_delay - self.current_delay) * DELAY_GLIDE;

        // Fractional read: interpolate between the two nearest taps.
        let read_pos = self.pos as f32 - self.current_delay;
        let base = read_pos.floor();
        let frac = read_pos - base;
        let i0 = (base as isize).rem_euclid(len as isize) as usize;
        let i1 = (i0 + 1) % len;
        let delayed = self.buffer[i0] * (1.0 - frac) + self.buffer[i1] * frac;

        let mut written = x + delayed * self.feedback;
        if !written.is_finite() || written.abs() < FLUSH_THRESHOLD {
            written = 0.0;
        }
        self.buffer[self.pos] = written;
        self.pos = (self.pos + 1) % len;
        x * (1.0 - self.mix) + delayed * self.mix
    }

    /// Process a whole block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

fn clamp_or_zero(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_delayed_silence() {
        let mut d = Delay::new(4800);
        d.set_mix(1.0);
        for _ in 0..100 {
            assert_eq!(d.tick(0.0), 0.0);
        }
    }

    #[test]
    fn impulse_echoes_after_delay() {
        let mut d = Delay::new(1000);
        d.set_delay_samples(100);
        d.reset(); // snap to the target so the glide doesn't shift the echo
        d.set_feedback(0.0);
        d.set_mix(1.0);
        let mut seen = vec![0.0f32; 300];
        for (i, s) in seen.iter_mut().enumerate() {
            let x = if i == 0 { 1.0 } else { 0.0 };
            *s = d.tick(x);
        }
        assert_eq!(seen[0], 0.0); // impulse goes into the line first
        assert!((seen[100] - 1.0).abs() < 1e-6, "echo at {}", seen[100]);
    }

    #[test]
    fn feedback_decay_produces_trailing_echos() {
        let mut d = Delay::new(500);
        d.set_delay_samples(50);
        d.reset();
        d.set_feedback(0.5);
        d.set_mix(1.0);
        // Feed one impulse; subsequent echoes should be ~0.5^k of the previous.
        let _ = d.tick(1.0);
        let mut prev = 0.0f32;
        let mut first = true;
        let mut echoes_found = 0;
        for _ in 0..200 {
            let tap = d.tick(0.0);
            if tap > 0.01 {
                if first {
                    assert!((tap - 1.0).abs() < 1e-4, "first echo {tap}");
                    first = false;
                    prev = 1.0;
                } else {
                    assert!(
                        (tap - prev * 0.5).abs() < 1e-4,
                        "tap {tap} expected {}",
                        prev * 0.5
                    );
                    prev = tap;
                }
                echoes_found += 1;
            }
        }
        assert!(echoes_found >= 3, "found {echoes_found} echoes");
    }

    #[test]
    fn fractional_delay_interpolates_linearly() {
        let mut d = Delay::new(100);
        d.set_delay_fractional(10.5);
        d.reset();
        d.set_feedback(0.0);
        d.set_mix(1.0);
        let mut seen = [0.0f32; 16];
        for (i, s) in seen.iter_mut().enumerate() {
            *s = d.tick(if i == 0 { 1.0 } else { 0.0 });
        }
        // The impulse sits halfway between taps 10 and 11.
        assert!((seen[10] - 0.5).abs() < 1e-6, "tap10 {}", seen[10]);
        assert!((seen[11] - 0.5).abs() < 1e-6, "tap11 {}", seen[11]);
        assert!(seen[9].abs() < 1e-6 && seen[12].abs() < 1e-6);
    }

    #[test]
    fn delay_time_glides_instead_of_jumping() {
        let mut d = Delay::new(1000);
        d.set_delay_samples(100);
        d.reset();
        d.set_delay_samples(500);
        let before = d.current_delay;
        d.tick(0.0);
        let step = d.current_delay - before;
        assert!(step > 0.0 && step < 1.0, "one tick moved by {step}");
        for _ in 0..200_000 {
            d.tick(0.0);
        }
        assert!((d.current_delay - 500.0).abs() < 0.5);
    }

    #[test]
    fn non_finite_inputs_stay_finite() {
        let mut d = Delay::new(64);
        d.set_feedback(f32::NAN);
        d.set_mix(f32::INFINITY);
        d.set_delay_fractional(f32::NAN); // ignored
        for i in 0..512 {
            let x = if i % 7 == 0 { f32::NAN } else { 0.5 };
            assert!(d.tick(x).is_finite());
        }
    }

    #[test]
    fn decaying_tail_flushes_to_silence() {
        let mut d = Delay::new(64);
        d.set_delay_samples(4);
        d.reset();
        d.set_feedback(0.5);
        d.set_mix(1.0);
        let _ = d.tick(1.0);
        for _ in 0..2000 {
            let y = d.tick(0.0);
            assert!(y == 0.0 || y.abs() >= FLUSH_THRESHOLD, "subnormal {y}");
        }
    }
}
