// SPDX-License-Identifier: MIT OR Apache-2.0

//! {{project-name}} — a real-time-safe DSP effect built on `tpt-dsp-core`.
//!
//! Conventions enforced by this skeleton (see the workspace AGENTS.md):
//!
//! - Anything named `process`/`tick` must not allocate, lock, or syscall.
//! - All scratch buffers are allocated once in [`new`](Effect::new) and reused.
//! - Every public item carries a doc comment; CI runs clippy with
//!   `-D warnings` and `#![warn(missing_docs)]`.
//!
//! Replace the placeholder `GainEffect` below with your own transfer
//! function, keeping the same shape.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

/// A trivial example effect: fixed-gain block processor.
///
/// Demonstrates the two established real-time idioms from the workspace:
/// a struct that allocates all scratch in `new()` and reuses it, and an
/// allocation-free `process` over caller-owned slices.
#[derive(Debug, Clone)]
pub struct GainEffect {
    /// Linear gain applied to every sample.
    gain: f32,
    /// Pre-allocated scratch buffer reused across `process` calls.
    scratch: Vec<f32>,
}

impl GainEffect {
    /// Create the effect. The **only** place this type allocates.
    pub fn new(gain: f32, max_block_size: usize) -> Self {
        Self {
            gain,
            scratch: vec![0.0; max_block_size],
        }
    }

    /// Set the linear gain (real-time safe: no allocation).
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
    }

    /// Process one block in place into `output`. Allocation-free.
    ///
    /// Panics if `output` is longer than the `max_block_size` given to
    /// [`GainEffect::new`] — size your buffers at construction time.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert!(
            input.len() <= self.scratch.len(),
            "block larger than max_block_size",
        );
        // Example use of the pre-allocated scratch: stage the input copy so
        // later stages could operate on it without allocating.
        let staged = &mut self.scratch[..input.len()];
        staged.copy_from_slice(input);
        for (o, x) in output.iter_mut().zip(staged.iter()) {
            *o = x * self.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_gain_block() {
        let mut fx = GainEffect::new(0.5, 128);
        let input = [2.0f32; 64];
        let mut out = [0.0f32; 64];
        fx.process(&input, &mut out);
        assert!(out.iter().all(|&s| s == 1.0));
    }

    #[test]
    fn gain_change_does_not_allocate() {
        let mut fx = GainEffect::new(1.0, 128);
        fx.set_gain(0.25); // plain field write, safe in the audio callback
        let mut out = [0.0f32; 16];
        fx.process(&[1.0; 16], &mut out);
        assert!(out.iter().all(|&s| s == 0.25));
    }
}
