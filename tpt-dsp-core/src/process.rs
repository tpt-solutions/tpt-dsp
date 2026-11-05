//! A common processing interface, a static [`Chain`] combinator, and a
//! click-free parameter smoother.
//!
//! [`Processor`] is the shared shape of the single-sample filters in this
//! crate: `process` one sample, optionally `process_block` in place, and
//! `reset` the state. [`Chain`] composes two processors with static dispatch,
//! so no allocation or virtual call is involved.
//!
//! [`Smoothed`] turns a jumpy control value (a slider, a MIDI controller) into
//! a glide, so parameters can change without zipper noise.

// `no_std` builds have no inherent float math; see `windows.rs`.
#[allow(unused_imports)]
use num_traits::Float;

/// A single-channel sample processor.
pub trait Processor {
    /// Process one sample.
    fn process(&mut self, x: f32) -> f32;

    /// Process a block in place. The default calls [`process`](Self::process)
    /// for each sample.
    fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process(*s);
        }
    }

    /// Clear the internal state.
    fn reset(&mut self) {}
}

/// Two processors in series: `b` consumes the output of `a`.
#[derive(Debug, Clone)]
pub struct Chain<A, B> {
    /// First stage.
    pub a: A,
    /// Second stage.
    pub b: B,
}

impl<A, B> Chain<A, B> {
    /// Series composition of `a` then `b`.
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<A: Processor, B: Processor> Processor for Chain<A, B> {
    fn process(&mut self, x: f32) -> f32 {
        let y = self.a.process(x);
        self.b.process(y)
    }

    fn reset(&mut self) {
        self.a.reset();
        self.b.reset();
    }
}

impl Processor for crate::Biquad<f32> {
    fn process(&mut self, x: f32) -> f32 {
        self.tick(x)
    }

    fn reset(&mut self) {
        crate::Biquad::reset(self);
    }
}

impl Processor for crate::OnePole {
    fn process(&mut self, x: f32) -> f32 {
        self.tick(x)
    }

    fn reset(&mut self) {
        crate::OnePole::reset(self);
    }
}

impl Processor for crate::DcBlocker {
    fn process(&mut self, x: f32) -> f32 {
        self.tick(x)
    }

    fn reset(&mut self) {
        crate::DcBlocker::reset(self);
    }
}

/// A parameter that glides toward its target with a one-pole response.
///
/// `time_s` is the time to cover about 63% of a step. A zero time means the
/// value jumps immediately. Non-finite targets are ignored.
#[derive(Debug, Clone, Copy)]
pub struct Smoothed<F: Float> {
    current: F,
    target: F,
    coeff: F,
}

impl<F: Float> Smoothed<F> {
    /// Create a smoother starting at `initial`.
    pub fn new(initial: F, time_s: F, sample_rate: F) -> Self {
        let coeff = if time_s <= F::zero() || sample_rate <= F::zero() {
            F::zero()
        } else {
            (-F::one() / (time_s * sample_rate)).exp()
        };
        Self {
            current: initial,
            target: initial,
            coeff,
        }
    }

    /// Set the target value. Non-finite values are ignored.
    pub fn set_target(&mut self, target: F) {
        if target.is_finite() {
            self.target = target;
        }
    }

    /// The current target.
    pub fn target(&self) -> F {
        self.target
    }

    /// The value as of the last [`advance`](Self::advance) call.
    pub fn value(&self) -> F {
        self.current
    }

    /// Advance one sample and return the smoothed value.
    pub fn advance(&mut self) -> F {
        self.current = self.target + self.coeff * (self.current - self.target);
        self.current
    }

    /// Jump straight to the target (for example when a voice starts).
    pub fn snap(&mut self) {
        self.current = self.target;
    }

    /// `true` once the value is within `tol` of the target.
    pub fn is_settled(&self, tol: F) -> bool {
        (self.current - self.target).abs() <= tol
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DcBlocker, OnePole};

    #[test]
    fn chain_matches_manual_series_application() {
        let mut chain = Chain::new(
            OnePole::low_pass(48_000.0, 1000.0).unwrap(),
            DcBlocker::new(48_000.0, 10.0).unwrap(),
        );
        let mut a = OnePole::low_pass(48_000.0, 1000.0).unwrap();
        let mut b = DcBlocker::new(48_000.0, 10.0).unwrap();
        for i in 0..500 {
            let x = (i as f32 * 0.1).sin();
            let want = b.tick(a.tick(x));
            assert!((chain.process(x) - want).abs() < 1e-6);
        }
    }

    #[test]
    fn process_block_equals_per_sample() {
        let mut a = OnePole::low_pass(48_000.0, 500.0).unwrap();
        let mut b = OnePole::low_pass(48_000.0, 500.0).unwrap();
        let mut block: Vec<f32> = (0..256).map(|i| (i as f32 * 0.05).cos()).collect();
        let reference: Vec<f32> = block.iter().map(|&x| a.tick(x)).collect();
        b.process_block(&mut block);
        for (x, y) in block.iter().zip(reference.iter()) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    #[test]
    fn reset_clears_the_chain() {
        let mut c = Chain::new(
            OnePole::low_pass(48_000.0, 100.0).unwrap(),
            OnePole::low_pass(48_000.0, 100.0).unwrap(),
        );
        for _ in 0..100 {
            c.process(1.0);
        }
        c.reset();
        assert_eq!(c.a.tick(0.0), 0.0);
    }

    #[test]
    fn smoother_reaches_about_63_percent_after_its_time_constant() {
        let sr = 1000.0f32;
        let mut s = Smoothed::new(0.0f32, 0.1, sr);
        s.set_target(1.0);
        let mut y = 0.0;
        for _ in 0..100 {
            y = s.advance();
        }
        assert!((y - 0.632).abs() < 0.02, "{y}");
        for _ in 0..2000 {
            s.advance();
        }
        assert!(s.is_settled(1e-3));
    }

    #[test]
    fn smoother_zero_time_jumps_and_nan_is_ignored() {
        let mut s = Smoothed::new(0.0f64, 0.0, 48_000.0);
        s.set_target(2.0);
        assert_eq!(s.advance(), 2.0);
        s.set_target(f64::NAN);
        assert_eq!(s.target(), 2.0);
    }
}
