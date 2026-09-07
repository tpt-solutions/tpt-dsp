//! Discrete Cosine Transforms (DCT-II, DCT-III, DCT-IV).
//!
//! Direct implementations (O(N²)) that work for any length and are
//! allocation-free. These are reference-quality baselines; profile-driven
//! callers may swap in a faster DCT for their specific length.

use num_traits::Float;

use crate::complex::cos_pi_over_n;

/// DCT-II: `X[k] = Σ_n x[n]·cos(π/N·(n + ½)·k)`, for `k` in `0..N`.
///
/// This is the "the" DCT used by JPEG/MP3 analysis. Reads from `input`,
/// writes `input.len()` outputs into `out`.
pub fn dct_ii<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for DCT-II");
    for (k, slot) in out.iter_mut().take(n).enumerate() {
        let two_k = (2 * k) as i64;
        let mut acc = F::zero();
        for (m, x) in input.iter().enumerate() {
            let two_m = (2 * m + 1) as i64;
            acc = acc + *x * cos_pi_over_n::<F>(n, two_k, two_m);
        }
        *slot = acc;
    }
}

/// DCT-III: `x[n] = ½·X[0] + Σ_{k≥1} X[k]·cos(π/N·k·(n + ½))`.
///
/// The inverse of an unnormalized DCT-II (apply DCT-II then divide by `N`
/// to recover the original signal).
pub fn dct_iii<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert!(out.len() >= n, "output too small for DCT-III");
    for (m, slot) in out.iter_mut().take(n).enumerate() {
        let two_m = (2 * m + 1) as i64;
        let mut acc = input[0] * F::from(0.5).unwrap();
        for (k, x) in input.iter().enumerate().skip(1) {
            let two_k = (2 * k) as i64;
            acc = acc + *x * cos_pi_over_n::<F>(n, two_m, two_k);
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
    for (k, slot) in out.iter_mut().take(n).enumerate() {
        let two_k = (2 * k + 1) as i64;
        let mut acc = F::zero();
        for (m, x) in input.iter().enumerate() {
            let two_m = (2 * m + 1) as i64;
            acc = acc + *x * cos_pi_over_n::<F>(n, two_k, two_m);
        }
        *slot = acc;
    }
}

/// Fast DCT-IV for a fixed size `n`, backed by a single length-`2n` complex
/// FFT ([`crate::FftPlan`], RustFFT-backed and hardware-accelerated where
/// available) instead of the direct O(N²) sum in [`dct_iv`].
///
/// `std`-only (needs [`crate::FftPlan`]) and `f32`-only (matches
/// `FftPlan`'s precision). DCT-IV is self-inverse up to the `N/2` scaling
/// `dct_iv_orthogonality_roundtrip` documents, so `forward` also serves as
/// the fast inverse.
///
/// # Derivation
///
/// Same chirp/FFT technique as
/// [`crate::mdct::FastMdctPlan`](../mdct/struct.FastMdctPlan.html),
/// specialised to DCT-IV's `θ(m,k) = (π/N)(m+½)(k+½)`: both indices carry a
/// half-integer offset and both range only `0..N` (unlike MDCT, where one
/// index already spans `0..2N`), so the input is zero-padded to length `2N`
/// before the single forward FFT rather than already filling it. See
/// `fast_matches_reference` below for the numerical proof against the
/// direct reference.
#[cfg(feature = "std")]
pub struct FastDctIvPlan {
    n: usize,
    fft: crate::plan::FftPlan,
    /// `e^{jφ(m)}`, `φ(m) = (π/(2n))·m`, `m = 0..n`.
    phi: std::vec::Vec<crate::complex::C32>,
    /// `e^{jψ(k)}`, `ψ(k) = (π/(2n))·k + π/(4n)`, `k = 0..n`.
    psi: std::vec::Vec<crate::complex::C32>,
    scratch: std::vec::Vec<crate::complex::C32>,
}

#[cfg(feature = "std")]
impl FastDctIvPlan {
    /// Build a fast plan for size `n`.
    ///
    /// # Panics
    ///
    /// Panics if `n == 0`.
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "dct-iv size must be positive");
        use crate::complex::exp_i;
        let two_n = 2 * n;
        let fft = crate::plan::FftPlan::new_forward(two_n);
        let step = core::f32::consts::PI / (2.0 * n as f32);
        let phi = (0..n).map(|m| exp_i(step * m as f32)).collect();
        let psi = (0..n)
            .map(|k| exp_i(step * k as f32 + core::f32::consts::PI / (4.0 * n as f32)))
            .collect();
        let scratch = std::vec![crate::complex::C32::new(0.0, 0.0); two_n];
        FastDctIvPlan { n, fft, phi, psi, scratch }
    }

    /// Transform size `N`.
    pub fn n(&self) -> usize {
        self.n
    }

    /// Forward (equivalently, inverse up to the `N/2` scaling) DCT-IV.
    /// Both `input.len()` and `out.len()` must be `n`.
    ///
    /// # Panics
    ///
    /// Panics if the lengths don't match the plan's size.
    pub fn forward(&mut self, input: &[f32], out: &mut [f32]) {
        assert_eq!(input.len(), self.n, "input length must be n");
        assert_eq!(out.len(), self.n, "output length must be n");
        for slot in self.scratch.iter_mut() {
            *slot = crate::complex::C32::new(0.0, 0.0);
        }
        for (m, x) in input.iter().enumerate() {
            self.scratch[m] = crate::complex::C32::new(*x, 0.0) * self.phi[m].conj();
        }
        self.fft.process_inplace(&mut self.scratch);
        for (k, slot) in out.iter_mut().enumerate() {
            *slot = (self.psi[k] * self.scratch[k].conj()).re;
        }
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

    #[cfg(feature = "std")]
    #[test]
    fn fast_dct_iv_matches_reference() {
        for &n in &[8usize, 32, 128, 1024] {
            let input: Vec<f32> = (0..n)
                .map(|i| (i as f32 * 0.137).sin() + 0.4 * (i as f32 * 0.051).cos())
                .collect();

            let mut reference = vec![0.0f32; n];
            dct_iv(&input, &mut reference);

            let mut fast_plan = FastDctIvPlan::new(n);
            let mut fast = vec![0.0f32; n];
            fast_plan.forward(&input, &mut fast);

            for (k, (r, f)) in reference.iter().zip(fast.iter()).enumerate() {
                let tol = 1e-4 * r.abs().max(1.0);
                assert!((r - f).abs() < tol, "n={n} bin {k}: reference {r} vs fast {f}");
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn fast_dct_iv_is_self_inverse_up_to_scale() {
        let n = 64usize;
        let input: Vec<f32> = (0..n).map(|i| (i as f32 * 1.7).cos()).collect();
        let mut plan = FastDctIvPlan::new(n);
        let mut first = vec![0.0f32; n];
        let mut second = vec![0.0f32; n];
        plan.forward(&input, &mut first);
        plan.forward(&first, &mut second);
        for (a, b) in second.iter().zip(input.iter()) {
            assert!((a - b * (n as f32 / 2.0)).abs() < 1e-2, "a={a} b={b}");
        }
    }
}
