//! Discrete Cosine Transforms (DCT-II, DCT-III, DCT-IV).
//!
//! Direct implementations (O(N²)) that work for any length and are
//! allocation-free. These are reference-quality baselines; profile-driven
//! callers may swap in a faster DCT for their specific length.

use num_traits::Float;

use crate::complex::pi;

/// DCT-II: `X[k] = Σ_n x[n]·cos(π/N·(n + ½)·k)`, for `k` in `0..N`.
///
/// This is the "the" DCT used by JPEG/MP3 analysis. Reads from `input`,
/// writes `input.len()` outputs into `out`.
pub fn dct_ii<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for DCT-II");
    let scale = pi::<F>() / F::from(n).unwrap();
    for k in 0..n {
        let kf = F::from(k).unwrap();
        let mut acc = F::zero();
        for (m, x) in input.iter().enumerate() {
            let theta = scale * kf * (F::from(m).unwrap() + F::from(0.5).unwrap());
            acc = acc + *x * theta.cos();
        }
        out[k] = acc;
    }
}

/// DCT-III: `x[n] = ½·X[0] + Σ_{k≥1} X[k]·cos(π/N·k·(n + ½))`.
///
/// The inverse of an unnormalized DCT-II (apply DCT-II then divide by `N`
/// to recover the original signal).
pub fn dct_iii<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for DCT-III");
    let scale = pi::<F>() / F::from(n).unwrap();
    for (m, slot) in out.iter_mut().take(n).enumerate() {
        let mf = F::from(m).unwrap() + F::from(0.5).unwrap();
        let mut acc = input[0] * F::from(0.5).unwrap();
        for (k, x) in input.iter().enumerate().skip(1) {
            let theta = scale * mf * F::from(k).unwrap();
            acc = acc + *x * theta.cos();
        }
        *slot = acc;
    }
}

/// DCT-IV: `X[k] = Σ_n x[n]·cos(π/N·(n + ½)·(k + ½))`.
///
/// Used by MDCT-based codecs as a building block.
pub fn dct_iv<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for DCT-IV");
    let scale = pi::<F>() / F::from(n).unwrap();
    for k in 0..n {
        let kf = F::from(k).unwrap() + F::from(0.5).unwrap();
        let mut acc = F::zero();
        for (m, x) in input.iter().enumerate() {
            let theta = scale * kf * (F::from(m).unwrap() + F::from(0.5).unwrap());
            acc = acc + *x * theta.cos();
        }
        out[k] = acc;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dct_ii_of_constant_is_scaled() {
        let input = [1.0f32; 16];
        let mut out = [0.0f32; 16];
        dct_ii(&input, &mut out);
        assert!((out[0] - 16.0).abs() < 1e-4);
        for x in out.iter().skip(1) {
            assert!(x.abs() < 1e-4);
        }
    }

    #[test]
    fn dct_ii_then_iii_roundtrips() {
        let input: Vec<f32> = (0..32).map(|i| (i as f32 * 0.37).sin()).collect();
        let mut dct = vec![0.0f32; 32];
        let mut back = vec![0.0f32; 32];
        dct_ii(&input, &mut dct);
        dct_iii(&dct, &mut back);
        for (a, b) in back.iter().zip(input.iter()) {
            // DCT-II then DCT-III recovers N/2·x (orthogonality constant).
            assert!((a - b * 16.0).abs() < 1e-3);
        }
    }

    #[test]
    fn dct_iv_orthogonality_roundtrip() {
        // DCT-IV is self-inverse (up to scaling): applying twice gives N·x.
        let input: Vec<f64> = (0..8).map(|i| (i as f64 * 1.7).cos()).collect();
        let mut first = vec![0.0f64; 8];
        let mut second = vec![0.0f64; 8];
        dct_iv(&input, &mut first);
        dct_iv(&first, &mut second);
        for (a, b) in second.iter().zip(input.iter()) {
            // DCT-IV is orthogonal with constant N/2: two passes give N/2·x.
            assert!((a - b * 4.0).abs() < 1e-9);
        }
    }
}
