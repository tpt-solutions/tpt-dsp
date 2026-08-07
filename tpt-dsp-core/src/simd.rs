//! Portable-SIMD accelerated complex arithmetic.
//!
//! This module is compiled only when the nightly-only `simd` feature is
//! enabled; the crate root then turns on the `portable_simd` language feature.
//! When `simd` is off, [`crate::simd`] resolves to a portable scalar fallback
//! with an identical API, so callers never need feature-gates of their own.
//!
//! Real and imaginary parts are de-interleaved into separate `f32x4` lanes,
//! the arithmetic runs four complex numbers at a time, and a scalar tail
//! handles the remaining `len % 4` elements. Operation order matches the
//! scalar fallback exactly, so both paths agree bit-for-bit.
//!
//! Everything here is `core`-only and allocation-free, so it also works under
//! `--no-default-features --features simd`.

#![cfg(feature = "simd")]

use core::simd::f32x4;
use num_complex::Complex;

/// Number of complex numbers processed per vector iteration.
const LANES: usize = 4;

#[inline]
fn load(chunk: &[Complex<f32>]) -> (f32x4, f32x4) {
    (
        f32x4::from_array([chunk[0].re, chunk[1].re, chunk[2].re, chunk[3].re]),
        f32x4::from_array([chunk[0].im, chunk[1].im, chunk[2].im, chunk[3].im]),
    )
}

#[inline]
fn store(re: f32x4, im: f32x4, out: &mut [Complex<f32>]) {
    let re = re.to_array();
    let im = im.to_array();
    for i in 0..LANES {
        out[i] = Complex::new(re[i], im[i]);
    }
}

#[cfg(feature = "std")]
#[inline]
fn sqrt(v: f32x4) -> f32x4 {
    use std::simd::StdFloat;
    v.sqrt()
}

// `StdFloat` (and therefore the vectorised `sqrt`) lives in `std`, not `core`,
// because it may lower to a `math.h` call on targets without hardware support.
// Under `no_std` fall back to the libm-backed scalar root, lane by lane.
#[cfg(not(feature = "std"))]
#[inline]
fn sqrt(v: f32x4) -> f32x4 {
    use num_traits::Float;
    let a = v.to_array();
    f32x4::from_array([a[0].sqrt(), a[1].sqrt(), a[2].sqrt(), a[3].sqrt()])
}

#[inline]
fn scalar_magnitude(z: Complex<f32>) -> f32 {
    #[cfg(not(feature = "std"))]
    use num_traits::Float;
    (z.re * z.re + z.im * z.im).sqrt()
}

/// Element-wise complex multiply: `out[i] = a[i] * b[i]`.
///
/// # Panics
///
/// Panics if the three slices do not all have the same length.
pub fn complex_mul_simd(a: &[Complex<f32>], b: &[Complex<f32>], out: &mut [Complex<f32>]) {
    assert_eq!(a.len(), b.len(), "complex_mul_simd: input length mismatch");
    assert_eq!(
        a.len(),
        out.len(),
        "complex_mul_simd: output length mismatch"
    );

    let n = a.len();
    let body = n - n % LANES;

    let mut i = 0;
    while i < body {
        let (ar, ai) = load(&a[i..i + LANES]);
        let (br, bi) = load(&b[i..i + LANES]);
        store(ar * br - ai * bi, ar * bi + ai * br, &mut out[i..i + LANES]);
        i += LANES;
    }
    for k in body..n {
        out[k] = a[k] * b[k];
    }
}

/// Element-wise complex add: `out[i] = a[i] + b[i]`.
///
/// # Panics
///
/// Panics if the three slices do not all have the same length.
pub fn complex_add_simd(a: &[Complex<f32>], b: &[Complex<f32>], out: &mut [Complex<f32>]) {
    assert_eq!(a.len(), b.len(), "complex_add_simd: input length mismatch");
    assert_eq!(
        a.len(),
        out.len(),
        "complex_add_simd: output length mismatch"
    );

    let n = a.len();
    let body = n - n % LANES;

    let mut i = 0;
    while i < body {
        let (ar, ai) = load(&a[i..i + LANES]);
        let (br, bi) = load(&b[i..i + LANES]);
        store(ar + br, ai + bi, &mut out[i..i + LANES]);
        i += LANES;
    }
    for k in body..n {
        out[k] = a[k] + b[k];
    }
}

/// Element-wise magnitude: `out[i] = |input[i]|`, computed as
/// `sqrt(re² + im²)`.
///
/// Note this is the direct formula rather than the `hypot` used by
/// [`crate::magnitude`], so extreme inputs may overflow where `hypot` would
/// not; for normal audio/IQ ranges the results agree to within `f32` rounding.
///
/// # Panics
///
/// Panics if `input` and `out` have different lengths.
pub fn magnitude_simd(input: &[Complex<f32>], out: &mut [f32]) {
    assert_eq!(
        input.len(),
        out.len(),
        "magnitude_simd: output length mismatch"
    );

    let n = input.len();
    let body = n - n % LANES;

    let mut i = 0;
    while i < body {
        let (re, im) = load(&input[i..i + LANES]);
        out[i..i + LANES].copy_from_slice(&sqrt(re * re + im * im).to_array());
        i += LANES;
    }
    for k in body..n {
        out[k] = scalar_magnitude(input[k]);
    }
}

/// One radix-2 decimation-in-time butterfly stage block.
///
/// Computes, for every `k`, `t = twiddles[k * step] * upper[k]` and then
/// `lower[k] += t`, `upper[k] = lower_old[k] - t`. This is the inner loop of
/// [`crate::fft_inplace_f32`], vectorised four butterflies at a time.
///
/// # Panics
///
/// Panics if `lower` and `upper` have different lengths, or if `twiddles` is
/// too short for the requested `step`.
pub fn fft_butterfly(
    lower: &mut [Complex<f32>],
    upper: &mut [Complex<f32>],
    twiddles: &[Complex<f32>],
    step: usize,
) {
    let half = lower.len();
    assert_eq!(half, upper.len(), "fft_butterfly: half length mismatch");
    assert!(
        half == 0 || (half - 1) * step < twiddles.len(),
        "fft_butterfly: twiddle table too small"
    );

    let body = half - half % LANES;

    let mut k = 0;
    while k < body {
        let (w0, w1, w2, w3) = (
            twiddles[k * step],
            twiddles[(k + 1) * step],
            twiddles[(k + 2) * step],
            twiddles[(k + 3) * step],
        );
        let wr = f32x4::from_array([w0.re, w1.re, w2.re, w3.re]);
        let wi = f32x4::from_array([w0.im, w1.im, w2.im, w3.im]);

        let (ur, ui) = load(&upper[k..k + LANES]);
        let (lr, li) = load(&lower[k..k + LANES]);

        let tr = wr * ur - wi * ui;
        let ti = wr * ui + wi * ur;

        store(lr + tr, li + ti, &mut lower[k..k + LANES]);
        store(lr - tr, li - ti, &mut upper[k..k + LANES]);
        k += LANES;
    }
    for k in body..half {
        let t = twiddles[k * step] * upper[k];
        let a = lower[k];
        lower[k] = a + t;
        upper[k] = a - t;
    }
}

/// Feature-dispatching alias for [`complex_mul_simd`].
///
/// Resolves to the vectorised implementation here and to the scalar fallback
/// when the `simd` feature is off, so downstream code can always call
/// `simd::mul`.
pub use self::complex_mul_simd as mul;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::C32;

    const N: usize = 10;

    fn sample(seed: f32) -> [C32; N] {
        core::array::from_fn(|i| {
            let x = i as f32 + seed;
            C32::new((x * 0.37).sin() * 2.0, (x * 0.11).cos() - 0.25)
        })
    }

    #[test]
    fn simd_mul_matches_scalar() {
        let a = sample(0.0);
        let b = sample(3.5);
        let mut got = [C32::default(); N];
        complex_mul_simd(&a, &b, &mut got);

        for i in 0..N {
            let want = a[i] * b[i];
            assert!((got[i].re - want.re).abs() <= f32::EPSILON * 8.0);
            assert!((got[i].im - want.im).abs() <= f32::EPSILON * 8.0);
        }
    }

    #[test]
    fn simd_add_matches_scalar() {
        let a = sample(1.0);
        let b = sample(7.25);
        let mut got = [C32::default(); N];
        complex_add_simd(&a, &b, &mut got);

        for i in 0..N {
            let want = a[i] + b[i];
            assert_eq!(got[i], want);
        }
    }

    #[test]
    fn simd_magnitude_matches_scalar() {
        let a = sample(2.0);
        let mut got = [0.0f32; N];
        magnitude_simd(&a, &mut got);

        for i in 0..N {
            let want = crate::magnitude(a[i]);
            assert!(
                (got[i] - want).abs() <= 1e-6 * (1.0 + want),
                "lane {i}: {} vs {want}",
                got[i]
            );
        }
    }

    #[test]
    fn simd_tail_is_handled() {
        for n in 0..=9usize {
            let a = sample(0.5);
            let b = sample(4.5);
            let mut got = [C32::new(f32::NAN, f32::NAN); N];
            complex_mul_simd(&a[..n], &b[..n], &mut got[..n]);
            for i in 0..n {
                let want = a[i] * b[i];
                assert!((got[i].re - want.re).abs() <= f32::EPSILON * 8.0);
                assert!((got[i].im - want.im).abs() <= f32::EPSILON * 8.0);
            }
            for slot in got.iter().skip(n) {
                assert!(slot.re.is_nan(), "wrote past the requested length");
            }
        }
    }

    #[test]
    fn mul_alias_dispatches() {
        let a = sample(0.0);
        let b = sample(1.0);
        let mut via_alias = [C32::default(); N];
        let mut via_fn = [C32::default(); N];
        mul(&a, &b, &mut via_alias);
        complex_mul_simd(&a, &b, &mut via_fn);
        assert_eq!(via_alias, via_fn);
    }

    #[test]
    fn simd_fft_matches_scalar_fft() {
        use crate::{fft_inplace, fft_inplace_f32};

        for n in [4usize, 8, 16, 64, 256] {
            let input: Vec<C32> = (0..n)
                .map(|i| C32::new((i as f32 * 0.1).sin(), (i as f32 * 0.05).cos()))
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
}
