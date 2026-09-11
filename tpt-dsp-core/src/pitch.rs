//! Pitch (fundamental frequency, F0) estimation: normalized autocorrelation
//! and AMDF (Average Magnitude Difference Function) estimators, with
//! voiced/unvoiced classification.
//!
//! Required by `tpt-media`'s Vox codec (and potentially useful elsewhere —
//! e.g. Aura's psychoacoustic model). Both estimators are allocation-free
//! (single pass over the search-lag range, no scratch buffer needed) and
//! generic over `f32`/`f64` via [`num_traits::Float`].
//!
//! # Convention
//!
//! A "pitch period" is the lag (in samples) at which a signal is most
//! self-similar. [`autocorrelation_pitch`] finds the lag that *maximizes*
//! normalized autocorrelation; [`amdf_pitch`] finds the lag that
//! *minimizes* the mean absolute sample-to-sample difference — the two are
//! complementary measures of periodicity. Both report a `confidence` in
//! `[0, 1]` and classify the signal as voiced/unvoiced by thresholding it,
//! since real speech alternates between periodic (voiced) and noise-like
//! (unvoiced) segments and a caller must be able to tell which is which.

use num_traits::Float;

/// Typical human speech fundamental-frequency (F0) range, in Hz — a
/// reasonable default search range when the caller has no more specific
/// requirement (covers the low end of a bass voice through the high end
/// of a child's/soprano voice).
pub const SPEECH_MIN_F0_HZ: f64 = 50.0;
/// See [`SPEECH_MIN_F0_HZ`].
pub const SPEECH_MAX_F0_HZ: f64 = 500.0;

/// Default voiced/unvoiced confidence threshold (normalized periodicity
/// strength, in `[0, 1]`) — a literature-typical value; classic
/// autocorrelation-based pitch trackers commonly use 0.3-0.5.
pub const DEFAULT_VOICED_THRESHOLD: f64 = 0.3;

/// Convert an F0 search range (in Hz) to a pitch-*period* search range (in
/// samples) at the given sample rate — higher frequency means a shorter
/// period, so `max_f0_hz` maps to the smaller sample count.
///
/// # Panics
///
/// Panics if `min_f0_hz <= 0`, `max_f0_hz <= 0`, or `min_f0_hz > max_f0_hz`.
pub fn hz_range_to_period_samples(
    sample_rate: f64,
    min_f0_hz: f64,
    max_f0_hz: f64,
) -> (usize, usize) {
    assert!(
        min_f0_hz > 0.0 && max_f0_hz > 0.0,
        "F0 bounds must be positive"
    );
    assert!(
        min_f0_hz <= max_f0_hz,
        "min_f0_hz must not exceed max_f0_hz"
    );
    let min_period = ((sample_rate / max_f0_hz).round() as usize).max(1);
    let max_period = ((sample_rate / min_f0_hz).round() as usize).max(min_period);
    (min_period, max_period)
}

/// Result of a pitch-period search.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PitchEstimate<F> {
    /// The estimated pitch period, in samples. `None` when `voiced` is
    /// `false` — an unvoiced/silent frame has no meaningful period, so
    /// callers must check `voiced` rather than use a stale/best-effort lag.
    pub period_samples: Option<usize>,
    /// Confidence in `[0, 1]` that the signal is periodic at
    /// `period_samples` (0 = no periodicity found, 1 = perfectly
    /// periodic over the searched window).
    pub confidence: F,
    /// Whether `confidence` cleared the voiced/unvoiced threshold that was
    /// passed in.
    pub voiced: bool,
}

impl<F: Float> PitchEstimate<F> {
    /// Convert `period_samples` to a fundamental frequency in Hz at the
    /// given sample rate. `None` if unvoiced.
    pub fn frequency_hz(&self, sample_rate: F) -> Option<F> {
        self.period_samples
            .map(|p| sample_rate / F::from(p).unwrap())
    }
}

/// Estimate the pitch period via normalized autocorrelation: for each lag
/// in `min_period..=max_period`, compute `Σ x[n]·x[n+lag] / Σ x[n]·x[n]`
/// and report the lag with the largest value.
///
/// `voiced_threshold` is the minimum normalized peak (in `[0, 1]`) to
/// classify the signal as voiced — see [`DEFAULT_VOICED_THRESHOLD`].
///
/// # Panics
///
/// Panics if `min_period == 0`, `min_period > max_period`, or
/// `signal.len() <= max_period` (there must be at least one sample of
/// overlap at the largest lag searched).
pub fn autocorrelation_pitch<F: Float>(
    signal: &[F],
    min_period: usize,
    max_period: usize,
    voiced_threshold: F,
) -> PitchEstimate<F> {
    assert!(min_period >= 1, "min_period must be at least 1");
    assert!(
        min_period <= max_period,
        "min_period must not exceed max_period"
    );
    assert!(
        signal.len() > max_period,
        "signal must be longer than max_period to correlate at the largest lag"
    );

    let mut r0 = F::zero();
    for &x in signal {
        r0 = r0 + x * x;
    }
    if r0 <= F::zero() {
        // Silence: no energy to be periodic in.
        return PitchEstimate {
            period_samples: None,
            confidence: F::zero(),
            voiced: false,
        };
    }

    // A signal that is genuinely periodic at `period` is, by construction,
    // *also* (near-)periodic at every multiple of `period` (2x, 3x, ...) —
    // the classic "octave error" in pitch detection. Since lags are
    // searched shortest-first, only replace the current best with a
    // *meaningfully* larger one (by more than a small absolute margin);
    // this keeps the fundamental rather than letting floating-point noise
    // between two otherwise-tied octave candidates pick a harmonic.
    let tie_margin = F::from(1e-4).unwrap();
    let mut best_lag = min_period;
    let mut best_norm = F::neg_infinity();
    for lag in min_period..=max_period {
        let mut acc = F::zero();
        for n in 0..(signal.len() - lag) {
            acc = acc + signal[n] * signal[n + lag];
        }
        let norm = acc / r0;
        if norm > best_norm + tie_margin {
            best_norm = norm;
            best_lag = lag;
        }
    }

    // A partial-window correlation can slightly exceed the full-signal
    // Cauchy-Schwarz bound of 1; clamp to keep `confidence` a valid [0,1].
    let confidence = clamp01(best_norm);
    let voiced = confidence >= voiced_threshold;
    PitchEstimate {
        period_samples: voiced.then_some(best_lag),
        confidence,
        voiced,
    }
}

/// Estimate the pitch period via the Average Magnitude Difference Function
/// (AMDF): for each lag in `min_period..=max_period`, compute
/// `mean(|x[n] - x[n+lag]|)` and report the lag with the *smallest* value
/// — periodic signals show a sharp minimum near the true period, the
/// opposite of autocorrelation's maximum.
///
/// Confidence is `1 - min/max` of the AMDF values seen across the searched
/// range: a sharp, deep minimum relative to the range's own scale means
/// high confidence; a flat AMDF (no clear minimum) means low confidence.
/// Same argument/threshold conventions as [`autocorrelation_pitch`].
///
/// # Panics
///
/// Panics if `min_period == 0`, `min_period > max_period`, or
/// `signal.len() <= max_period`.
pub fn amdf_pitch<F: Float>(
    signal: &[F],
    min_period: usize,
    max_period: usize,
    voiced_threshold: F,
) -> PitchEstimate<F> {
    assert!(min_period >= 1, "min_period must be at least 1");
    assert!(
        min_period <= max_period,
        "min_period must not exceed max_period"
    );
    assert!(
        signal.len() > max_period,
        "signal must be longer than max_period to correlate at the largest lag"
    );

    // Same octave-error concern as `autocorrelation_pitch`'s tie-break, but
    // AMDF's "good" values cluster near zero rather than near a bounded
    // maximum, so a fixed absolute margin isn't scale-invariant. Instead
    // require a new minimum to beat the current one by more than a small
    // fraction of the AMDF range observed *so far* — by the time the loop
    // reaches a later octave of an earlier near-zero minimum, `max_amdf`
    // has already captured the signal's typical (non-matching-lag) scale.
    let tie_fraction = F::from(1e-3).unwrap();
    let mut best_lag = min_period;
    let mut min_amdf = F::infinity();
    let mut max_amdf = F::zero();
    for lag in min_period..=max_period {
        let count = signal.len() - lag;
        let mut acc = F::zero();
        for n in 0..count {
            acc = acc + (signal[n] - signal[n + lag]).abs();
        }
        let mean_diff = acc / F::from(count).unwrap();
        if mean_diff > max_amdf {
            max_amdf = mean_diff;
        }
        if mean_diff < min_amdf - max_amdf * tie_fraction {
            min_amdf = mean_diff;
            best_lag = lag;
        }
    }

    let confidence = if max_amdf <= F::zero() {
        // Every lag agreed exactly (e.g. silence): nothing to distinguish
        // periodicity from, so report no confidence rather than a
        // spurious 1.0 from a 0/0-style comparison.
        F::zero()
    } else {
        clamp01(F::one() - min_amdf / max_amdf)
    };
    let voiced = confidence >= voiced_threshold;
    PitchEstimate {
        period_samples: voiced.then_some(best_lag),
        confidence,
        voiced,
    }
}

#[inline]
fn clamp01<F: Float>(x: F) -> F {
    if x > F::one() {
        F::one()
    } else if x < F::zero() {
        F::zero()
    } else {
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(len: usize, sample_rate: f64, freq_hz: f64) -> Vec<f64> {
        (0..len)
            .map(|n| (core::f64::consts::TAU * freq_hz * n as f64 / sample_rate).sin())
            .collect()
    }

    /// A deterministic, fixed-seed noise-like sequence (SplitMix64 — the
    /// well-known, well-mixed generator used e.g. to seed Java's
    /// `SplittableRandom`), folded into `[-0.5, 0.5)`. Used as a
    /// reproducible stand-in for "not periodic in this search window"
    /// without pulling in a `rand` dependency. A simpler per-index hash
    /// (e.g. `(n * constant) % small_modulus`) was tried first and
    /// rejected: it still had enough short-range structure for
    /// `autocorrelation_pitch` to find a spurious periodicity above the
    /// voiced threshold, which is exactly the kind of false positive this
    /// test exists to catch.
    fn splitmix64_noise(len: usize) -> Vec<f64> {
        let mut state: u64 = 0x9E3779B97F4A7C15;
        (0..len)
            .map(|_| {
                state = state.wrapping_add(0x9E3779B97F4A7C15);
                let mut z = state;
                z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
                z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
                z ^= z >> 31;
                (z as f64 / u64::MAX as f64) - 0.5
            })
            .collect()
    }

    #[test]
    fn hz_range_to_period_samples_matches_hand_computation() {
        // 8kHz sample rate, 50-500 Hz F0 range (SPEECH_MIN/MAX_F0_HZ):
        // period = sample_rate/freq, inverted since higher freq -> shorter period.
        let (min_p, max_p) = hz_range_to_period_samples(8000.0, 50.0, 500.0);
        assert_eq!(min_p, 16); // 8000/500
        assert_eq!(max_p, 160); // 8000/50
    }

    // Both estimators correlate over a *truncated* window (the cross term
    // at lag L only sums `signal.len() - L` terms), so even a perfect
    // periodic signal caps out below 1.0 confidence: roughly
    // `(len - max_period) / len` for a signal with fairly uniform energy
    // per period. Using a window several periods longer than `max_period`
    // (here 1600 samples vs. `max_period` = 160) keeps that shortfall
    // small (~90% confidence ceiling) while still being a realistic
    // multi-frame analysis window, rather than inflating the window
    // arbitrarily far just to make the assertion pass.
    const TEST_SIGNAL_LEN: usize = 1600;

    #[test]
    fn autocorrelation_pitch_recovers_known_period() {
        let sample_rate = 8000.0;
        let freq = 100.0; // period = 80 samples exactly
        let signal = sine(TEST_SIGNAL_LEN, sample_rate, freq);
        let (min_p, max_p) = hz_range_to_period_samples(sample_rate, 50.0, 500.0);
        let est = autocorrelation_pitch(&signal, min_p, max_p, 0.3);
        assert!(est.voiced, "a clean sine must be classified voiced");
        assert_eq!(
            est.period_samples,
            Some(80),
            "must recover the fundamental, not an octave"
        );
        assert!(est.confidence > 0.85, "confidence={}", est.confidence);
        assert!((est.frequency_hz(sample_rate).unwrap() - freq).abs() < 1e-6);
    }

    #[test]
    fn amdf_pitch_recovers_known_period() {
        let sample_rate = 8000.0;
        let freq = 100.0;
        let signal = sine(TEST_SIGNAL_LEN, sample_rate, freq);
        let (min_p, max_p) = hz_range_to_period_samples(sample_rate, 50.0, 500.0);
        let est = amdf_pitch(&signal, min_p, max_p, 0.3);
        assert!(est.voiced, "a clean sine must be classified voiced");
        assert_eq!(
            est.period_samples,
            Some(80),
            "must recover the fundamental, not an octave"
        );
        assert!(est.confidence > 0.85, "confidence={}", est.confidence);
    }

    #[test]
    fn silence_is_unvoiced_for_both_estimators() {
        let signal = vec![0.0f64; 400];
        let auto = autocorrelation_pitch(&signal, 16, 160, 0.3);
        assert!(!auto.voiced);
        assert_eq!(auto.period_samples, None);
        assert_eq!(auto.confidence, 0.0);

        let amdf = amdf_pitch(&signal, 16, 160, 0.3);
        assert!(!amdf.voiced);
        assert_eq!(amdf.period_samples, None);
        assert_eq!(amdf.confidence, 0.0);
    }

    #[test]
    fn hash_noise_is_unvoiced_for_both_estimators() {
        let signal = splitmix64_noise(400);
        let auto = autocorrelation_pitch(&signal, 16, 160, 0.3);
        assert!(!auto.voiced, "confidence={}", auto.confidence);

        let amdf = amdf_pitch(&signal, 16, 160, 0.3);
        assert!(!amdf.voiced, "confidence={}", amdf.confidence);
    }

    #[test]
    fn lower_threshold_can_flip_the_same_signal_to_voiced() {
        // The threshold is a caller-chosen policy knob, not baked into the
        // estimator: confirm a very permissive threshold accepts a signal
        // the default threshold would reject, using the same input.
        let signal = splitmix64_noise(400);
        let permissive = autocorrelation_pitch(&signal, 16, 160, 0.0);
        assert!(permissive.voiced);
        assert!(permissive.period_samples.is_some());
    }
}
