//! Fast Fourier Transform — a self-contained radix-2 implementation.
//!
//! The classic iterative Cooley–Tukey decimation-in-time algorithm with a
//! precomputed twiddle table. It runs entirely on caller-provided buffers
//! (`no_std` / zero-allocation friendly) and is generic over `f32`/`f64`.
//!
//! For arbitrary-length or hardware-accelerated transforms enable the
//! `std` feature and use [`crate::FftPlan`], which wraps the RustFFT
//! library.
//!
//! # Algorithm
//!
//! 1. Bit-reversal permutation.
//! 2. `log2(n)` butterfly stages; each stage combines pairs of the previous
//!    half-size transforms. Twiddle factors are computed once into a scratch
//!    buffer and reused across all blocks of a stage.

use num_complex::Complex;
use num_traits::Float;

use crate::complex::tau;

/// Returns `true` if `n` is a positive power of two.
#[inline]
pub fn is_power_of_two(n: usize) -> bool {
    n.is_power_of_two()
}

/// Round `n` up to the next power of two (returns `1` for `n == 0`).
#[inline]
pub fn next_power_of_two(n: usize) -> usize {
    if n == 0 {
        1
    } else {
        n.next_power_of_two()
    }
}

/// Fill `scratch[0..len]` with the radix-2 twiddle factors for a transform
/// of length `len`. `scratch` must hold at least `len` entries.
///
/// This is exposed so a caller can precompute twiddles once and reuse them
/// across many transforms; [`fft_inplace`] also fills it for you.
pub fn twiddles<F: Float>(len: usize, scratch: &mut [Complex<F>]) {
    assert!(
        len <= scratch.len(),
        "twiddle scratch too small: need {len}"
    );
    assert!(
        is_power_of_two(len),
        "radix-2 FFT requires a power-of-two length, got {len}"
    );
    let tau = tau::<F>();
    for (i, slot) in scratch[..len].iter_mut().enumerate() {
        let a = tau * F::from(i).unwrap() / F::from(len).unwrap();
        *slot = Complex::new(a.cos(), -a.sin());
    }
}

/// In-place forward DFT of `buf` using `scratch` as temporary storage.
///
/// `buf.len()` must be a power of two and `scratch.len() >= buf.len()`.
/// On return `buf` holds `X[k] = Σ x[n]·e^(-2πi·nk/N)`.
pub fn fft_inplace<F: Float>(buf: &mut [Complex<F>], scratch: &mut [Complex<F>]) {
    let n = buf.len();
    assert!(is_power_of_two(n), "FFT length must be a power of two");
    assert!(scratch.len() >= n, "FFT scratch too small");

    twiddles(n, scratch);
    bit_reverse(buf);

    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let step = n / size;
        for start in (0..n).step_by(size) {
            for k in 0..half {
                let t = scratch[k * step] * buf[start + half + k];
                let (a, b) = (buf[start + k], t);
                buf[start + k] = a + b;
                buf[start + half + k] = a - b;
            }
        }
        size *= 2;
    }
}

/// In-place forward DFT specialised for `Complex<f32>`.
///
/// Numerically equivalent to [`fft_inplace::<f32>`](fft_inplace), but the
/// butterfly inner loop is delegated to [`crate::simd::fft_butterfly`], which
/// is vectorised with `core::simd` when the nightly-only `simd` feature is
/// enabled and plain scalar code otherwise.
pub fn fft_inplace_f32(buf: &mut [Complex<f32>], scratch: &mut [Complex<f32>]) {
    let n = buf.len();
    assert!(is_power_of_two(n), "FFT length must be a power of two");
    assert!(scratch.len() >= n, "FFT scratch too small");

    twiddles(n, scratch);
    bit_reverse(buf);

    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let step = n / size;
        for start in (0..n).step_by(size) {
            let (lower, upper) = buf[start..start + size].split_at_mut(half);
            crate::simd::fft_butterfly(lower, upper, &scratch[..n], step);
        }
        size *= 2;
    }
}

/// Inverse in-place DFT of `buf` (`x[n] = (1/N)·Σ X[k]·e^(+2πi·nk/N)`).
pub fn ifft_inplace<F: Float>(buf: &mut [Complex<F>], scratch: &mut [Complex<F>]) {
    let n = buf.len();
    conjugate(buf);
    fft_inplace(buf, scratch);
    conjugate(buf);
    let inv = F::from(n).unwrap().recip();
    for z in buf.iter_mut() {
        *z = *z * inv;
    }
}

/// Out-of-place forward transform: writes the DFT of `input` into `out`.
/// `out.len() == input.len()`, power of two; `scratch.len() >= len`.
pub fn fft<F: Float>(input: &[Complex<F>], out: &mut [Complex<F>], scratch: &mut [Complex<F>]) {
    assert_eq!(input.len(), out.len(), "input/output length mismatch");
    out.copy_from_slice(input);
    fft_inplace(out, scratch);
}

/// Out-of-place inverse transform.
pub fn ifft<F: Float>(input: &[Complex<F>], out: &mut [Complex<F>], scratch: &mut [Complex<F>]) {
    assert_eq!(input.len(), out.len(), "input/output length mismatch");
    out.copy_from_slice(input);
    ifft_inplace(out, scratch);
}

fn bit_reverse<F: Float>(buf: &mut [Complex<F>]) {
    let n = buf.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            buf.swap(i, j);
        }
    }
}

fn conjugate<F: Float>(buf: &mut [Complex<F>]) {
    for z in buf.iter_mut() {
        z.im = -z.im;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complex::tau;
    use crate::{Complex64, C32};

    fn naive_dft<F: Float>(input: &[Complex<F>]) -> Vec<Complex<F>> {
        let n = input.len();
        let tau = tau::<F>();
        (0..n)
            .map(|k| {
                let mut acc = Complex::new(F::zero(), F::zero());
                for (m, x) in input.iter().enumerate() {
                    let a = tau * F::from(k * m).unwrap() / F::from(n).unwrap();
                    let w = Complex::new(a.cos(), -a.sin());
                    acc = acc + *x * w;
                }
                acc
            })
            .collect()
    }

    fn roundtrip(real: &[f32]) {
        let n = real.len();
        let input: Vec<C32> = real.iter().map(|x| C32::new(*x, 0.0)).collect();
        let mut work = input.clone();
        let mut scratch = vec![C32::default(); n];
        let mut inv = vec![C32::default(); n];

        fft_inplace(&mut work, &mut scratch);
        ifft(&work, &mut inv, &mut scratch);

        for (a, b) in inv.iter().zip(real.iter()) {
            assert!((a.re - b).abs() < 1e-4, "roundtrip re {a} vs {b}");
            assert!(a.im.abs() < 1e-4);
        }
    }

    #[test]
    fn fft_matches_naive() {
        let n = 32;
        let input: Vec<C32> = (0..n)
            .map(|i| C32::new((i as f32 * 0.1).sin(), (i as f32 * 0.05).cos()))
            .collect();
        let expected = naive_dft(&input);

        let mut work = input.clone();
        let mut scratch = vec![C32::default(); n];
        fft_inplace(&mut work, &mut scratch);

        for (got, want) in work.iter().zip(expected.iter()) {
            assert!((got.re - want.re).abs() < 1e-3);
            assert!((got.im - want.im).abs() < 1e-3);
        }
    }

    #[test]
    fn f64_matches_naive() {
        let n = 16;
        let input: Vec<Complex64> = (0..n)
            .map(|i| Complex64::new((i as f64 * 0.3).sin(), (i as f64 * 0.2).cos()))
            .collect();
        let expected = naive_dft(&input);

        let mut work = input.clone();
        let mut scratch = vec![Complex64::default(); n];
        fft_inplace(&mut work, &mut scratch);

        let mut max_err = 0.0f64;
        for (got, want) in work.iter().zip(expected.iter()) {
            max_err = max_err.max((got.re - want.re).abs());
            assert!((got.re - want.re).abs() < 1e-10, "re err {max_err}");
            assert!((got.im - want.im).abs() < 1e-10, "im");
        }
    }

    #[test]
    fn roundtrip_various_lengths() {
        for n in [2usize, 4, 8, 16, 32, 64, 128, 256] {
            let real: Vec<f32> = (0..n).map(|i| (i as f32).sin() * 0.5).collect();
            roundtrip(&real);
        }
    }

    #[test]
    fn impulse_is_flat_spectrum() {
        let n = 16;
        let mut input = vec![C32::default(); n];
        input[0] = C32::new(1.0, 0.0);
        let mut scratch = vec![C32::default(); n];
        fft_inplace(&mut input, &mut scratch);
        for z in input.iter() {
            assert!((z.re - 1.0).abs() < 1e-6);
            assert!(z.im.abs() < 1e-6);
        }
    }

    #[test]
    fn f32_specialised_matches_generic() {
        for n in [1usize, 2, 4, 8, 16, 64, 256, 1024] {
            let input: Vec<C32> = (0..n)
                .map(|i| C32::new((i as f32 * 0.17).sin(), (i as f32 * 0.09).cos()))
                .collect();

            let mut want = input.clone();
            let mut scratch = vec![C32::default(); n];
            fft_inplace(&mut want, &mut scratch);

            let mut got = input.clone();
            fft_inplace_f32(&mut got, &mut scratch);

            for (g, w) in got.iter().zip(want.iter()) {
                assert!((g.re - w.re).abs() < 1e-4, "n={n}: {g} vs {w}");
                assert!((g.im - w.im).abs() < 1e-4, "n={n}: {g} vs {w}");
            }
        }
    }

    #[test]
    fn power_of_two_helpers() {
        assert!(is_power_of_two(1) && is_power_of_two(1024));
        assert!(!is_power_of_two(0) && !is_power_of_two(6));
        assert_eq!(next_power_of_two(5), 8);
        assert_eq!(next_power_of_two(0), 1);
        assert_eq!(next_power_of_two(1024), 1024);
    }
}
