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

/// Streaming uniformly-partitioned FFT convolver for real-time block
/// processing.
///
/// The impulse response is split into `block_len`-sized partitions, each
/// transformed once at construction. Every block is convolved against all
/// partitions through a ring of recent input spectra, so the output is exact
/// for an impulse response of any length; the cost per block grows with the
/// number of partitions.
///
/// Requires the `alloc` feature. Construct once with the impulse response;
/// each [`process`](Self::process) call is allocation-free.
#[cfg(feature = "alloc")]
pub struct FftConvolver<F: Float> {
    fft_len: usize,
    block_len: usize,
    partitions: usize,
    scratch: alloc::vec::Vec<Complex<F>>,
    work: alloc::vec::Vec<Complex<F>>,
    /// Spectra of the kernel partitions, `partitions * fft_len` values.
    kernel_spec: alloc::vec::Vec<Complex<F>>,
    /// Spectra of the most recent input blocks, one slot per partition.
    input_spec: alloc::vec::Vec<Complex<F>>,
    /// Slot that the next input spectrum is written to.
    head: usize,
    /// Circular-convolution tail carried into the next block.
    tail: alloc::vec::Vec<F>,
}

#[cfg(feature = "alloc")]
impl<F: Float + Default> FftConvolver<F> {
    /// Create a convolver for `block_len`-sized blocks. The FFT size is
    /// `2 * next_power_of_two(block_len)`, and the kernel is split into
    /// `ceil(kernel.len() / block_len)` partitions (at least one).
    ///
    /// # Panics
    ///
    /// Panics if `block_len` is zero.
    pub fn new(kernel: &[F], block_len: usize) -> Self {
        assert!(block_len > 0, "block_len must be nonzero");
        let fft_len = next_power_of_two(block_len * 2).max(1);
        let partitions = kernel.len().div_ceil(block_len).max(1);
        let mut scratch = alloc::vec![Complex::default(); fft_len];
        let mut kernel_spec = alloc::vec![Complex::default(); partitions * fft_len];
        for (spec, chunk) in kernel_spec
            .chunks_mut(fft_len)
            .zip(kernel.chunks(block_len))
        {
            for (z, &x) in spec.iter_mut().zip(chunk.iter()) {
                *z = Complex::new(x, F::zero());
            }
            fft_inplace(spec, &mut scratch);
        }
        Self {
            fft_len,
            block_len,
            partitions,
            scratch,
            work: alloc::vec![Complex::default(); fft_len],
            kernel_spec,
            input_spec: alloc::vec![Complex::default(); partitions * fft_len],
            head: 0,
            tail: alloc::vec![F::zero(); fft_len - block_len],
        }
    }

    /// Block length this convolver was configured for.
    #[inline]
    pub fn block_len(&self) -> usize {
        self.block_len
    }

    /// FFT size used for each partition of the convolution.
    #[inline]
    pub fn fft_len(&self) -> usize {
        self.fft_len
    }

    /// Process one block of `block_len()` samples, producing `block_len()`
    /// output samples (the convolution tail is carried internally).
    /// `output` must have at least `block_len()` elements. Allocation-free.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        assert!(input.len() == self.block_len, "wrong input block size");
        assert!(output.len() >= self.block_len, "output too small");
        let (n, b) = (self.fft_len, self.block_len);

        // Transform the new block into the slot the ring is about to reuse.
        let newest = self.head;
        let spec = &mut self.input_spec[newest * n..(newest + 1) * n];
        for (z, &x) in spec.iter_mut().zip(input.iter()) {
            *z = Complex::new(x, F::zero());
        }
        for z in spec[b..].iter_mut() {
            *z = Complex::default();
        }
        fft_inplace(spec, &mut self.scratch);
        self.head = (newest + 1) % self.partitions;

        // Partition `p` multiplies the input from `p` blocks ago.
        for z in self.work.iter_mut() {
            *z = Complex::default();
        }
        for p in 0..self.partitions {
            let slot = (newest + self.partitions - p) % self.partitions;
            let x = &self.input_spec[slot * n..(slot + 1) * n];
            let h = &self.kernel_spec[p * n..(p + 1) * n];
            for ((acc, &xi), &hi) in self.work.iter_mut().zip(x.iter()).zip(h.iter()) {
                *acc = *acc + xi * hi;
            }
        }
        ifft_inplace(&mut self.work, &mut self.scratch);

        for ((o, &t), w) in output[..b]
            .iter_mut()
            .zip(self.tail.iter())
            .zip(self.work.iter())
        {
            *o = w.re + t;
        }
        for (t, w) in self.tail.iter_mut().zip(self.work[b..].iter()) {
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

    /// Streams `input` through `FftConvolver` in `block_len` blocks and
    /// compares every sample with direct convolution.
    fn max_stream_error(kernel: &[f32], block_len: usize, input: &[f32]) -> f32 {
        let mut conv = FftConvolver::<f32>::new(kernel, block_len);
        let mut got = Vec::new();
        let mut block_out = vec![0.0f32; block_len];
        for block in input.chunks(block_len) {
            conv.process(block, &mut block_out);
            got.extend_from_slice(&block_out);
        }
        let mut want = vec![0.0f32; input.len() + kernel.len() - 1];
        convolve(input, kernel, &mut want);
        got.iter()
            .zip(want.iter())
            .map(|(g, w)| (g - w).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn fft_convolver_is_exact_for_long_kernels() {
        // Kernels longer than one block need several partitions; the old
        // single-block convolver silently truncated them.
        let input: Vec<f32> = (0..2048)
            .map(|i| ((i * 7 + 3) % 17) as f32 / 17.0 - 0.5)
            .collect();
        for &(kernel_len, block_len) in &[
            (129usize, 128usize),
            (200, 128),
            (1024, 128),
            (4096, 256),
            (256, 64),
        ] {
            let kernel: Vec<f32> = (0..kernel_len)
                .map(|i| ((i * 5 + 1) % 13) as f32 / 13.0 - 0.5)
                .collect();
            let err = max_stream_error(&kernel, block_len, &input);
            assert!(
                err < 1e-3,
                "kernel={kernel_len} block={block_len} max_err={err}"
            );
        }
    }

    #[test]
    fn fft_convolver_handles_short_and_empty_kernels() {
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.3).cos()).collect();
        assert!(max_stream_error(&[0.75], 32, &input) < 1e-5);
        assert!(max_stream_error(&[0.5, -0.25, 0.125], 64, &input) < 1e-5);
        // An empty kernel is silence, not a panic.
        let mut conv = FftConvolver::<f32>::new(&[], 16);
        let mut out = [1.0f32; 16];
        conv.process(&input[..16], &mut out);
        assert!(out.iter().all(|&x| x == 0.0));
    }
}
