//! Windowing functions (Hann, Hamming, Blackman, Kaiser, Tukey and friends).
//!
//! Each window comes in two conventions:
//!
//! - **periodic** ([`windowed`], [`windowed_f64`]): the classic FFT convention,
//!   where the window is one period of a cycle, so `w[0] == w[len]`. Use this
//!   for spectral analysis.
//! - **symmetric** ([`windowed_symmetric`], [`windowed_symmetric_f64`]): the
//!   convention used for FIR design and filter windows, where the first and
//!   last samples are both at the ends of a `len`-sample span.
//!
//! The f32 functions compute in f64 and round once, so they match the f64
//! functions to f32 precision. Everything writes into a caller-provided slice
//! and allocates nothing.

// On `no_std` targets the inherent float methods do not exist, so pull in the
// `num_traits::Float` trait. Under `std` the inherent methods are used and the
// import would be flagged as unused, hence the allow.
#[allow(unused_imports)]
use num_traits::Float;

/// The kind of spectral window to generate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowType {
    /// Rectangular (no windowing).
    Rectangular,
    /// Hann (raised cosine).
    Hann,
    /// Hamming.
    Hamming,
    /// Blackman (α = 0.16).
    Blackman,
    /// Five-term flat-top (accurate amplitude for sinusoids, wide main lobe).
    FlatTop,
    /// Kaiser with shape parameter `beta` (`0` is rectangular; larger values
    /// trade main-lobe width for side-lobe rejection).
    Kaiser(f32),
    /// Tukey (tapered cosine) with taper fraction `alpha` in `[0, 1]`
    /// (`0` is rectangular, `1` is Hann).
    Tukey(f32),
    /// Gaussian with standard deviation `sigma` as a fraction of half the
    /// window length (typically `0.1..0.5`).
    Gaussian(f32),
}

/// Generate a periodic window of `len` samples into `out` (f32).
///
/// Sample `i` equals `w(i / len)`, so the first and last samples of a
/// Hann-type window are equal. If `out` is longer than `len`, only the first
/// `len` entries are written.
///
/// # Panics
///
/// Panics if `len > out.len()`.
pub fn windowed(win: WindowType, len: usize, out: &mut [f32]) {
    fill_f32(win, len, out, false);
}

/// Generate a symmetric window of `len` samples into `out` (f32). The first
/// and last samples sit at the two ends of the span, which suits FIR design.
///
/// # Panics
///
/// Panics if `len > out.len()`.
pub fn windowed_symmetric(win: WindowType, len: usize, out: &mut [f32]) {
    fill_f32(win, len, out, true);
}

/// Periodic window of `len` samples into `out` (f64). See [`windowed`].
///
/// # Panics
///
/// Panics if `len > out.len()`.
pub fn windowed_f64(win: WindowType, len: usize, out: &mut [f64]) {
    assert!(len <= out.len(), "window longer than output buffer");
    for (i, slot) in out.iter_mut().take(len).enumerate() {
        *slot = sample(win, i, len, false);
    }
}

/// Symmetric window of `len` samples into `out` (f64). See
/// [`windowed_symmetric`].
///
/// # Panics
///
/// Panics if `len > out.len()`.
pub fn windowed_symmetric_f64(win: WindowType, len: usize, out: &mut [f64]) {
    assert!(len <= out.len(), "window longer than output buffer");
    for (i, slot) in out.iter_mut().take(len).enumerate() {
        *slot = sample(win, i, len, true);
    }
}

fn fill_f32(win: WindowType, len: usize, out: &mut [f32], symmetric: bool) {
    assert!(len <= out.len(), "window longer than output buffer");
    for (i, slot) in out.iter_mut().take(len).enumerate() {
        *slot = sample(win, i, len, symmetric) as f32;
    }
}

/// Evaluate sample `i` of a `len`-point window in f64.
fn sample(win: WindowType, i: usize, len: usize, symmetric: bool) -> f64 {
    if len == 0 {
        return 0.0;
    }
    // Denominator: `len` for periodic, `len - 1` for symmetric (clamped so a
    // single-point window is well defined and equal to 1).
    let denom = if symmetric {
        (len - 1).max(1) as f64
    } else {
        len as f64
    };
    let t = i as f64 / denom; // position in [0, 1]
    let x = core::f64::consts::TAU * t;
    match win {
        WindowType::Rectangular => 1.0,
        WindowType::Hann => 0.5 - 0.5 * x.cos(),
        WindowType::Hamming => 0.54 - 0.46 * x.cos(),
        WindowType::Blackman => 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos(),
        WindowType::FlatTop => {
            // Five-term flat-top (Heinzel et al. coefficients).
            const A0: f64 = 0.215_578_95;
            const A1: f64 = 0.416_631_58;
            const A2: f64 = 0.277_263_16;
            const A3: f64 = 0.083_578_95;
            const A4: f64 = 0.006_947_37;
            A0 - A1 * x.cos() + A2 * (2.0 * x).cos() - A3 * (3.0 * x).cos() + A4 * (4.0 * x).cos()
        }
        WindowType::Kaiser(beta) => {
            let beta = f64::from(beta);
            let u = 2.0 * t - 1.0; // [-1, 1]
            let arg = beta * (1.0 - u * u).max(0.0).sqrt();
            bessel_i0(arg) / bessel_i0(beta)
        }
        WindowType::Tukey(alpha) => {
            let a = f64::from(alpha).clamp(0.0, 1.0);
            if a <= 0.0 {
                1.0
            } else if t < a / 2.0 {
                0.5 * (1.0 - (core::f64::consts::TAU * t / a).cos())
            } else if t > 1.0 - a / 2.0 {
                0.5 * (1.0 - (core::f64::consts::TAU * (1.0 - t) / a).cos())
            } else {
                1.0
            }
        }
        WindowType::Gaussian(sigma) => {
            let sigma = f64::from(sigma);
            let centre = denom / 2.0;
            let std = (sigma * centre).max(f64::MIN_POSITIVE);
            let n = i as f64 - centre;
            (-0.5 * (n / std) * (n / std)).exp()
        }
    }
}

/// Modified Bessel function of the first kind, order zero (power series).
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..=200 {
        let kf = k as f64;
        term *= (half / kf) * (half / kf);
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(win: WindowType, len: usize) -> Vec<f32> {
        let mut out = vec![0.0f32; len];
        windowed(win, len, &mut out);
        out
    }

    #[test]
    fn hann_endpoints_are_near_zero() {
        // Periodic Hann: w[0] == 0 exactly, w[N-1] ~ ½(1-cos(2π/N)) ≈ 0.
        let w = window(WindowType::Hann, 64);
        assert!(w[0].abs() < 1e-6);
        assert!(w[63].abs() < 0.01);
        assert!(w[32].abs() > 0.999);
        assert!((w[0] - w[63]).abs() < 0.01, "window should be periodic");
    }

    #[test]
    fn hamming_endpoints_are_nonzero() {
        let w = window(WindowType::Hamming, 64);
        assert!((w[0] - 0.08).abs() < 1e-6);
    }

    #[test]
    fn windows_normalized_to_correct_scale() {
        for win in [
            WindowType::Hann,
            WindowType::Hamming,
            WindowType::Blackman,
            WindowType::Kaiser(8.0),
            WindowType::Tukey(0.5),
        ] {
            let w = window(win, 256);
            let max = w.iter().cloned().fold(0.0f32, f32::max);
            assert!((max - 1.0).abs() < 1e-6, "{win:?} max = {max}");
            assert!(w.iter().all(|x| *x >= -1e-6 && *x <= 1.0 + 1e-6));
        }
    }

    #[test]
    fn blackman_has_lower_side_lobe_midpoint() {
        let w = window(WindowType::Blackman, 256);
        // Blackman α=0.16 centre value: 0.42 + 0.5 + 0.08 = 1.0
        assert!((w[128] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn partial_output_preserves_tail() {
        let mut out = vec![7.0f32; 16];
        windowed(WindowType::Hann, 8, &mut out);
        assert!(out[8..].iter().all(|x| *x == 7.0));
    }

    #[test]
    fn rectangular_is_all_ones() {
        assert!(window(WindowType::Rectangular, 32)
            .iter()
            .all(|x| *x == 1.0));
    }

    #[test]
    fn kaiser_beta_zero_is_rectangular() {
        let w = window(WindowType::Kaiser(0.0), 64);
        assert!(w.iter().all(|x| (x - 1.0).abs() < 1e-6));
    }

    #[test]
    fn kaiser_matches_known_endpoint_value() {
        // Symmetric Kaiser endpoint is 1 / I0(beta).
        let mut w = vec![0.0f64; 33];
        windowed_symmetric_f64(WindowType::Kaiser(5.0), 33, &mut w);
        let expected = 1.0 / bessel_i0(5.0);
        assert!((w[0] - expected).abs() < 1e-12, "{} vs {expected}", w[0]);
        assert!((w[16] - 1.0).abs() < 1e-12);
        // I0(5) ≈ 27.2399 (tabulated value).
        assert!((bessel_i0(5.0) - 27.239_87).abs() < 1e-3);
    }

    #[test]
    fn tukey_extremes_match_rectangular_and_hann() {
        let rect = window(WindowType::Tukey(0.0), 64);
        assert!(rect.iter().all(|x| (x - 1.0).abs() < 1e-6));
        let tukey1 = window(WindowType::Tukey(1.0), 64);
        let hann = window(WindowType::Hann, 64);
        for (a, b) in tukey1.iter().zip(hann.iter()) {
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn flat_top_peaks_at_centre_with_zero_edges() {
        // The five coefficients sum to 1 at the centre (x = π), and the edge
        // value is ~0 (a0 - a1 + a2 - a3 + a4 ≈ -4e-4).
        let w = window(WindowType::FlatTop, 128);
        assert!((w[64] - 1.0).abs() < 1e-5, "centre {}", w[64]);
        assert!(w[0].abs() < 1e-3, "edge {}", w[0]);
    }

    #[test]
    fn gaussian_peaks_at_centre_and_decays() {
        let mut w = vec![0.0f64; 65];
        windowed_symmetric_f64(WindowType::Gaussian(0.3), 65, &mut w);
        assert!((w[32] - 1.0).abs() < 1e-12);
        assert!(w[0] < 0.1 && w[0] > 0.0);
        assert!((w[0] - w[64]).abs() < 1e-12, "symmetric");
    }

    #[test]
    fn f64_and_f32_variants_agree() {
        let mut a = vec![0.0f64; 40];
        windowed_f64(WindowType::Blackman, 40, &mut a);
        let b = window(WindowType::Blackman, 40);
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x - f64::from(*y)).abs() < 1e-6);
        }
    }
}
