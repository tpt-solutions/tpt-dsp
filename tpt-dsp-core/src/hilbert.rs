//! Hilbert transform for analytic-signal / envelope extraction.
//!
//! The transform is implemented in the frequency domain: FFT → zero the
//! DC and Nyquist bins and halve the positive-frequency coefficients →
//! inverse FFT. The output `y[n]` is the 90°-shifted version of `x[n]` so
//! that `x[n] + i·y[n]` is the analytic signal.
//!
//! The free [`hilbert`] function is allocation-free and operates on
//! caller-provided buffers. The [`HilbertTransformer`] convenience wrapper
//! (requires the `alloc` feature) owns its buffers.

use num_complex::Complex;
use num_traits::Float;

use crate::fft::{fft_inplace, ifft_inplace};

/// Compute the Hilbert transform of `input` into `out`.
///
/// Lengths must all be equal and a power of two. `work` and `scratch` must
/// each hold at least `input.len()` complex entries.
///
/// # Panics
///
/// Panics if the length is not a power of two or a buffer is too small.
pub fn hilbert<F: Float>(
    input: &[F],
    out: &mut [F],
    work: &mut [Complex<F>],
    scratch: &mut [Complex<F>],
) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for Hilbert transform");
    assert!(work.len() >= n, "work buffer too small for Hilbert transform");
    assert!(
        scratch.len() >= n,
        "scratch buffer too small for Hilbert transform"
    );
    assert!(
        n.is_power_of_two(),
        "Hilbert transform requires a power-of-two length"
    );

    for (z, x) in work[..n].iter_mut().zip(input.iter()) {
        *z = Complex::new(*x, F::zero());
    }

    fft_inplace(&mut work[..n], scratch);

    let mut mid = n / 2;
    if n % 2 == 0 {
        mid -= 1;
    }
    work[0] = Complex::new(F::zero(), F::zero());
    if n % 2 == 0 {
        work[mid + 1] = Complex::new(F::zero(), F::zero());
    }
    for k in 1..=mid {
        let z = work[k];
        work[k] = Complex::new(z.re + z.re, z.im + z.im);
    }
    for k in (mid + 2 + (n % 2))..n {
        work[k] = Complex::new(F::zero(), F::zero());
    }

    ifft_inplace(&mut work[..n], scratch);
    for (o, z) in out.iter_mut().zip(work[..n].iter()) {
        *o = z.im;
    }
}

/// Buffer-owning convenience wrapper around [`hilbert`].
///
/// Requires the `alloc` feature. Construct once (allocates), then call
/// [`process`](Self::process) per block — no further allocation occurs.
#[cfg(feature = "alloc")]
pub struct HilbertTransformer<F: Float> {
    len: usize,
    work: alloc::vec::Vec<Complex<F>>,
    scratch: alloc::vec::Vec<Complex<F>>,
}

#[cfg(feature = "alloc")]
impl<F: Float + Default> HilbertTransformer<F> {
    /// Create a transformer for blocks of `len` samples (power of two).
    ///
    /// # Panics
    ///
    /// Panics if `len` is not a power of two.
    pub fn new(len: usize) -> Self {
        assert!(
            len.is_power_of_two(),
            "HilbertTransformer length must be a power of two"
        );
        Self {
            len,
            work: alloc::vec![Complex::default(); len],
            scratch: alloc::vec![Complex::default(); len],
        }
    }

    /// Block length this transformer was configured for.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if the configured length is zero.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Process one block; `input` and `output` must have `self.len()`
    /// elements.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        hilbert(input, output, &mut self.work, &mut self.scratch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::C32;

    #[test]
    fn hilbert_shifts_sine_to_cosine() {
        let n = 128;
        let input: Vec<f32> = (0..n).map(|i| (i as f32 * 0.2).sin()).collect();
        let mut out = vec![0.0f32; n];
        let mut work = vec![C32::default(); n];
        let mut scratch = vec![C32::default(); n];
        hilbert(&input, &mut out, &mut work, &mut scratch);

        // Hilbert of sin is -cos. Ignore the first/last samples (transient
        // ringing at the edges of a finite transform).
        for i in 10..n - 10 {
            let expected = -(i as f32 * 0.2).cos();
            assert!((out[i] - expected).abs() < 0.2, "i={i} got {} want {}", out[i], expected);
        }
    }

    #[test]
    fn analytic_signal_envelope_is_flat() {
        let n = 256;
        let freq = 0.05f32;
        let input: Vec<f32> = (0..n).map(|i| (i as f32 * freq).sin()).collect();
        let mut q = vec![0.0f32; n];
        let mut work = vec![C32::default(); n];
        let mut scratch = vec![C32::default(); n];
        hilbert(&input, &mut q, &mut work, &mut scratch);

        // Envelope = magnitude of analytic signal, should be ~1 for a pure sine.
        let mid = n / 2;
        let env = (input[mid] * input[mid] + q[mid] * q[mid]).sqrt();
        assert!((env - 1.0).abs() < 0.1, "envelope {env}");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn transformer_wrapper_roundtrip() {
        let n = 64;
        let mut t = HilbertTransformer::<f64>::new(n);
        assert_eq!(t.len(), n);
        let input: Vec<f64> = (0..n).map(|i| (i as f64 * 0.3).sin()).collect();
        let mut out = vec![0.0f64; n];
        t.process(&input, &mut out);
        for i in 8..n - 8 {
            let expected = -(i as f64 * 0.3).cos();
            assert!((out[i] - expected).abs() < 0.25);
        }
    }
}
