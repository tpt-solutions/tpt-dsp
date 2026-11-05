//! PID controller with anti-windup.
//!
//! A standard positional-form PID. The integral term is guarded against
//! windup using either clamping (freeze integration while saturated) or
//! back-calculation (bleed the integrator toward the unsaturated output).
//!
//! The derivative term acts on the **measurement**, not the error, so a
//! setpoint step does not produce a derivative kick. An optional first-order
//! derivative filter damps measurement noise, and [`Pid::update_with_dt`]
//! accepts a per-call timestep for irregular loop rates.

/// Anti-windup strategy applied when the output saturates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AntiWindup {
    /// Stop integrating while the output is clamped.
    Clamp,
    /// Feed the saturation error back into the integrator at rate `kaw`
    /// (per second). Smaller `kaw` recovers more slowly.
    BackCalculation(f32),
}

/// Error returned by [`Pid::try_new`] for an unusable parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PidError {
    /// `sample_time` was not a finite, strictly positive number.
    InvalidSampleTime,
    /// A gain was NaN or infinite.
    NonFiniteGain,
}

impl core::fmt::Display for PidError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PidError::InvalidSampleTime => f.write_str("sample time must be finite and positive"),
            PidError::NonFiniteGain => f.write_str("PID gains must be finite"),
        }
    }
}

/// A discrete-time PID controller.
#[derive(Debug, Clone)]
pub struct Pid {
    kp: f32,
    ki: f32,
    kd: f32,
    sample_time: f32,
    setpoint: f32,
    prev_measurement: f32,
    has_prev: bool,
    /// Filtered derivative of the measurement (per second, signed for `-d/dt`).
    d_state: f32,
    /// Derivative filter time constant in seconds; `0` disables filtering.
    derivative_tau: f32,
    integral: f32,
    out_min: f32,
    out_max: f32,
    anti_windup: AntiWindup,
}

impl Pid {
    /// Create a PID with gains `kp, ki, kd` and a fixed `sample_time` (s).
    ///
    /// # Panics
    ///
    /// Panics if `sample_time` is not finite and positive. Use
    /// [`Pid::try_new`] to get an error instead.
    pub fn new(kp: f32, ki: f32, kd: f32, sample_time: f32) -> Self {
        Self::try_new(kp, ki, kd, sample_time).expect("invalid PID parameters")
    }

    /// Fallible constructor: rejects a non-finite or non-positive
    /// `sample_time`, and non-finite gains.
    pub fn try_new(kp: f32, ki: f32, kd: f32, sample_time: f32) -> Result<Self, PidError> {
        if !(sample_time.is_finite() && sample_time > 0.0) {
            return Err(PidError::InvalidSampleTime);
        }
        if !(kp.is_finite() && ki.is_finite() && kd.is_finite()) {
            return Err(PidError::NonFiniteGain);
        }
        Ok(Self {
            kp,
            ki,
            kd,
            sample_time,
            setpoint: 0.0,
            prev_measurement: 0.0,
            has_prev: false,
            d_state: 0.0,
            derivative_tau: 0.0,
            integral: 0.0,
            out_min: f32::NEG_INFINITY,
            out_max: f32::INFINITY,
            anti_windup: AntiWindup::Clamp,
        })
    }

    /// Set the controller target.
    pub fn set_setpoint(&mut self, setpoint: f32) {
        self.setpoint = setpoint;
    }

    /// The current target.
    pub fn setpoint(&self) -> f32 {
        self.setpoint
    }

    /// Clamp the output to `[min, max]`.
    pub fn set_output_limits(&mut self, min: f32, max: f32) {
        assert!(min <= max, "min must be <= max");
        self.out_min = min;
        self.out_max = max;
    }

    /// Choose an anti-windup strategy.
    pub fn set_anti_windup(&mut self, mode: AntiWindup) {
        self.anti_windup = mode;
    }

    /// Set the derivative filter time constant in seconds. `0` (the default)
    /// leaves the derivative unfiltered. Non-finite or negative values are
    /// treated as `0`.
    pub fn set_derivative_filter(&mut self, tau: f32) {
        self.derivative_tau = if tau.is_finite() && tau > 0.0 {
            tau
        } else {
            0.0
        };
    }

    /// Reset integral and derivative history (e.g. after a large setpoint
    /// step or a fault).
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_measurement = 0.0;
        self.has_prev = false;
        self.d_state = 0.0;
    }

    /// Current integrator value (for diagnostics).
    pub fn integral(&self) -> f32 {
        self.integral
    }

    /// Advance one sample given a `measurement`, using the configured sample
    /// time; returns the control output.
    pub fn update(&mut self, measurement: f32) -> f32 {
        self.update_with_dt(measurement, self.sample_time)
    }

    /// Advance one sample with an explicit timestep `dt` (seconds), for loops
    /// whose rate varies. A non-finite or non-positive `dt` falls back to the
    /// configured sample time.
    pub fn update_with_dt(&mut self, measurement: f32, dt: f32) -> f32 {
        let dt = if dt.is_finite() && dt > 0.0 {
            dt
        } else {
            self.sample_time
        };
        let error = self.setpoint - measurement;

        // Derivative on measurement: -d(y)/dt. A setpoint step leaves it
        // untouched, so there is no derivative kick.
        let raw_d = if self.has_prev {
            -(measurement - self.prev_measurement) / dt
        } else {
            0.0
        };
        let alpha = if self.derivative_tau > 0.0 {
            dt / (self.derivative_tau + dt)
        } else {
            1.0
        };
        self.d_state += alpha * (raw_d - self.d_state);

        let p = self.kp * error;
        let d = self.kd * self.d_state;
        let mut integral = self.integral + self.ki * error * dt;

        let output = p + integral + d;
        let clamped = output.clamp(self.out_min, self.out_max);

        match self.anti_windup {
            AntiWindup::Clamp => {
                // If saturated, discard the integration we just added.
                if clamped != output {
                    integral = self.integral;
                }
            }
            AntiWindup::BackCalculation(kaw) => {
                integral -= kaw * (output - clamped) * dt;
            }
        }

        self.integral = integral;
        self.prev_measurement = measurement;
        self.has_prev = true;
        clamped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_to_setpoint() {
        let mut pid = Pid::new(2.0, 1.0, 0.1, 0.01);
        pid.set_setpoint(10.0);
        let mut y = 0.0f32;
        for _ in 0..2000 {
            let u = pid.update(y);
            y += u * 0.01; // simple plant: y' = u
        }
        assert!((y - 10.0).abs() < 0.5, "y {y}");
    }

    #[test]
    fn setpoint_step_causes_no_derivative_kick() {
        // Pure derivative action with a constant measurement: a setpoint step
        // must not produce any output (the old error-based derivative kicked
        // by kd * step / dt here).
        let mut pid = Pid::new(0.0, 0.0, 1.0, 0.01);
        pid.update(0.0);
        pid.set_setpoint(100.0);
        let u = pid.update(0.0);
        assert!(u.abs() < 1e-6, "kick of {u}");
    }

    #[test]
    fn derivative_responds_to_measurement_motion() {
        let mut pid = Pid::new(0.0, 0.0, 1.0, 0.1);
        pid.update(0.0);
        // Measurement rises by 1 over dt = 0.1 s → d/dt = 10 → -kd * 10.
        let u = pid.update(1.0);
        assert!((u + 10.0).abs() < 1e-4, "u {u}");
    }

    #[test]
    fn update_with_dt_uses_the_given_timestep() {
        let mut a = Pid::new(0.0, 1.0, 0.0, 0.01);
        let mut b = Pid::new(0.0, 1.0, 0.0, 0.01);
        a.set_setpoint(1.0);
        b.set_setpoint(1.0);
        let ua = a.update_with_dt(0.0, 0.1);
        let ub = b.update(0.0);
        assert!((ua - 10.0 * ub).abs() < 1e-6, "{ua} vs {ub}");
        // Invalid dt falls back to the configured sample time.
        let mut c = Pid::new(0.0, 1.0, 0.0, 0.01);
        c.set_setpoint(1.0);
        assert!((c.update_with_dt(0.0, f32::NAN) - ub).abs() < 1e-9);
    }

    #[test]
    fn derivative_filter_attenuates_noise_spikes() {
        let mut raw = Pid::new(0.0, 0.0, 1.0, 0.001);
        let mut filtered = Pid::new(0.0, 0.0, 1.0, 0.001);
        filtered.set_derivative_filter(0.01);
        raw.update(0.0);
        filtered.update(0.0);
        let r = raw.update(1.0).abs();
        let f = filtered.update(1.0).abs();
        assert!(f < r * 0.5, "filtered {f} raw {r}");
    }

    #[test]
    fn try_new_rejects_bad_parameters() {
        assert_eq!(
            Pid::try_new(1.0, 0.0, 0.0, 0.0).unwrap_err(),
            PidError::InvalidSampleTime
        );
        assert_eq!(
            Pid::try_new(1.0, 0.0, 0.0, f32::NAN).unwrap_err(),
            PidError::InvalidSampleTime
        );
        assert_eq!(
            Pid::try_new(f32::INFINITY, 0.0, 0.0, 0.01).unwrap_err(),
            PidError::NonFiniteGain
        );
        assert!(Pid::try_new(1.0, 0.5, 0.0, 0.01).is_ok());
    }

    #[test]
    fn output_is_clamped() {
        let mut pid = Pid::new(10.0, 5.0, 0.0, 0.01);
        pid.set_output_limits(-1.0, 1.0);
        pid.set_setpoint(100.0);
        for _ in 0..100 {
            let u = pid.update(0.0);
            assert!((-1.0..=1.0).contains(&u), "u {u}");
        }
    }

    #[test]
    fn back_calculation_stays_bounded_and_recovers() {
        let mut pid = Pid::new(5.0, 2.0, 0.05, 0.01);
        pid.set_output_limits(-1.0, 1.0);
        pid.set_anti_windup(AntiWindup::BackCalculation(1.0));
        pid.set_setpoint(50.0);
        // Saturated region — integrator must not blow up.
        for _ in 0..500 {
            let u = pid.update(0.0);
            assert!((-1.0..=1.0).contains(&u));
        }
        // Now relax the setpoint; output should recover smoothly.
        pid.set_setpoint(0.0);
        let u = pid.update(0.0);
        assert!((-1.0..=1.0).contains(&u));
    }
}
