# tpt-dsp-control

> PID control, input shaping and real-time kinematics for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-control` provides the control-theory building blocks needed for real-time
automation: a discrete PID controller with anti-windup, an input shaper that
cancels mechanical resonance, and trajectory planners with bounded velocity,
acceleration and jerk for stepper and servo drives.

All algorithms are written to be called from a real-time loop: they are
deterministic, allocation-free, and stateful via a single struct you update each
tick.

## What's inside

### [`Pid`]

Discrete PID controller with a fixed sample time, output limits, and a selectable
anti-windup strategy via [`AntiWindup`]:

- `AntiWindup::Clamp` — stop integrating while the output is saturated.
- `AntiWindup::BackCalculation(kaw)` — feed the saturation error back into the
  integrator at rate `kaw` per second.

```rust
use tpt_dsp_control::{AntiWindup, Pid};

let mut pid = Pid::new(1.0, 0.1, 0.01, dt); // kp, ki, kd, sample time in seconds
pid.set_output_limits(-10.0, 10.0);
pid.set_anti_windup(AntiWindup::BackCalculation(0.2));
pid.set_setpoint(target);

for measurement in feedback.iter() {
    let u = pid.update(*measurement);
    actuator.set(u);
}
```

### [`InputShaper`]

Zero Vibration Derivative (ZVD) input shaping: a streaming filter that convolves a
command with a set of impulses that cancel the residual oscillation of a 2nd-order
system at a known natural frequency and damping ratio. This removes end-effector
wobble on gantry and Cartesian robots without retuning the controller.

```rust
use tpt_dsp_control::InputShaper;

let mut shaper = InputShaper::new_zvd(natural_freq_rad_s, damping_ratio, sample_rate);
for command in commands.iter() {
    let shaped = shaper.tick(*command); // one sample in, one sample out
    motor.set(shaped);
}
```

### [`JerkLimiter`], [`TrapezoidalProfile`]

Real-time trajectory planning:

- [`JerkLimiter`] tracks a target velocity while bounding jerk, acceleration and
  velocity, for smooth starts and stops that minimise mechanical stress.
- [`TrapezoidalProfile`] plans a point-to-point move with bounded velocity and
  acceleration, and can be sampled at any time.

```rust
use tpt_dsp_control::{JerkLimiter, TrapezoidalProfile};

let mut limiter = JerkLimiter::new(max_jerk, max_accel, max_vel, dt);
let (pos, vel, acc) = limiter.update(target_velocity);

let profile = TrapezoidalProfile::new(start, goal, max_vel, max_accel);
let (pos, vel, acc) = profile.at(t); // position, velocity and acceleration at time t
let total = profile.duration();
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
