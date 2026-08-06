//! Convolution — direct and FFT-accelerated.
//!
//! [`convolve`] is a straightforward O(N·M) direct convolution that works
//! for any lengths and never allocates. For long signals use
//! [`ConvolvePlan`] (FFT-based) or [`FftConvolver`] for a streaming
//! zero-alloc real-time block convolver.

use num_traits::Float;

#[cfg(feature = "alloc")]
use crate::fft::{fft_inplace, ifft_inplace, next_power_of_two};
#[cfg(feature = "alloc")]
use num_complex::Complex;

/// Direct convolution of `input` with `kernel`.
///
/// `out` must be at least `input.len() + kernel.len() - 1` long; the full
/// convolution is written starting at index 0. Allocation-free.
pub fn convolve<F: Float>(input: &[F], kernel: &[F], out: &mut [F]) {
    let total = input.len() + kernel.len() - 1;
    assert!(out.len() >= total, "output too small for convolution");
    for o in out[..total].iter_mut() {
        *o = F::zero();
    }
    for (n, &x) in input.iter().enumerate() {
        for (m, &h) in kernel.iter().enumerate() {
            out[n + m] = out[n + m] + x * h;
        }
    }
}

/// FFT-based full-convolution plan for repeated convolutions of a fixed
/// block length.
///
/// Requires the `alloc` feature. Allocates all buffers once; each
/// [`convolve`](Self::convolve) call is allocation-free.
#[cfg(feature = "alloc")]
pub struct ConvolvePlan<F: Float> {
    fft_len: usize,
    out_len: usize,
    scratch: alloc::vec::Vec<Complex<F>>,
    work: alloc::vec::Vec<Complex<F>>,
    kernel_spec: alloc::vec::Vec<Complex<F>>,
}

#[cfg(feature = "alloc")]
impl<F: Float + Default> ConvolvePlan<F> {
    /// Build a plan that convolves blocks of `block_len` samples against
    /// `kernel`. Output blocks are `block_len + kernel.len() - 1` long.
    pub fn new(kernel: &[F], block_len: usize) -> Self {
        let out_len = block_len + kernel.len() - 1;
        let fft_len = next_power_of_two(out_len.max(1));
        let mut scratch = alloc::vec![Complex::default(); fft_len];
        let work = alloc::vec![Complex::default(); fft_len];
        let mut kernel_spec = alloc::vec![Complex::default(); fft_len];
        for (k, &x) in kernel.iter().enumerate() {
            kernel_spec[k] = Complex::new(x, F::zero());
        }
        fft_inplace(&mut kernel_spec, &mut scratch);
        Self {
            fft_len,
            out_len,
            scratch,
            work,
            kernel_spec,
        }
    }

    /// Convolve one block. `input` must have at most `block_len` samples
    /// (from construction) and `output` must hold at least `out_len`
    /// elements. Allocation-free.
    pub fn convolve(&mut self, input: &[F], output: &mut [F]) {
        assert!(input.len() < self.fft_len, "block too large for this plan");
        assert!(output.len() >= self.out_len, "output too small");
        for (z, &x) in self.work.iter_mut().zip(input.iter()) {
            *z = Complex::new(x, F::zero());
        }
        for z in self.work[input.len()..].iter_mut() {
            *z = Complex::default();
        }
        fft_inplace(&mut self.work, &mut self.scratch);
        for (z, &h) in self.work.iter_mut().zip(self.kernel_spec.iter()) {
            *z = *z * h;
        }
        ifft_inplace(&mut self.work, &mut self.scratch);
        for (o, z) in output[..self.out_len].iter_mut().zip(self.work.iter()) {
            *o = z.re;
        }
    }
}

/// Streaming overlap-add FFT convolver for real-time block processing.
///
/// Requires the `alloc` feature. Construct once with the impulse response;
/// each [`process`](Self::process) call is allocation-free.
#[cfg(feature = "alloc")]
pub struct FftConvolver<F: Float> {
    fft_len: usize,
    block_len: usize,
    scratch: alloc::vec::Vec<Complex<F>>,
    work: alloc::vec::Vec<Complex<F>>,
    kernel_spec: alloc::vec::Vec<Complex<F>>,
    tail: alloc::vec::Vec<F>,
}

#[cfg(feature = "alloc")]
impl<F: Float + Default> FftConvolver<F> {
    /// Create a convolver for `block_len`-sized blocks using an FFT size
    /// of `2 * next_power_of_two(block_len)`.
    ///
    /// # Panics
    ///
    /// Panics if `block_len` is zero.
    pub fn new(kernel: &[F], block_len: usize) -> Self {
        assert!(block_len > 0, "block_len must be nonzero");
        let fft_len = next_power_of_two(block_len * 2).max(1);
        let mut scratch = alloc::vec![Complex::default(); fft_len];
        let work = alloc::vec![Complex::default(); fft_len];
        let mut kernel_spec = alloc::vec![Complex::default(); fft_len];
        for (k, &x) in kernel.iter().take(fft_len).enumerate() {
            kernel_spec[k] = Complex::new(x, F::zero());
        }
        fft_inplace(&mut kernel_spec, &mut scratch);
        Self {
            fft_len,
            block_len,
            scratch,
            work,
            kernel_spec,
            tail: alloc::vec![F::zero(); block_len],
        }
    }

    /// Block length this convolver was configured for.
    #[inline]
    pub fn block_len(&self) -> usize {
        self.block_len
    }

    /// FFT size used internally for the overlap-add convolution.
    #[inline]
    pub fn fft_len(&self) -> usize {
        self.fft_len
    }

    /// Process one block of `block_len()` samples, producing `block_len()`
    /// output samples (the overlap-add tail is carried internally).
    /// `output` must have at least `block_len()` elements. Allocation-free.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        assert!(input.len() == self.block_len, "wrong input block size");
        assert!(output.len() >= self.block_len, "output too small");
        let block_len = self.block_len;

        for (z, &x) in self.work.iter_mut().zip(input.iter()) {
            *z = Complex::new(x, F::zero());
        }
        for z in self.work[block_len..].iter_mut() {
            *z = Complex::default();
        }
        fft_inplace(&mut self.work, &mut self.scratch);
        for (z, &h) in self.work.iter_mut().zip(self.kernel_spec.iter()) {
            *z = *z * h;
        }
        ifft_inplace(&mut self.work, &mut self.scratch);

        let (head, tail_part) = self.work.split_at(block_len);
        for ((o, t), w) in output.iter_mut().zip(self.tail.iter_mut()).zip(head.iter()) {
            *o = w.re + *t;
        }
        for (t, w) in self.tail.iter_mut().zip(tail_part.iter()) {
            *t = w.re;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolve_matches_direct() {
        let input: Vec<f32> = (0..10).map(|i| (i as f32) * 0.5).collect();
        let kernel: Vec<f32> = (0..4).map(|i| 1.0 / (i as f32 + 1.0)).collect();
        let mut out = vec![0.0f32; input.len() + kernel.len() - 1];
        convolve(&input, &kernel, &mut out);

        for n in 0..out.len() {
            let mut acc = 0.0f32;
            for m in 0..kernel.len() {
                if n >= m && n - m < input.len() {
                    acc += input[n - m] * kernel[m];
                }
            }
            assert!((out[n] - acc).abs() < 1e-5, "n={n}");
        }
    }

    #[test]
    fn convolution_with_impulse_returns_input() {
        let input: Vec<f64> = (0..20).map(|i| (i as f64).sin()).collect();
        let mut out = vec![0.0f64; input.len()];
        convolve(&input, &[1.0], &mut out);
        for (a, b) in out.iter().zip(input.iter()) {
            assert!((a - b).abs() < 1e-12);
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn convolve_plan_matches_direct() {
        let kernel: Vec<f32> = (0..6).map(|i| (i as f32 * 0.13).sin()).collect();
        let input: Vec<f32> = (0..16).map(|i| (i as f32 * 0.7).cos()).collect();
        let mut expected = vec![0.0f32; input.len() + kernel.len() - 1];
        convolve(&input, &kernel, &mut expected);

        let mut plan = ConvolvePlan::<f32>::new(&kernel, input.len());
        let mut got = vec![0.0f32; input.len() + kernel.len() - 1];
        plan.convolve(&input, &mut got);
        for (a, b) in got.iter().zip(expected.iter()) {
            assert!((a - b).abs() < 1e-3, "got {a} want {b}");
        }
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn fft_convolver_matches_direct_stream() {
        let kernel: Vec<f32> = (0..5).map(|i| 1.0 / (i as f32 + 1.0)).collect();
        let input: Vec<f32> = (0..48).map(|i| (i as f32 * 0.21).sin()).collect();

        let block_len = 8;
        let mut conv = FftConvolver::<f32>::new(&kernel, block_len);
        let mut out = vec![0.0f32; input.len()];
        for (chunk, o) in input
            .chunks_exact(block_len)
            .zip(out.chunks_exact_mut(block_len))
        {
            conv.process(chunk, o);
        }

        // Direct convolution per-block (zero initial conditions) as reference.
        for n in 0..input.len() {
            let mut acc = 0.0f32;
            for m in 0..kernel.len() {
                if n >= m {
                    acc += input[n - m] * kernel[m];
                }
            }
            assert!(
                (out[n] - acc).abs() < 1e-2,
                "n={n} got {} want {}",
                out[n],
                acc
            );
        }
    }
}
