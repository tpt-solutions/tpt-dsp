# tpt-dsp-control

> PID control, input shaping and real-time kinematics for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-control` builds on [`tpt-dsp-core`](../tpt-dsp-core) to provide the
control-theory building blocks needed for real-time automation: a discrete PID
controller with anti-windup, an input shaper that cancels mechanical resonance,
and trajectory planners with bounded velocity / acceleration / jerk for stepper
and servo drives.

All algorithms are written to be called from a real-time loop: they are
deterministic, allocation-free, and stateful via a single struct you update each
tick.

## What's inside

### [`Pid`]

Discrete PID controller with selectable anti-windup strategy via [`AntiWindup`]:

- `clamping` — integrate only when the actuator is not saturated.
- `back_calculation` — bleed off accumulated integral error proportional to
  saturation overshoot.

```rust
use tpt_dsp_control::{Pid, AntiWindup};

let mut pid = Pid::new(1.0, 0.1, 0.01)
    .with_anti_windup(AntiWindup::BackCalculation(0.2))
    .with_output_limits(-10.0, 10.0);

let mut u = 0.0;
for (setpoint, measurement) in references.iter().zip(feedback.iter()) {
    u = pid.update(*setpoint, *measurement, dt);
    actuator.set(u);
}
```

### [`InputShaper`]

Zero Vibration Derivative (ZVD) input shaping: convolves a command profile with
a set of impulses that cancel the residual oscillation of a 2nd-order system at a
known natural frequency and damping ratio — the classic way to remove end-effector
wobble on gantry/cartesian robots without tuning the controller.

```rust
use tpt_dsp_control::InputShaper;

let shaper = InputShaper::zvd(angular_freq, damping_ratio);
let shaped = shaper.shape(&command); // returns the reshaped command sequence
```

### [`JerkLimiter`], [`TrapezoidalProfile`]

Real-time trajectory planning:

- [`TrapezoidalProfile`] generates a position/velocity/acceleration plan with
  bounded velocity and acceleration (a trapezoidal speed profile).
- [`JerkLimiter`] further bounds jerk (the derivative of acceleration) for smooth
  starts/stops that minimise mechanical stress.

```rust
use tpt_dsp_control::TrapezoidalProfile;

let mut profile = TrapezoidalProfile::new(max_vel, max_accel, dt);
profile.move_to(target);
while let Some((pos, vel, acc)) = profile.step() {
    motor.set_position(pos);
}
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
