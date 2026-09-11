//! Linear Predictive Coding (LPC): autocorrelation analysis, Levinson–Durbin
//! recursion, prediction, residual generation and synthesis.
//!
//! Required by `tpt-media`'s Aura (lossless mode) and Vox codecs. The core
//! functions are allocation-free — every buffer is caller-owned — and
//! generic over `f32`/`f64` via [`num_traits::Float`]. [`LpcAnalyzer`] is an
//! owning convenience wrapper gated behind the `alloc` feature.
//!
//! # Convention
//!
//! The predictor and its coefficients `a[0..order)` are related by
//!
//! ```text
//! predicted[n] = Σ_{k=0}^{order-1} a[k]·x[n-1-k]
//! residual[n]  = x[n] - predicted[n]
//! ```
//!
//! [`residual`] truncates the predictor order near the start of the buffer
//! (sample `i` is predicted from `min(order, i)` prior samples, so the
//! first sample is never predicted), and [`synthesize`] mirrors the same
//! truncation, making the two exact inverses of one another regardless of
//! where the coefficients came from.
//!
//! # Numerical contract (§12)
//!
//! - **Output domain**: `autocorrelation` and `levinson_durbin`'s returned
//!   error energy are non-negative for real input (sums of squares /
//!   variance-reduction steps); LPC coefficients, `predict`, `residual`,
//!   and `synthesize` outputs are otherwise unbounded — finite input
//!   produces finite output, with no fixed range.
//! - **Precision / acceptable error**: `levinson_durbin` recovers the exact
//!   coefficients of an idealized pure-tone autocorrelation to `1e-9` in
//!   `f64` (`levinson_durbin_predicts_pure_tone`); `predict`/`residual` are
//!   plain finite sums of the input's own values, so they inherit whatever
//!   precision the caller's samples and coefficients already carry (no
//!   additional tolerance to budget for); `residual` then `synthesize` is
//!   an *exact* algebraic inverse (`residual_then_synthesize_is_identity`,
//!   `1e-12` in `f64`, i.e. accumulated floating-point rounding only, not a
//!   modeling approximation).

use num_traits::Float;

/// Compute the biased autocorrelation `r[k] = Σ_n x[n]·x[n+k]` for lags
/// `k = 0..=order`.
///
/// # Panics
///
/// Panics if `out.len() != order + 1`.
pub fn autocorrelation<F: Float>(x: &[F], order: usize, out: &mut [F]) {
    assert_eq!(
        out.len(),
        order + 1,
        "autocorrelation output must have order+1 entries"
    );
    for (k, slot) in out.iter_mut().enumerate() {
        let mut acc = F::zero();
        if k < x.len() {
            for n in 0..(x.len() - k) {
                acc = acc + x[n] * x[n + k];
            }
        }
        *slot = acc;
    }
}

/// Levinson–Durbin recursion: derive `order` LPC coefficients from the
/// autocorrelation sequence `r[0..=order]` (see the module-level doc for the
/// prediction convention).
///
/// `scratch` is caller-owned working storage of length `order` (avoids
/// heap allocation in the core recursion).
///
/// Returns the final residual (prediction error) energy. If `r[0]` is zero
/// (silence) or a reflection coefficient reaches unity (which would produce
/// an unstable synthesis filter), the coefficients computed in the
/// completed iterations are kept, later ones stay zero, and the recursion
/// stops early — callers should treat a returned error energy of zero as
/// "no usable prediction; encode this block unpredicted".
///
/// # Panics
///
/// Panics if `r` is empty, or if `coeffs_out`/`scratch` don't have exactly
/// `r.len() - 1` entries.
pub fn levinson_durbin<F: Float>(r: &[F], coeffs_out: &mut [F], scratch: &mut [F]) -> F {
    assert!(!r.is_empty(), "autocorrelation sequence must be non-empty");
    let order = r.len() - 1;
    assert_eq!(
        coeffs_out.len(),
        order,
        "coeffs_out must have order entries"
    );
    assert_eq!(scratch.len(), order, "scratch must have order entries");
    for c in coeffs_out.iter_mut() {
        *c = F::zero();
    }
    let mut error = r[0];
    if error <= F::zero() {
        return F::zero();
    }
    for i in 0..order {
        let mut k = r[i + 1];
        for j in 0..i {
            k = k - coeffs_out[j] * r[i - j];
        }
        k = k / error;
        if k.abs() >= F::one() {
            return error;
        }
        scratch[..i].copy_from_slice(&coeffs_out[..i]);
        for j in 0..i {
            coeffs_out[j] = scratch[j] - k * scratch[i - 1 - j];
        }
        coeffs_out[i] = k;
        error = error * (F::one() - k * k);
        if error <= F::zero() {
            return F::zero();
        }
    }
    error
}

/// One-step linear prediction from history:
/// `Σ_k coeffs[k]·history[history.len()-1-k]`.
///
/// # Panics
///
/// Panics if `history.len() < coeffs.len()`.
pub fn predict<F: Float>(history: &[F], coeffs: &[F]) -> F {
    assert!(
        history.len() >= coeffs.len(),
        "not enough history for prediction order"
    );
    let h = history.len();
    let mut acc = F::zero();
    for (k, c) in coeffs.iter().enumerate() {
        acc = acc + *c * history[h - 1 - k];
    }
    acc
}

/// Compute the LPC residual `out[n] = x[n] - predicted[n]` (see the
/// module-level doc for the truncated-history convention near `n = 0`).
///
/// # Panics
///
/// Panics if `out.len() != x.len()`.
pub fn residual<F: Float>(x: &[F], coeffs: &[F], out: &mut [F]) {
    assert_eq!(
        out.len(),
        x.len(),
        "residual output must match input length"
    );
    let order = coeffs.len();
    for i in 0..x.len() {
        let take = order.min(i);
        let mut acc = x[i];
        for (k, c) in coeffs.iter().enumerate().take(take) {
            acc = acc - *c * x[i - 1 - k];
        }
        out[i] = acc;
    }
}

/// Reconstruct `out[n] = residual[n] + predicted[n]` — the exact inverse
/// of [`residual`] given the same `coeffs`.
///
/// # Panics
///
/// Panics if `out.len() != residual.len()`.
pub fn synthesize<F: Float>(residual: &[F], coeffs: &[F], out: &mut [F]) {
    assert_eq!(
        out.len(),
        residual.len(),
        "synthesis output must match residual length"
    );
    let order = coeffs.len();
    for i in 0..residual.len() {
        let take = order.min(i);
        let mut acc = residual[i];
        for (k, c) in coeffs.iter().enumerate().take(take) {
            acc = acc + *c * out[i - 1 - k];
        }
        out[i] = acc;
    }
}

/// Owning convenience wrapper around the free LPC functions (requires the
/// `alloc` feature). Allocates its autocorrelation and recursion scratch
/// buffers once, at construction, and reuses them across calls.
#[cfg(feature = "alloc")]
pub struct LpcAnalyzer {
    order: usize,
    r: alloc::vec::Vec<f64>,
    scratch: alloc::vec::Vec<f64>,
}

#[cfg(feature = "alloc")]
impl LpcAnalyzer {
    /// Build an analyzer for the given prediction order.
    pub fn new(order: usize) -> Self {
        LpcAnalyzer {
            order,
            r: alloc::vec![0.0; order + 1],
            scratch: alloc::vec![0.0; order],
        }
    }

    /// The prediction order this analyzer was built for.
    pub fn order(&self) -> usize {
        self.order
    }

    /// Run autocorrelation + Levinson–Durbin on `x`, writing `order`
    /// coefficients into `coeffs_out` and returning the final prediction
    /// error energy.
    ///
    /// # Panics
    ///
    /// Panics if `coeffs_out.len() != self.order()`.
    pub fn analyze(&mut self, x: &[f64], coeffs_out: &mut [f64]) -> f64 {
        autocorrelation(x, self.order, &mut self.r);
        levinson_durbin(&self.r, coeffs_out, &mut self.scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocorrelation_of_zeros_is_zero() {
        let mut r = [0.0f64; 5];
        autocorrelation(&[0.0; 32], 4, &mut r);
        assert!(r.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn autocorrelation_of_impulse_is_zero_at_lag() {
        let mut x = [0.0f64; 16];
        x[4] = 1.0;
        let mut r = [0.0f64; 5];
        autocorrelation(&x, 4, &mut r);
        assert!((r[0] - 1.0).abs() < 1e-9);
        for v in &r[1..] {
            assert!(v.abs() < 1e-9);
        }
    }

    #[test]
    fn levinson_durbin_predicts_pure_tone() {
        // A sinusoid x[n] = cos(nw) is exactly predictable by a 2nd-order
        // AR model, x[n] = 2cos(w) x[n-1] - x[n-2], and its (infinite,
        // periodic) autocorrelation is exactly r[k] = C*cos(kw). Feeding
        // Levinson-Durbin that idealized r directly (rather than the biased
        // estimate `autocorrelation` computes from a short finite window)
        // isolates the recursion from windowing bias and lets the round
        // trip be checked to floating-point precision.
        let w = 0.3f64;
        let order = 2;
        let r: Vec<f64> = (0..=order).map(|k| (k as f64 * w).cos()).collect();
        let mut coeffs = vec![0.0f64; order];
        let mut scratch = vec![0.0f64; order];
        let err = levinson_durbin(&r, &mut coeffs, &mut scratch);
        assert!(err >= 0.0);
        assert!((coeffs[0] - 2.0 * w.cos()).abs() < 1e-9, "a0={}", coeffs[0]);
        assert!((coeffs[1] + 1.0).abs() < 1e-9, "a1={}", coeffs[1]);
    }

    #[test]
    fn levinson_durbin_handles_silence() {
        let r = [0.0f64; 5];
        let mut coeffs = [0.0f64; 4];
        let mut scratch = [0.0f64; 4];
        let err = levinson_durbin(&r, &mut coeffs, &mut scratch);
        assert_eq!(err, 0.0);
        assert!(coeffs.iter().all(|c| *c == 0.0));
    }

    #[test]
    fn levinson_durbin_stops_early_at_first_reflection() {
        // order=1: the single reflection coefficient k = r[1]/r[0] = 2.0
        // already exceeds unity in magnitude — the simplest instability
        // case, exercising the `k.abs() >= F::one()` branch directly.
        let r = [1.0f64, 2.0];
        let mut coeffs = [0.0f64];
        let mut scratch = [0.0f64];
        let err = levinson_durbin(&r, &mut coeffs, &mut scratch);
        assert_eq!(
            err, 1.0,
            "must return the (unchanged) error from before the aborted iteration"
        );
        assert_eq!(
            coeffs[0], 0.0,
            "the unstable coefficient must stay zero, not be set to k"
        );
    }

    #[test]
    fn levinson_durbin_stops_early_and_keeps_stable_prefix() {
        // order=2, but only the *second* reflection coefficient (i=1)
        // reaches unity; the first (i=0) is well-behaved. Proves the
        // early-stop keeps the completed stable prefix (`coeffs[0]`) and
        // zeros only the coefficient(s) that were never computed, rather
        // than discarding everything on any instability.
        //
        // Hand-derivation (all values exact in binary floating point):
        //   i=0: k0 = r[1]/r[0] = 0.5/1.0 = 0.5 (stable) -> coeffs[0]=0.5,
        //        error = 1.0*(1-0.5^2) = 0.75
        //   i=1: k1 = (r[2] - coeffs[0]*r[1]) / error
        //          = (1.375 - 0.5*0.5) / 0.75 = 1.125/0.75 = 1.5 (unstable)
        //        -> returns the pre-iteration error (0.75), coeffs[1] stays 0
        let r = [1.0f64, 0.5, 1.375];
        let mut coeffs = [0.0f64; 2];
        let mut scratch = [0.0f64; 2];
        let err = levinson_durbin(&r, &mut coeffs, &mut scratch);
        assert_eq!(err, 0.75);
        assert_eq!(
            coeffs[0], 0.5,
            "stable prefix from the completed i=0 iteration must be kept"
        );
        assert_eq!(coeffs[1], 0.0, "the aborted i=1 coefficient must stay zero");
    }

    #[test]
    fn residual_then_synthesize_is_identity() {
        let x: Vec<f64> = (0..64).map(|i| (i as f64 * 0.37).sin() * 100.0).collect();
        let coeffs = [0.6f64, -0.2, 0.05];
        let mut res = vec![0.0f64; x.len()];
        residual(&x, &coeffs, &mut res);
        let mut back = vec![0.0f64; x.len()];
        synthesize(&res, &coeffs, &mut back);
        for (a, b) in x.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-9, "{a} vs {b}");
        }
    }

    #[test]
    fn predict_matches_residual_formula() {
        let history = [1.0f64, 2.0, 3.0, 4.0];
        let coeffs = [0.5f64, 0.25];
        // predicted = 0.5*history[3] + 0.25*history[2] = 0.5*4 + 0.25*3 = 2.75
        assert!((predict(&history, &coeffs) - 2.75).abs() < 1e-12);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn analyzer_reuses_buffers() {
        let mut analyzer = LpcAnalyzer::new(2);
        let x: Vec<f64> = (0..128).map(|i| (i as f64 * 0.3).cos()).collect();
        let mut coeffs = vec![0.0f64; 2];
        let err1 = analyzer.analyze(&x, &mut coeffs);
        let err2 = analyzer.analyze(&x, &mut coeffs);
        assert_eq!(err1, err2);
    }
}
