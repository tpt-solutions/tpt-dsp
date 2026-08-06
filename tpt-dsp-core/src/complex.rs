//! Complex-number helpers for RF / IQ processing.
//!
//! Type aliases and small numerical utilities built on [`num_complex`].
//! All helpers are pure and allocation-free so they are safe to call from
//! real-time threads.

use num_complex::Complex;
use num_traits::Float;

/// π at f64 precision, converted to `F` (correctly rounded for `f32`).
/// Avoids the loss of precision caused by going through `f32` constants.
#[inline]
pub(crate) fn pi<F: Float>() -> F {
    F::from(3.141592653589793238462643383279502884_f64).unwrap()
}

/// 2π at f64 precision, converted to `F`.
#[inline]
pub(crate) fn tau<F: Float>() -> F {
    F::from(6.283185307179586476925286766559005768_f64).unwrap()
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
}
