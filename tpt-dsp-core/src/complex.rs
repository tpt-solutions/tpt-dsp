//! Complex-number helpers for RF / IQ processing.
//!
//! Type aliases and small numerical utilities built on [`num_complex`].
//! All helpers are pure and allocation-free so they are safe to call from
//! real-time threads.

use num_complex::Complex;
use num_traits::Float;

/// π at f64 precision, converted to `F`.
#[inline]
pub(crate) fn pi<F: Float>() -> F {
    F::from(core::f64::consts::PI).unwrap()
}

/// 2π at f64 precision, converted to `F`.
#[inline]
pub(crate) fn tau<F: Float>() -> F {
    F::from(core::f64::consts::TAU).unwrap()
}

/// `cos(π/n · x · y)` for possibly-half-integer `x`, `y`, computed via
/// integer-domain angle reduction first.
///
/// The DCT/MDCT family all evaluate `cos` at arguments of the form
/// `(π/n)·x·y` where `x`,`y` are sample/bin indices (each either an integer
/// or a half-integer). Evaluating that product directly for realistic codec
/// block sizes (`n` in the thousands) produces raw angles in the thousands
/// of radians — poorly conditioned for a direct `f32`/`f64` `cos()`, since
/// representing a large angle consumes most of the type's mantissa bits
/// before the periodic (mod `2π`) part that actually determines the result.
/// `x`/`y` are each an integer or a half-integer, so `2x`/`2y` (`two_x`,
/// `two_y`) are always exact integers; their product `P = (2x)(2y)` is
/// therefore also exact, and `θ = (π/(4n))·P` reduces to the same value
/// every time `P` changes by `8n` (since `(π/(4n))·8n = 2π`) — so reducing
/// `P` modulo `8n` in the integer domain, before ever forming a float,
/// keeps the angle handed to `cos()` bounded to `[0, 2π)` regardless of how
/// large the original indices were.
#[inline]
pub(crate) fn cos_pi_over_n<F: Float>(n: usize, two_x: i64, two_y: i64) -> F {
    let period = (8 * n) as i64;
    let reduced = (two_x * two_y).rem_euclid(period);
    let scale = pi::<F>() / F::from(4 * n).unwrap();
    (scale * F::from(reduced).unwrap()).cos()
}

/// `f32` complex type.
pub type Complex32 = Complex<f32>;
/// `f64` complex type.
pub type Complex64 = Complex<f64>;

/// Convenience alias for [`Complex32`].
pub type C32 = Complex32;
/// Convenience alias for [`Complex64`].
pub type C64 = Complex64;

/// Squared magnitude of a complex number — cheaper than [`Complex::norm`]
/// because it avoids the square root.
///
/// For `f32` inputs this uses `mul_add` under the hood on most targets.
#[inline]
pub fn magnitude_squared<F: Float>(z: Complex<F>) -> F {
    z.re * z.re + z.im * z.im
}

/// Magnitude (absolute value) of a complex number.
#[inline]
pub fn magnitude<F: Float>(z: Complex<F>) -> F {
    z.norm()
}

/// Phase angle of a complex number in radians.
#[inline]
pub fn phase<F: Float>(z: Complex<F>) -> F {
    z.arg()
}

/// Complex exponential `e^(i·theta)` for real `theta`.
///
/// This is frequently needed in IQ mixing and oscillator code; computing it
/// directly is clearer and no slower than reusing `Complex::new`.
#[inline]
pub fn exp_i<F: Float>(theta: F) -> Complex<F> {
    Complex::new(theta.cos(), theta.sin())
}

/// Multiply `z` by `e^(i·theta)` — a phase rotation. Cheaper than a general
/// complex multiply followed by forming the exponential separately.
#[inline]
pub fn rotate<F: Float>(z: Complex<F>, theta: F) -> Complex<F> {
    let c = theta.cos();
    let s = theta.sin();
    Complex::new(z.re * c - z.im * s, z.re * s + z.im * c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_i_has_unit_magnitude() {
        let z = exp_i(1.234f32);
        assert!((z.norm() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rotate_is_correct() {
        let z = C32::new(1.0, 0.0);
        let r = rotate(z, std::f32::consts::FRAC_PI_2);
        assert!((r.re - 0.0).abs() < 1e-6);
        assert!((r.im - 1.0).abs() < 1e-6);
    }

    #[test]
    fn magnitude_squared_matches() {
        let z = C32::new(3.0, 4.0);
        assert!((magnitude_squared(z) - 25.0).abs() < 1e-6);
        assert!((magnitude(z) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn cos_pi_over_n_matches_direct_for_small_angles() {
        // Where the raw (unreduced) angle is small, the reduced and direct
        // evaluations must agree.
        let n = 16usize;
        for x in 0..8i64 {
            for y in 0..8i64 {
                let direct =
                    (core::f64::consts::PI / n as f64 * (x as f64 / 2.0) * (y as f64 / 2.0)).cos();
                let reduced: f64 = cos_pi_over_n(n, x, y);
                assert!(
                    (direct - reduced).abs() < 1e-9,
                    "x={x} y={y}: direct={direct} reduced={reduced}"
                );
            }
        }
    }

    #[test]
    fn cos_pi_over_n_stays_accurate_for_large_indices() {
        // This is the case that motivated the helper: raw indices large
        // enough that a direct `(pi/n * x * y).cos()` would be evaluated at
        // an angle of thousands of radians. Compare against an f64 direct
        // evaluation (itself precise here since f64 has ~15-16 significant
        // digits, comfortably enough for this magnitude) as ground truth.
        let n = 1024usize;
        let two_x = 2 * 2047 + 1 + n as i64; // mdct's `2m+1+n` at m = 2n-1
        let two_y = 2 * 571 + 1; // mdct's `2k+1` at k = 571
        let direct =
            (core::f64::consts::PI / n as f64 * (two_x as f64 / 2.0) * (two_y as f64 / 2.0)).cos();
        let reduced: f64 = cos_pi_over_n(n, two_x, two_y);
        assert!(
            (direct - reduced).abs() < 1e-9,
            "direct={direct} reduced={reduced}"
        );
    }
}
