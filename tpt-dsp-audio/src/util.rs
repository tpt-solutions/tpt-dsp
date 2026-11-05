//! Crate-private helpers shared by the oscillator-style generators.
//!
//! These keep phase accumulators bounded and reject non-finite or
//! non-positive parameters so a bad value from a UI or a device switch can't
//! wedge the audio thread (an unbounded `while phase >= 1.0` loop never
//! terminates for `inf` or very large steps).

/// Fallback sample rate used when a constructor is given an invalid one.
pub(crate) const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;

/// `true` when `sr` is a usable sample rate (finite and strictly positive).
pub(crate) fn is_valid_sample_rate(sr: f32) -> bool {
    sr.is_finite() && sr > 0.0
}

/// Sample rate to store: `sr` if valid, otherwise [`DEFAULT_SAMPLE_RATE`].
pub(crate) fn sanitize_sample_rate(sr: f32) -> f32 {
    if is_valid_sample_rate(sr) {
        sr
    } else {
        DEFAULT_SAMPLE_RATE
    }
}

/// Frequency to store: `f` if finite and non-negative, otherwise `0.0`.
pub(crate) fn sanitize_frequency(f: f32) -> f32 {
    if f.is_finite() && f >= 0.0 {
        f
    } else {
        0.0
    }
}

/// Duration in seconds: `t` clamped to `>= 0`, with non-finite values as `0.0`.
pub(crate) fn sanitize_time(t: f32) -> f32 {
    if t.is_finite() {
        t.max(0.0)
    } else {
        0.0
    }
}

/// Wrap a phase (in cycles) into `[0, 1)`.
///
/// Runs in constant time for any finite input, including steps far larger
/// than one cycle. Non-finite input collapses to `0.0`.
pub(crate) fn wrap_phase(p: f32) -> f32 {
    if !p.is_finite() {
        return 0.0;
    }
    let q = p - p.floor();
    if q >= 1.0 {
        0.0
    } else {
        q
    }
}
