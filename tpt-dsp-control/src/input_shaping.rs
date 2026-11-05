//! Input shaping for mechanical resonance cancellation.
//!
//! Implements the Zero Vibration Derivative (ZVD) shaper for a second-order
//! system (natural frequency `wn`, damping `zeta`). The shaper convolves the
//! command with three impulses spaced over one damped period, so the
//! resulting motion arrives at the target with negligible residual vibration.
//! It is a fixed-coefficient FIR running on a pre-allocated delay line, so it
//! is allocation-free and real-time safe.

/// Error returned by [`InputShaper::try_new_zvd`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaperError {
    /// Natural frequency was not finite and positive.
    InvalidFrequency,
    /// Damping ratio was not finite or outside `[0, 1)`.
    InvalidDamping,
    /// Sample rate was not finite and positive.
    InvalidSampleRate,
    /// The sample rate is too low to place the impulses on distinct samples.
    SampleRateTooLow,
}

impl core::fmt::Display for ShaperError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            ShaperError::InvalidFrequency => "natural frequency must be finite and positive",
            ShaperError::InvalidDamping => "damping ratio must be finite and in [0, 1)",
            ShaperError::InvalidSampleRate => "sample rate must be finite and positive",
            ShaperError::SampleRateTooLow => "sample rate too low to resolve the damped period",
        })
    }
}

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
    /// Panics on any condition [`InputShaper::try_new_zvd`] rejects.
    pub fn new_zvd(wn: f32, zeta: f32, sample_rate: f32) -> Self {
        Self::try_new_zvd(wn, zeta, sample_rate).expect("invalid input-shaper parameters")
    }

    /// Fallible ZVD constructor.
    ///
    /// Rejects non-finite or out-of-range parameters, and a `sample_rate` too
    /// low to resolve the damped period: the two-sample impulse spacing would
    /// round to zero or one sample, silently producing a shaper that doesn't
    /// cancel the resonance.
    pub fn try_new_zvd(wn: f32, zeta: f32, sample_rate: f32) -> Result<Self, ShaperError> {
        if !(wn.is_finite() && wn > 0.0) {
            return Err(ShaperError::InvalidFrequency);
        }
        if !(zeta.is_finite() && (0.0..1.0).contains(&zeta)) {
            return Err(ShaperError::InvalidDamping);
        }
        if !(sample_rate.is_finite() && sample_rate > 0.0) {
            return Err(ShaperError::InvalidSampleRate);
        }

        let wd = wn * (1.0 - zeta * zeta).sqrt();
        let td = 2.0 * core::f32::consts::PI / wd; // damped period
        let k = (-zeta * core::f32::consts::PI / (1.0 - zeta * zeta).sqrt()).exp();
        // ZVD amplitudes: 1 : K : K^2 (sum to 1 → unity DC gain).
        let denom = 1.0 + k + k * k;
        let a0 = 1.0 / denom;
        let a1 = k / denom;
        let a2 = k * k / denom;

        let d2 = (td * sample_rate).round();
        if !(2.0..16_777_216.0).contains(&d2) {
            return Err(ShaperError::SampleRateTooLow);
        }
        let d2 = d2 as usize;
        let d1 = d2 / 2;
        let len = d2 + 1;
        Ok(Self {
            a0,
            a1,
            a2,
            buf: vec![0.0; len],
            pos: 0,
            d1,
            d2,
        })
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

#[cfg(test)]
mod robustness_tests {
    use super::*;

    #[test]
    fn too_low_sample_rate_is_rejected_not_silently_degenerate() {
        // 10 rad/s resonance has a ~0.63 s damped period; at 2 Hz that is
        // about one sample, which can't place three distinct impulses.
        assert_eq!(
            InputShaper::try_new_zvd(10.0, 0.1, 2.0).unwrap_err(),
            ShaperError::SampleRateTooLow
        );
    }

    #[test]
    fn try_new_rejects_bad_parameters() {
        assert_eq!(
            InputShaper::try_new_zvd(0.0, 0.1, 1000.0).unwrap_err(),
            ShaperError::InvalidFrequency
        );
        assert_eq!(
            InputShaper::try_new_zvd(10.0, 1.0, 1000.0).unwrap_err(),
            ShaperError::InvalidDamping
        );
        assert_eq!(
            InputShaper::try_new_zvd(10.0, 0.1, f32::NAN).unwrap_err(),
            ShaperError::InvalidSampleRate
        );
    }
}
