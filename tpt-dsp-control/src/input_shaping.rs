//! Input shaping for mechanical resonance cancellation.
//!
//! Implements the Zero Vibration Derivative (ZVD) shaper for a second-order
//! system (natural frequency `wn`, damping `zeta`). The shaper convolves the
//! command with three impulses spaced over one damped period, so the
//! resulting motion arrives at the target with negligible residual vibration.
//! It is a fixed-coefficient FIR running on a pre-allocated delay line, so it
//! is allocation-free and real-time safe.

/// An input shaper that reshapes a command stream to suppress ringing.
#[derive(Debug, Clone)]
pub struct InputShaper {
    a0: f32,
    a1: f32,
    a2: f32,
    buf: Vec<f32>,
    pos: usize,
    d1: usize,
    d2: usize,
}

impl InputShaper {
    /// Build a Zero Vibration Derivative (ZVD) shaper.
    ///
    /// * `wn` — natural frequency (rad/s).
    /// * `zeta` — damping ratio (`0..1`).
    /// * `sample_rate` — control sample rate (Hz).
    ///
    /// # Panics
    ///
    /// Panics if `wn <= 0`, `zeta` is outside `[0, 1)` or `sample_rate <= 0`.
    pub fn new_zvd(wn: f32, zeta: f32, sample_rate: f32) -> Self {
        assert!(wn > 0.0, "natural frequency must be positive");
        assert!((0.0..1.0).contains(&zeta), "damping must be in [0, 1)");
        assert!(sample_rate > 0.0, "sample rate must be positive");

        let wd = wn * (1.0 - zeta * zeta).sqrt();
        let td = 2.0 * core::f32::consts::PI / wd; // damped period
        let k = (-zeta * core::f32::consts::PI / (1.0 - zeta * zeta).sqrt()).exp();
        // ZVD amplitudes: 1 : K : K^2 (sum to 1 → unity DC gain).
        let denom = 1.0 + k + k * k;
        let a0 = 1.0 / denom;
        let a1 = k / denom;
        let a2 = k * k / denom;

        let d2 = (td * sample_rate).round() as usize;
        let d1 = (d2 / 2).max(1);
        let len = d2 + 1;
        Self {
            a0,
            a1,
            a2,
            buf: vec![0.0; len],
            pos: 0,
            d1,
            d2,
        }
    }

    /// Sum of impulse amplitudes (should be 1.0 — unity DC gain).
    pub fn gain(&self) -> f32 {
        self.a0 + self.a1 + self.a2
    }

    /// Clear the delay line.
    pub fn reset(&mut self) {
        for s in self.buf.iter_mut() {
            *s = 0.0;
        }
        self.pos = 0;
    }

    /// Feed one command sample and return the shaped command.
    pub fn tick(&mut self, command: f32) -> f32 {
        let len = self.buf.len();
        self.buf[self.pos] = command;
        let c1 = self.buf[(self.pos + len - self.d1) % len];
        let c2 = self.buf[(self.pos + len - self.d2) % len];
        let out = self.a0 * command + self.a1 * c1 + self.a2 * c2;
        self.pos = (self.pos + 1) % len;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_gains_sum_to_one() {
        let shaper = InputShaper::new_zvd(10.0, 0.1, 1000.0);
        assert!((shaper.gain() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn is_linear() {
        let mut s1 = InputShaper::new_zvd(10.0, 0.0, 1000.0);
        let mut s2 = InputShaper::new_zvd(10.0, 0.0, 1000.0);
        for _ in 0..50 {
            let a = s1.tick(2.0);
            let b = s2.tick(1.0);
            assert!((a - 2.0 * b).abs() < 1e-6, "{a} vs {b}");
        }
    }

    #[test]
    fn step_reaches_final_value() {
        let mut s = InputShaper::new_zvd(5.0, 0.05, 500.0);
        let mut last = 0.0f32;
        // Buffer length is ~one damped period; run well past it to settle.
        for _ in 0..800 {
            last = s.tick(1.0);
        }
        assert!((last - 1.0).abs() < 1e-4, "last {last}");
    }

    #[test]
    fn undamped_shaper_has_three_equal_impulses() {
        let s = InputShaper::new_zvd(10.0, 0.0, 1000.0);
        // zeta = 0 => k = 1 => all amplitudes equal 1/3.
        assert!((s.a0 - 1.0 / 3.0).abs() < 1e-6);
        assert!((s.a1 - 1.0 / 3.0).abs() < 1e-6);
        assert!((s.a2 - 1.0 / 3.0).abs() < 1e-6);
    }
}
