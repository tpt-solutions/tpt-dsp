//! Windowing functions (Hann, Hamming, Blackman and friends).
//!
//! All window functions are symmetric (periodic for use with the FFT) and
//! allocate nothing: they write into a caller-provided slice.

// On `no_std` targets the inherent `f32::cos` does not exist, so pull in the
// `num_traits::Float` trait. Under `std` the inherent methods are used and the
// import would be flagged as unused, hence the cfg gate.
#[cfg(not(feature = "std"))]
use num_traits::Float;

/// The kind of spectral window to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Hann (raised cosine).
    Hann,
    /// Hamming.
    Hamming,
    /// Blackman (α = 0.16).
    Blackman,
}

/// Generate a window of `len` samples into `out`.
///
/// The window is *periodic* (the classic FFT convention): sample `i` equals
/// `w(2π·i / len)` so the first and last samples are equal. If `out` is
/// longer than `len`, only the first `len` entries are written.
///
/// # Panics
///
/// Panics if `len > out.len()`.
pub fn windowed(win: WindowType, len: usize, out: &mut [f32]) {
    assert!(len <= out.len(), "window longer than output buffer");
    let n = len as f32;
    let two_pi = core::f32::consts::TAU;
    for (i, slot) in out.iter_mut().take(len).enumerate() {
        let x = two_pi * i as f32 / n;
        *slot = match win {
            WindowType::Hann => 0.5 - 0.5 * x.cos(),
            WindowType::Hamming => 0.54 - 0.46 * x.cos(),
            WindowType::Blackman => 0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos(),
        };
    }
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
        for win in [WindowType::Hann, WindowType::Hamming, WindowType::Blackman] {
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
}
