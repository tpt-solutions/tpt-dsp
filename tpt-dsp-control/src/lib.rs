//! `tpt-dsp-control` — PID control, input shaping and real-time kinematics.
//!
//! - [`Pid`] — discrete PID with clamping / back-calculation anti-windup.
//! - [`InputShaper`] — ZVD input shaping to cancel mechanical resonance.
//! - [`JerkLimiter`], [`TrapezoidalProfile`] — real-time trajectory planning
//!   with bounded velocity / acceleration / jerk for stepper and servo
//!   drives.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod input_shaping;
mod kinematics;
mod pid;

pub use input_shaping::InputShaper;
pub use kinematics::{JerkLimiter, TrapezoidalProfile};
pub use pid::{AntiWindup, Pid};
