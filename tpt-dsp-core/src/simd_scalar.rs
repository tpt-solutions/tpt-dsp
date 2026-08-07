//! Portable scalar fallback for [`crate::simd`].
//!
//! Compiled whenever the nightly-only `simd` feature is *off*, which is the
//! default. It mirrors the vectorised API one-for-one — same names, same
//! signatures, same operation order — so downstream code can call
//! `crate::simd::*` unconditionally and stable builds keep working. (It is also
//! selected on a stable toolchain when the `simd` feature is enabled, because
//! `core::simd` is unavailable there.)

use num_complex::Complex;

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

    for (o, (x, y)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *o = *x * *y;
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

    for (o, (x, y)) in out.iter_mut().zip(a.iter().zip(b.iter())) {
        *o = *x + *y;
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

    for (o, z) in out.iter_mut().zip(input.iter()) {
        *o = scalar_magnitude(*z);
    }
}

/// One radix-2 decimation-in-time butterfly stage block.
///
/// Computes, for every `k`, `t = twiddles[k * step] * upper[k]` and then
/// `lower[k] += t`, `upper[k] = lower_old[k] - t`. This is the inner loop of
/// [`crate::fft_inplace_f32`].
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

    for k in 0..half {
        let t = twiddles[k * step] * upper[k];
        let a = lower[k];
        lower[k] = a + t;
        upper[k] = a - t;
    }
}

/// Feature-dispatching alias for [`complex_mul_simd`].
///
/// Resolves to the scalar implementation here and to the vectorised one when
/// the `simd` feature is on, so downstream code can always call `simd::mul`.
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
    fn scalar_fallback_mul_and_add() {
        let a = sample(0.0);
        let b = sample(3.5);
        let mut prod = [C32::default(); N];
        let mut sum = [C32::default(); N];
        complex_mul_simd(&a, &b, &mut prod);
        complex_add_simd(&a, &b, &mut sum);

        for i in 0..N {
            assert_eq!(prod[i], a[i] * b[i]);
            assert_eq!(sum[i], a[i] + b[i]);
        }
    }

    #[test]
    fn scalar_fallback_magnitude() {
        let a = sample(2.0);
        let mut got = [0.0f32; N];
        magnitude_simd(&a, &mut got);

        for i in 0..N {
            let want = crate::magnitude(a[i]);
            assert!((got[i] - want).abs() <= 1e-6 * (1.0 + want));
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
}
