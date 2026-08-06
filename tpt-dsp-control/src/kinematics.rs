//! Real-time kinematics: jerk-limited (S-curve) smoothing and trapezoidal
//! point-to-point trajectory planning for stepper / servo drives.
//!
//! [`JerkLimiter`] turns a streaming target position into a smooth motion
//! whose velocity, acceleration **and** jerk are all bounded — essential for
//! mechanically delicate gantries. [`TrapezoidalProfile`] pre-computes the
//! time segments of a minimum-time move between two positions at fixed
//! velocity / acceleration limits.

/// A jerk-limited velocity smoother (S-curve filter) for servo / stepper
/// drives.
///
/// Given a streaming *target velocity* each control step, it produces a velocity
/// whose acceleration is bounded by `max_accel` and whose jerk (the rate of
/// change of acceleration) is bounded by `max_jerk`. Position is the integral
/// of the smoothed velocity. Because the loop is first-order in velocity with
/// rate limits (no position feedback), it is unconditionally stable and never
/// oscillates — the standard primitive for feeding jerk-limited motion commands
/// to a motor controller.
#[derive(Debug, Clone)]
pub struct JerkLimiter {
    max_jerk: f32,
    max_accel: f32,
    max_vel: f32,
    dt: f32,
    pos: f32,
    vel: f32,
    acc: f32,
}

impl JerkLimiter {
    /// Create a limiter.
    ///
    /// * `max_jerk` — maximum jerk (pos/s³).
    /// * `max_accel` — maximum acceleration (pos/s²).
    /// * `max_vel` — maximum velocity (pos/s).
    /// * `dt` — control step (s).
    pub fn new(max_jerk: f32, max_accel: f32, max_vel: f32, dt: f32) -> Self {
        assert!(max_jerk > 0.0 && max_accel > 0.0 && max_vel > 0.0);
        assert!(dt > 0.0);
        Self {
            max_jerk,
            max_accel,
            max_vel,
            dt,
            pos: 0.0,
            vel: 0.0,
            acc: 0.0,
        }
    }

    /// Current position (integral of the smoothed velocity).
    pub fn position(&self) -> f32 {
        self.pos
    }

    /// Current velocity.
    pub fn velocity(&self) -> f32 {
        self.vel
    }

    /// Current acceleration.
    pub fn acceleration(&self) -> f32 {
        self.acc
    }

    /// Advance one step given a `target_velocity` (pos/s), returning
    /// `(position, velocity, acceleration)`. The returned velocity ramps toward
    /// the target with bounded acceleration and jerk; position integrates it.
    pub fn update(&mut self, target_velocity: f32) -> (f32, f32, f32) {
        let dt = self.dt;
        let target = target_velocity.clamp(-self.max_vel, self.max_vel);
        // First-order velocity loop gain. Bounded so that the implied
        // acceleration (K·Δv) never exceeds `max_accel` and its rate of change
        // (K·accel) never exceeds `max_jerk` — i.e. the limiters never bind and
        // the response is a clean, stable exponential with no limit-cycle ring.
        let k = (self.max_jerk / self.max_accel).min(self.max_accel / self.max_vel);
        let desired_acc = k * (target - self.vel);
        // Safety: limit the *change* in acceleration to max_jerk (S-curve).
        let acc_step = (desired_acc - self.acc).clamp(-self.max_jerk * dt, self.max_jerk * dt);
        let mut acc = self.acc + acc_step;
        acc = acc.clamp(-self.max_accel, self.max_accel);
        // Integrate velocity and position.
        let mut vel = self.vel + acc * dt;
        vel = vel.clamp(-self.max_vel, self.max_vel);
        self.pos += vel * dt;
        self.vel = vel;
        self.acc = acc;
        (self.pos, self.vel, self.acc)
    }

    /// Reset state to rest at `position`.
    pub fn reset(&mut self, position: f32) {
        self.pos = position;
        self.vel = 0.0;
        self.acc = 0.0;
    }
}

/// A trapezoidal (constant-acceleration) point-to-point trajectory.
///
/// Pre-computes the accelerate / cruise / decelerate segment durations for a
/// move from `q0` to `q1` at the given velocity and acceleration limits, then
/// evaluates position/velocity/acceleration at any time `t` with
/// [`at`](Self::at).
#[derive(Debug, Clone)]
pub struct TrapezoidalProfile {
    q0: f32,
    q1: f32,
    dir: f32,
    v_peak: f32,
    a_max: f32,
    t_acc: f32,
    t_cruise: f32,
    t_total: f32,
}

impl TrapezoidalProfile {
    /// Plan a move from `q0` to `q1`.
    ///
    /// # Panics
    ///
    /// Panics if `v_max` or `a_max` are not positive, or if `q0 == q1`.
    pub fn new(q0: f32, q1: f32, v_max: f32, a_max: f32) -> Self {
        assert!(v_max > 0.0 && a_max > 0.0, "limits must be positive");
        assert!(q0 != q1, "degenerate move (q0 == q1)");
        let dir = if q1 > q0 { 1.0 } else { -1.0 };
        let dist = (q1 - q0).abs();
        let d_acc = 0.5 * v_max * v_max / a_max; // distance to reach v_max
        let (t_acc, t_cruise, v_peak) = if 2.0 * d_acc <= dist {
            // Trapezoidal: full cruise at v_max.
            (v_max / a_max, (dist - 2.0 * d_acc) / v_max, v_max)
        } else {
            // Triangular: never reaches v_max.
            let t = (dist / a_max).sqrt();
            (t, 0.0, a_max * t)
        };
        let t_total = 2.0 * t_acc + t_cruise;
        Self {
            q0,
            q1,
            dir,
            v_peak,
            a_max,
            t_acc,
            t_cruise,
            t_total,
        }
    }

    /// Total move duration (s).
    pub fn duration(&self) -> f32 {
        self.t_total
    }

    /// Evaluate the state at time `t` seconds: `(position, velocity,
    /// acceleration)`.
    pub fn at(&self, t: f32) -> (f32, f32, f32) {
        if t <= 0.0 {
            return (self.q0, 0.0, 0.0);
        }
        if t >= self.t_total {
            return (self.q1, 0.0, 0.0);
        }
        if t < self.t_acc {
            // Accelerating from rest.
            let acc = self.dir * self.a_max;
            let vel = acc * t;
            let pos = self.q0 + 0.5 * acc * t * t;
            (pos, vel, acc)
        } else if t < self.t_acc + self.t_cruise {
            // Cruising at peak velocity.
            let tc = t - self.t_acc;
            let acc = 0.0;
            let vel = self.dir * self.v_peak;
            let pos = self.q0 + self.dir * (0.5 * self.v_peak * self.t_acc + self.v_peak * tc);
            (pos, vel, acc)
        } else {
            // Decelerating to rest.
            let td = t - self.t_acc - self.t_cruise;
            let acc = -self.dir * self.a_max;
            let vel = self.dir * (self.v_peak - self.a_max * td);
            let remain = self.t_acc - td; // time left in decel
            let pos = self.q1 - self.dir * 0.5 * self.a_max * remain * remain;
            (pos, vel, acc)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jerk_limiter_tracks_velocity_within_limits() {
        let mut jl = JerkLimiter::new(10.0, 5.0, 2.0, 0.001);
        // Drive toward a constant target velocity, then stop. Verify the actual
        // velocity / acceleration / jerk never exceed the limits and the motion is
        // smooth (no oscillation / runaway).
        let mut max_vel = 0.0f32;
        let mut max_acc = 0.0f32;
        let mut max_jerk = 0.0f32;
        let mut prev_acc = 0.0f32;
        let dt = 0.001f32;
        for i in 0..5_000 {
            let target = if i < 2_500 { 2.0 } else { 0.0 };
            let (pos, vel, acc) = jl.update(target);
            let _ = pos;
            max_vel = max_vel.max(vel.abs());
            max_acc = max_acc.max(acc.abs());
            max_jerk = max_jerk.max(((acc - prev_acc) / dt).abs());
            prev_acc = acc;
        }
        assert!(max_vel <= 2.0 + 1e-3, "vel {max_vel}");
        assert!(max_acc <= 5.0 + 1e-3, "acc {max_acc}");
        assert!(max_jerk <= 10.0 + 1e-3, "jerk {max_jerk}");
        // After stopping, velocity and acceleration are back to rest.
        assert!(jl.velocity().abs() < 5e-2);
        assert!(jl.acceleration().abs() < 5e-2);
    }

    #[test]
    fn jerk_limiter_position_matches_integrated_velocity() {
        // At a constant target velocity, position should advance by ~v·t once the
        // velocity has settled.
        let mut jl = JerkLimiter::new(20.0, 20.0, 1.0, 0.001);
        for _ in 0..10_000 {
            jl.update(1.0);
        }
        // After 10 s at ~1 unit/s the position should be close to (10 - ramp).
        assert!(
            jl.position() > 8.5 && jl.position() < 10.0 + 1e-3,
            "pos {}",
            jl.position()
        );
    }

    #[test]
    fn trapezoidal_reaches_goal_with_zero_final_velocity() {
        let prof = TrapezoidalProfile::new(0.0, 10.0, 2.0, 4.0);
        let (p_end, v_end, a_end) = prof.at(prof.duration());
        assert!((p_end - 10.0).abs() < 1e-4, "pos {p_end}");
        assert!((v_end).abs() < 1e-4);
        assert!(a_end.abs() < 1e-4);
        // Midpoint is near the middle of the travel.
        let (p_mid, _, _) = prof.at(prof.duration() / 2.0);
        assert!((p_mid - 5.0).abs() < 1e-2, "mid {p_mid}");
        // Peak velocity never exceeds the limit.
        let mut peak = 0.0f32;
        let steps = (prof.duration() / 0.001).ceil() as usize;
        for i in 0..=steps {
            let (_, v, _) = prof.at(i as f32 * 0.001);
            peak = peak.max(v.abs());
        }
        assert!(peak <= 2.0 + 1e-3, "peak {peak}");
    }

    #[test]
    fn trapezoidal_triangular_when_short() {
        // Short move: never reaches v_max, so it is triangular.
        let prof = TrapezoidalProfile::new(0.0, 0.5, 2.0, 4.0);
        assert!(prof.t_cruise < 1e-6, "cruise {}", prof.t_cruise);
        let (p, v, _a) = prof.at(prof.duration());
        assert!((p - 0.5).abs() < 1e-4);
        assert!(v.abs() < 1e-4);
    }
}
