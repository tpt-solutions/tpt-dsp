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
    /// The output limits were NaN, or `min` was greater than `max`.
    InvalidOutputLimits,
}

impl core::fmt::Display for PidError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PidError::InvalidSampleTime => f.write_str("sample time must be finite and positive"),
            PidError::NonFiniteGain => f.write_str("PID gains must be finite"),
            PidError::InvalidOutputLimits => {
                f.write_str("output limits must be ordered, with min <= max")
            }
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

    /// Start a [`PidBuilder`] with the given `sample_time` (seconds). Gains
    /// default to zero and the output is unbounded.
    pub fn builder(sample_time: f32) -> PidBuilder {
        PidBuilder::new(sample_time)
    }
}

/// Builder for [`Pid`], for setting every option in one expression.
///
/// Only the sample time, given to [`Pid::builder`], is required. Settings that
/// are invalid are reported by [`PidBuilder::build`] rather than applied
/// silently, which is what the setters on [`Pid`] do for some of them.
///
/// ```
/// use tpt_dsp_control::{AntiWindup, Pid};
///
/// let pid = Pid::builder(0.01)
///     .kp(2.0)
///     .ki(0.5)
///     .kd(0.1)
///     .setpoint(10.0)
///     .output_limits(-1.0, 1.0)
///     .anti_windup(AntiWindup::BackCalculation(1.0))
///     .derivative_filter(0.02)
///     .build()
///     .unwrap();
/// assert_eq!(pid.setpoint(), 10.0);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PidBuilder {
    kp: f32,
    ki: f32,
    kd: f32,
    sample_time: f32,
    setpoint: f32,
    output_limits: Option<(f32, f32)>,
    anti_windup: AntiWindup,
    derivative_tau: f32,
}

impl PidBuilder {
    fn new(sample_time: f32) -> Self {
        Self {
            kp: 0.0,
            ki: 0.0,
            kd: 0.0,
            sample_time,
            setpoint: 0.0,
            output_limits: None,
            anti_windup: AntiWindup::Clamp,
            derivative_tau: 0.0,
        }
    }

    /// Proportional gain.
    pub fn kp(mut self, kp: f32) -> Self {
        self.kp = kp;
        self
    }

    /// Integral gain.
    pub fn ki(mut self, ki: f32) -> Self {
        self.ki = ki;
        self
    }

    /// Derivative gain.
    pub fn kd(mut self, kd: f32) -> Self {
        self.kd = kd;
        self
    }

    /// Initial setpoint.
    pub fn setpoint(mut self, setpoint: f32) -> Self {
        self.setpoint = setpoint;
        self
    }

    /// Clamp the output to `[min, max]`. `build` fails if `min > max` or
    /// either bound is NaN.
    pub fn output_limits(mut self, min: f32, max: f32) -> Self {
        self.output_limits = Some((min, max));
        self
    }

    /// Anti-windup strategy used while the output is saturated.
    pub fn anti_windup(mut self, mode: AntiWindup) -> Self {
        self.anti_windup = mode;
        self
    }

    /// Derivative filter time constant in seconds. Non-finite or negative
    /// values disable the filter, as [`Pid::set_derivative_filter`] does.
    pub fn derivative_filter(mut self, tau: f32) -> Self {
        self.derivative_tau = tau;
        self
    }

    /// Validate the settings and build the controller.
    ///
    /// # Errors
    ///
    /// Returns [`PidError::InvalidSampleTime`] or [`PidError::NonFiniteGain`]
    /// as [`Pid::try_new`] does, or [`PidError::InvalidOutputLimits`] for
    /// unordered output limits.
    pub fn build(self) -> Result<Pid, PidError> {
        let mut pid = Pid::try_new(self.kp, self.ki, self.kd, self.sample_time)?;
        if let Some((min, max)) = self.output_limits {
            if min.is_nan() || max.is_nan() || min > max {
                return Err(PidError::InvalidOutputLimits);
            }
            pid.out_min = min;
            pid.out_max = max;
        }
        pid.setpoint = self.setpoint;
        pid.anti_windup = self.anti_windup;
        pid.set_derivative_filter(self.derivative_tau);
        Ok(pid)
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

    #[test]
    fn builder_matches_the_setter_api() {
        let mut built = Pid::builder(0.01)
            .kp(2.0)
            .ki(1.0)
            .kd(0.1)
            .setpoint(5.0)
            .output_limits(-3.0, 3.0)
            .anti_windup(AntiWindup::BackCalculation(0.5))
            .derivative_filter(0.02)
            .build()
            .unwrap();
        let mut set = Pid::new(2.0, 1.0, 0.1, 0.01);
        set.set_setpoint(5.0);
        set.set_output_limits(-3.0, 3.0);
        set.set_anti_windup(AntiWindup::BackCalculation(0.5));
        set.set_derivative_filter(0.02);
        for i in 0..200 {
            let y = (i as f32 * 0.07).sin();
            assert_eq!(built.update(y), set.update(y), "step {i}");
        }
    }

    #[test]
    fn builder_reports_invalid_settings() {
        assert_eq!(
            Pid::builder(0.0).build().unwrap_err(),
            PidError::InvalidSampleTime
        );
        assert_eq!(
            Pid::builder(0.01).kp(f32::NAN).build().unwrap_err(),
            PidError::NonFiniteGain
        );
        assert_eq!(
            Pid::builder(0.01)
                .output_limits(1.0, -1.0)
                .build()
                .unwrap_err(),
            PidError::InvalidOutputLimits
        );
        assert_eq!(
            Pid::builder(0.01)
                .output_limits(f32::NAN, 1.0)
                .build()
                .unwrap_err(),
            PidError::InvalidOutputLimits
        );
    }

    #[test]
    fn builder_defaults_are_an_unbounded_zero_gain_controller() {
        let mut pid = Pid::builder(0.01).build().unwrap();
        pid.set_setpoint(1000.0);
        assert_eq!(pid.update(0.0), 0.0);
    }
}
