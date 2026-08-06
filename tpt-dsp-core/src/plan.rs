//! High-performance FFT planning via RustFFT (requires `std`).
//!
//! Use [`FftPlan`] when you need transforms of arbitrary (non-power-of-two)
//! length, hardware acceleration (AVX/SSE/NEON), or maximum throughput on
//! desktop targets. The transform buffers and scratch space are allocated
//! once at construction, so each [`process`](FftPlan::process) call is
//! allocation-free.

use std::sync::Arc;

use num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// A pre-planned FFT of fixed length.
///
/// Wrap the planner output once, then call [`process`](Self::process) for
/// every block. Because RustFFT may select length-specific optimizations,
/// constructing a plan is relatively expensive — reuse it.
pub struct FftPlan {
    fft: Arc<dyn Fft<f32>>,
    scratch: std::vec::Vec<Complex<f32>>,
}

impl FftPlan {
    /// Plan a forward transform of `len` samples.
    pub fn new_forward(len: usize) -> Self {
        Self::plan(len, false)
    }

    /// Plan an inverse transform of `len` samples.
    pub fn new_inverse(len: usize) -> Self {
        Self::plan(len, true)
    }

    fn plan(len: usize, inverse: bool) -> Self {
        assert!(len >= 1, "FFT length must be at least 1");
        let mut planner = FftPlanner::new();
        let fft: Arc<dyn Fft<f32>> = if inverse {
            planner.plan_fft_inverse(len)
        } else {
            planner.plan_fft_forward(len)
        };
        let scratch = std::vec![Complex::default(); fft.get_inplace_scratch_len()];
        Self { fft, scratch }
    }

    /// Transform length.
    #[inline]
    pub fn len(&self) -> usize {
        self.fft.len()
    }

    /// `true` if the configured length is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.fft.len() == 0
    }

    /// Apply the transform, writing `input.len()` results into `output`.
    ///
    /// Both buffers must hold `self.len()` elements. Allocation-free.
    pub fn process(&mut self, input: &[Complex<f32>], output: &mut [Complex<f32>]) {
        assert_eq!(input.len(), self.len(), "input length mismatch");
        assert_eq!(output.len(), self.len(), "output length mismatch");
        output.copy_from_slice(input);
        self.fft.process_with_scratch(output, &mut self.scratch);
    }

    /// In-place variant using a caller-provided buffer.
    pub fn process_inplace(&mut self, buffer: &mut [Complex<f32>]) {
        assert_eq!(buffer.len(), self.len(), "buffer length mismatch");
        self.fft.process_with_scratch(buffer, &mut self.scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_matches_reference_fft() {
        use crate::fft::fft_inplace;
        let len = 24; // not a power of two — RustFFT handles it
        let input: Vec<Complex<f32>> = (0..len)
            .map(|i| Complex::new((i as f32 * 0.3).sin(), (i as f32 * 0.1).cos()))
            .collect();

        let mut plan = FftPlan::new_forward(len);
        let mut got = vec![Complex::default(); len];
        plan.process(&input, &mut got);

        // Reference: pad to power of two and compare shared bins.
        let pad = len.next_power_of_two();
        let mut ref_buf: Vec<Complex<f32>> = input.clone();
        ref_buf.resize(pad, Complex::default());
        let mut scratch = vec![Complex::default(); pad];
        fft_inplace(&mut ref_buf, &mut scratch);

        for (k, g) in got.iter().enumerate() {
            assert!(
                (g.re - ref_buf[k].re).abs() < 1e-4 && (g.im - ref_buf[k].im).abs() < 1e-4,
                "bin {k}: got {g} want {}", ref_buf[k]
            );
        }
    }

    #[test]
    fn inverse_roundtrip() {
        let len = 32;
        let input: Vec<Complex<f32>> = (0..len)
            .map(|i| Complex::new((i as f32 * 0.2).sin(), 0.0))
            .collect();
        let mut fwd = FftPlan::new_forward(len);
        let mut inv = FftPlan::new_inverse(len);
        let mut spec = vec![Complex::default(); len];
        let mut back = vec![Complex::default(); len];
        fwd.process(&input, &mut spec);
        inv.process(&spec, &mut back);
        let scale = len as f32;
        for (a, b) in back.iter().zip(input.iter()) {
            assert!((a.re / scale - b.re).abs() < 1e-4);
        }
    }

    #[test]
    fn inplace_matches_out_of_place() {
        let len = 16;
        let input: Vec<Complex<f32>> = (0..len).map(|i| Complex::new(i as f32, -(i as f32))).collect();
        let mut plan = FftPlan::new_forward(len);
        let mut inplace = input.clone();
        plan.process_inplace(&mut inplace);
        let mut oop = vec![Complex::default(); len];
        plan.process(&input, &mut oop);
        for (a, b) in inplace.iter().zip(oop.iter()) {
            assert!((a.re - b.re).abs() < 1e-6 && (a.im - b.im).abs() < 1e-6);
        }
    }
}
