//! Noise-shaping quantization: an error-feedback filter that reshapes the
//! *spectrum* of quantization noise (e.g. pushing it toward frequencies a
//! psychoacoustic model — [`crate::psychoacoustic`] — says are masked)
//! without changing its total power.
//!
//! Useful for Aura's lossy mode. The free function [`noise_shape_step`]
//! is allocation-free (caller-owned error history); [`NoiseShaper`] (an
//! owning convenience wrapper) requires the `alloc` feature.
//!
//! # How it works
//!
//! For input `x[n]`, feedback coefficients `h[0..order]`, and error
//! history `e[n-1], e[n-2], ..., e[n-order]` (most recent first):
//!
//! ```text
//! shaped[n]  = x[n] - Σ_k h[k]·e[n-1-k]
//! y[n]       = quantize(shaped[n])
//! e[n]       = y[n] - shaped[n]
//! ```
//!
//! The past error is *subtracted* (not added) — this is what makes the
//! output track the input rather than its own quantization decisions.
//! For the simplest first-order case (`h = [1]`), `shaped[n] = x[n] -
//! e[n-1]`, and since `e[n] = y[n] - shaped[n]`, substituting gives
//! `y[n] - x[n] = e[n] - e[n-1]` — the classic noise transfer function
//! `NTF(z) = 1 - z⁻¹` (a first-order high-pass) applied to the
//! quantization error, and its sum over any run of samples *telescopes*
//! to just the two endpoint errors: `Σ(y[n]-x[n]) = e[N-1] - e[-1]`,
//! bounded regardless of how long the run is — unlike plain (memoryless)
//! quantization of e.g. a constant-but-unquantizable input, whose
//! `Σ(y[n]-x[n])` grows linearly with `N`. See
//! `shaping_bounds_the_cumulative_error_unlike_plain_quantization` below.

use num_traits::Float;

/// Process one sample through an order-`feedback.len()` noise-shaping
/// filter. `error_history[0]` must be the most recent past quantization
/// error, `error_history[1]` the one before that, etc.
/// (`error_history.len()` must equal `feedback.len()`, the filter order).
/// `quantize` maps a real value to its quantized representation (e.g.
/// round to the nearest step). Returns the quantized output and shifts
/// `error_history` in place, inserting this step's new error at index 0.
///
/// # Panics
///
/// Panics if `feedback.len() != error_history.len()`.
pub fn noise_shape_step<F: Float>(
    input: F,
    feedback: &[F],
    error_history: &mut [F],
    quantize: impl Fn(F) -> F,
) -> F {
    assert_eq!(feedback.len(), error_history.len(), "feedback and error_history must be the same length");
    let mut predicted_error = F::zero();
    for (h, e) in feedback.iter().zip(error_history.iter()) {
        predicted_error = predicted_error + *h * *e;
    }
    let shaped = input - predicted_error;
    let output = quantize(shaped);
    let error = output - shaped;
    for i in (1..error_history.len()).rev() {
        error_history[i] = error_history[i - 1];
    }
    if let Some(slot) = error_history.first_mut() {
        *slot = error;
    }
    output
}

/// Empirically check whether an order-`feedback.len()` noise-shaping
/// filter stays stable with a *particular* `quantize` function: simulate
/// `steps` iterations against a constant worst-case input (exactly
/// halfway between two notional quantization levels — the point of
/// maximum quantization ambiguity) and confirm every resulting error
/// stays within `bound`.
///
/// This matters for a `quantize` function that is *not* simple
/// round-to-nearest: round-to-nearest has a mathematical guarantee that
/// makes this check somewhat moot for it specifically — its error is
/// bounded within half a step by definition, for *any* finite feedback
/// coefficients, since `e[n] = y[n] - shaped[n]` is exactly that
/// rounding residual regardless of what `shaped[n]` is (see
/// `error_is_bounded_regardless_of_feedback_gain_for_round_to_nearest`
/// below). A quantizer that *isn't* self-limiting this way — e.g. one
/// with a gain other than 1 around its decision points — genuinely can
/// destabilize under feedback (see
/// `stability_check_catches_a_quantizer_without_a_bounded_error_property`
/// below), which is what this function is for.
///
/// # Panics
///
/// Panics if `scratch.len() != feedback.len()`.
pub fn noise_shaper_is_stable<F: Float>(
    feedback: &[F],
    scratch: &mut [F],
    worst_case_input: F,
    quantize: impl Fn(F) -> F,
    steps: usize,
    bound: F,
) -> bool {
    assert_eq!(scratch.len(), feedback.len(), "scratch must match feedback length");
    for slot in scratch.iter_mut() {
        *slot = F::zero();
    }
    for _ in 0..steps {
        let _ = noise_shape_step(worst_case_input, feedback, scratch, &quantize);
        if scratch.iter().any(|e| e.abs() > bound) {
            return false;
        }
    }
    true
}

/// Owning convenience wrapper around [`noise_shape_step`] (requires the
/// `alloc` feature): owns the feedback coefficients and error-history
/// scratch, reused across calls.
#[cfg(feature = "alloc")]
pub struct NoiseShaper<F> {
    feedback: alloc::vec::Vec<F>,
    error_history: alloc::vec::Vec<F>,
}

#[cfg(feature = "alloc")]
impl<F: Float> NoiseShaper<F> {
    /// Build a shaper for the given feedback coefficients (its length is
    /// the filter order).
    pub fn new(feedback: alloc::vec::Vec<F>) -> Self {
        let order = feedback.len();
        NoiseShaper { feedback, error_history: alloc::vec![F::zero(); order] }
    }

    /// Filter order (number of feedback taps).
    pub fn order(&self) -> usize {
        self.feedback.len()
    }

    /// Clear the error history (e.g. between independent streams).
    pub fn reset(&mut self) {
        for e in self.error_history.iter_mut() {
            *e = F::zero();
        }
    }

    /// See [`noise_shape_step`].
    pub fn process(&mut self, input: F, quantize: impl Fn(F) -> F) -> F {
        noise_shape_step(input, &self.feedback, &mut self.error_history, quantize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_to_step<F: Float>(step: F) -> impl Fn(F) -> F {
        move |x: F| (x / step).round() * step
    }

    #[test]
    fn shaping_tracks_a_dc_value_far_more_accurately_on_average_than_plain_quantization() {
        // The classic demonstration of noise shaping's benefit (this is
        // literally how a 1-bit delta-sigma DAC works): for a *constant*
        // input that isn't already a quantization level, plain
        // (memoryless) quantization repeats the exact same rounding
        // decision forever, so its long-run average is off by the full
        // rounding bias. Feeding the rounding error back (subtracted, per
        // the module doc's `NTF=1-z⁻¹` derivation) perturbs each
        // subsequent decision, so the shaped output's long-run average
        // converges much closer to the true value, even though every
        // individual sample is still one of the same coarse levels. (By
        // hand: x=0.3, h=[1] settles into an exact period-10 cycle whose
        // mean output is exactly 0.3 — so `n` a multiple of 10 recovers
        // it exactly, not just approximately.)
        let step = 1.0f64;
        let quantize = round_to_step(step);
        let x = 0.3f64;
        let n = 200; // 20 full periods of the period-10 cycle

        let plain_avg_error = (quantize(x) - x).abs(); // never changes: same decision every time

        let mut history = [0.0f64; 1];
        let feedback = [1.0f64]; // classic first-order error-feedback shaper
        let shaped_sum: f64 = (0..n).map(|_| noise_shape_step(x, &feedback, &mut history, &quantize)).sum();
        let shaped_avg_error = (shaped_sum / n as f64 - x).abs();

        assert!(
            shaped_avg_error < plain_avg_error,
            "shaped avg error={shaped_avg_error} vs plain={plain_avg_error}"
        );
        assert!(shaped_avg_error < 1e-9, "shaped average should recover {x} almost exactly, got avg error {shaped_avg_error}");
    }

    #[test]
    fn cumulative_error_stays_bounded_unlike_plain_quantization() {
        // The module doc's telescoping identity (h=[1]):
        // Sigma(y[n]-x[n]) = e[N-1] - e[-1], bounded by `step` regardless
        // of how long the run is (each e is itself a rounding residual,
        // bounded by step/2). Plain quantization of the same
        // unquantizable constant instead repeats the same nonzero bias
        // every sample, so its cumulative sum grows linearly with N.
        let step = 1.0f64;
        let quantize = round_to_step(step);
        let x = 0.3f64;
        let n = 200;

        let plain_cumulative: f64 = (0..n).map(|_| quantize(x) - x).sum();
        assert!(plain_cumulative.abs() > 50.0, "plain cumulative error should grow with N, got {plain_cumulative}");

        let mut history = [0.0f64];
        let feedback = [1.0f64];
        let shaped_cumulative: f64 =
            (0..n).map(|_| noise_shape_step(x, &feedback, &mut history, &quantize) - x).sum();
        assert!(shaped_cumulative.abs() < 1.0, "shaped cumulative error must stay bounded, got {shaped_cumulative}");
    }

    #[test]
    fn error_history_updates_and_shifts_correctly() {
        let feedback = [0.5f64, 0.25];
        let mut history = [0.0f64, 0.0];
        let quantize = round_to_step(1.0f64);

        // Step 1: input 0.6, predicted_error=0 -> shaped=0.6 -> y=1.0 -> e=1.0-0.6=0.4
        let y1 = noise_shape_step(0.6, &feedback, &mut history, &quantize);
        assert_eq!(y1, 1.0);
        assert!((history[0] - 0.4).abs() < 1e-9, "history={history:?}");
        assert_eq!(history[1], 0.0);

        // Step 2: predicted_error = 0.5*0.4 + 0.25*0.0 = 0.2 -> shaped=0.6-0.2=0.4 -> y=0.0 -> e=0.0-0.4=-0.4
        let y2 = noise_shape_step(0.6, &feedback, &mut history, &quantize);
        assert_eq!(y2, 0.0);
        assert!((history[0] - (-0.4)).abs() < 1e-9, "history={history:?}");
        assert!((history[1] - 0.4).abs() < 1e-9, "history={history:?} (previous e shifted into slot 1)");
    }

    #[test]
    fn error_is_bounded_regardless_of_feedback_gain_for_round_to_nearest() {
        // Round-to-nearest is self-limiting: e[n] = y[n]-shaped[n] is
        // exactly a rounding residual, bounded within half a step no
        // matter what `shaped[n]` is — so no finite feedback gain can
        // make it unbounded when `quantize` really is round-to-nearest.
        let mut scratch = [0.0f64];
        for &gain in &[1.0f64, 3.0, 1000.0] {
            assert!(
                noise_shaper_is_stable(&[gain], &mut scratch, 0.5, round_to_step(1.0), 500, 0.500_001),
                "gain={gain} should stay within half a step regardless"
            );
        }
    }

    #[test]
    fn stability_check_catches_a_quantizer_without_a_bounded_error_property() {
        // A "quantizer" whose own gain isn't 1 around its decision point
        // (unlike real round-to-nearest) genuinely can destabilize under
        // feedback: `bad_quantize(x) = 1.5x` gives `e[n] = y[n]-shaped[n]
        // = 0.5*shaped[n]`, so for constant input `x`,
        // `shaped[n] = x - 0.5h*shaped[n-1]` is a linear recursion with
        // ratio `r = -0.5h`. `|r|<1` converges (h=0.2 -> r=-0.1); `|r|>1`
        // diverges (h=3 -> r=-1.5).
        let bad_quantize = |x: f64| x * 1.5;
        let mut scratch = [0.0f64];
        assert!(noise_shaper_is_stable(&[0.2f64], &mut scratch, 0.5, bad_quantize, 40, 100.0));
        assert!(!noise_shaper_is_stable(&[3.0f64], &mut scratch, 0.5, bad_quantize, 40, 100.0));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn wrapper_matches_free_function_and_reset_clears_state() {
        let mut shaper = NoiseShaper::new(alloc::vec![1.0f64]);
        assert_eq!(shaper.order(), 1);
        let quantize = round_to_step(1.0f64);

        // Drive both the wrapper and a parallel free-function call with
        // the same input sequence (x=0.3 genuinely varies decisions step
        // to step, unlike 0.6 — see the DC-tracking test above for why
        // 0.3's feedback pattern isn't period-1) and confirm they agree
        // at every step.
        let mut history = [0.0f64];
        for _ in 0..5 {
            let wrapped = shaper.process(0.3, &quantize);
            let free = noise_shape_step(0.3, &[1.0], &mut history, &quantize);
            assert_eq!(wrapped, free);
        }

        shaper.reset();
        let mut history = [0.0f64];
        let a = shaper.process(0.6, &quantize);
        let c = noise_shape_step(0.6, &[1.0], &mut history, &quantize);
        assert_eq!(a, c, "after reset, behavior must match a fresh call");
    }
}
