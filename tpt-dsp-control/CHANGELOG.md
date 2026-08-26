# Changelog

All notable changes to `tpt-dsp-control` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

## [0.1.0] - 2026-08-26

### Added
- Initial release of the real-time control-theory crate built on `tpt-dsp-core`.
  All algorithms are deterministic, allocation-free and updated once per tick.
- `pid_speed_hold` example: a PID controller holding a simulated DC motor at a
  100 rpm setpoint with output limiting and back-calculation anti-windup.
- `Pid`: discrete PID controller with selectable anti-windup strategy
  (`AntiWindup::Clamping` / `AntiWindup::BackCalculation`) and output limiting.
- `InputShaper`: Zero Vibration Derivative (ZVD) input shaping that convolves a
  command profile with impulses cancelling residual oscillation of a 2nd-order
  system at a known natural frequency and damping ratio.
- `TrapezoidalProfile`: position/velocity/acceleration planning with bounded
  velocity and acceleration.
- `JerkLimiter`: further bounds jerk (derivative of acceleration) for smooth
  starts/stops that minimise mechanical stress.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
