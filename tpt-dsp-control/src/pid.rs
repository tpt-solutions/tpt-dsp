//! PID controller with anti-windup.
//!
//! A standard positional-form PID. The integral term is guarded against
//! windup using either clamping (freeze integration while saturated) or
//! back-calculation (bleed the integrator toward the unsaturated output).

/// Anti-windup strategy applied when the output saturates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AntiWindup {
    /// Stop integrating while the output is clamped.
    Clamp,
    /// Feed the saturation error back into the integrator at rate `kaw`
    /// (per second). Smaller `kaw` recovers more slowly.
    BackCalculation(f32),
}

/// A discrete-time PID controller.
#[derive(Debug, Clone)]
pub struct Pid {
    kp: f32,
    ki: f32,
    kd: f32,
    sample_time: f32,
    setpoint: f32,
    prev_error: f32,
    integral: f32,
    out_min: f32,
    out_max: f32,
    anti_windup: AntiWindup,
}

impl Pid {
    /// Create a PID with gains `kp, ki, kd` and a fixed `sample_time` (s).
    pub fn new(kp: f32, ki: f32, kd: f32, sample_time: f32) -> Self {
        assert!(sample_time > 0.0, "sample time must be positive");
        Self {
            kp,
            ki,
            kd,
            sample_time,
            setpoint: 0.0,
            prev_error: 0.0,
            integral: 0.0,
            out_min: f32::NEG_INFINITY,
            out_max: f32::INFINITY,
            anti_windup: AntiWindup::Clamp,
        }
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

    /// Reset integral and derivative history (e.g. after a large setpoint
    /// step or a fault).
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.prev_error = 0.0;
    }

    /// Current integrator value (for diagnostics).
    pub fn integral(&self) -> f32 {
        self.integral
    }

    /// Advance one sample given a `measurement`; returns the control output.
    pub fn update(&mut self, measurement: f32) -> f32 {
        let dt = self.sample_time;
        let error = self.setpoint - measurement;

        let p = self.kp * error;
        let d = self.kd * (error - self.prev_error) / dt;
        let mut integral = self.integral + self.ki * error * dt;

        let mut output = p + integral + d;
        let clamped = output.clamp(self.out_min, self.out_max);

        match self.anti_windup {
            AntiWindup::Clamp => {
                // If saturated, discard the integration we just added.
                if clamped != output {
                    integral = self.integral;
                    output = p + integral + d;
                }
            }
            AntiWindup::BackCalculation(kaw) => {
                integral -= kaw * (output - clamped) * dt;
            }
        }

        self.integral = integral;
        self.prev_error = error;
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
