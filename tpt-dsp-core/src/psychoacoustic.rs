//! Psychoacoustic modeling: threshold-in-quiet, tonal/noise
//! classification, spectral spreading, and the combined simultaneous
//! masking threshold used to decide how much quantization noise a signal
//! can tolerate before it becomes audible.
//!
//! Required primarily by Aura's lossy mode (§10.2), built on the Bark
//! frequency scale ([`crate::bark`]). These are the well-known generic
//! building blocks of perceptual audio coding — Terhardt's (1979)
//! threshold-in-quiet approximation, Schroeder's (1979) masking-spread
//! function, and the tonal/noise masking offsets used by e.g. MPEG
//! psychoacoustic model 1 — rather than a byte-for-byte reproduction of
//! any one standard's exact model. Aura's own tuning belongs in the
//! codec, not here.
//!
//! # Pipeline
//!
//! A typical caller: compute a power spectrum, aggregate it into Bark
//! bands ([`crate::bark::aggregate_bands`]), classify each band as
//! tonal/noise-dominated ([`classify_tonal_bins`] on the pre-aggregation
//! spectrum), spread each band's energy across the Bark axis with the
//! appropriate tonal/noise offset ([`simultaneous_masking`]), then
//! combine with [`threshold_in_quiet`] ([`combined_threshold_db`]) to get
//! a final per-band masking threshold, and finally compare that to the
//! signal's own energy ([`perceptual_weight_db`], the signal-to-mask
//! ratio) to decide bit allocation.

use num_traits::Float;

/// Convert a power (energy) ratio to decibels: `10*log10(power)`.
///
/// # Panics
///
/// Panics if `power <= 0`.
pub fn power_to_db<F: Float>(power: F) -> F {
    assert!(power > F::zero(), "power must be positive");
    F::from(10.0).unwrap() * power.log10()
}

/// Convert decibels back to a power (energy) ratio: `10^(db/10)`.
pub fn db_to_power<F: Float>(db: F) -> F {
    F::from(10.0).unwrap().powf(db / F::from(10.0).unwrap())
}

/// Absolute threshold of hearing (Terhardt, 1979 approximation), in dB
/// SPL, at `freq_hz`:
///
/// ```text
/// 3.64*(f/1000)^-0.8 - 6.5*exp(-0.6*(f/1000-3.3)^2) + 0.001*(f/1000)^4
/// ```
///
/// Rises sharply as `freq_hz` approaches 0 and again above ~15kHz;
/// most sensitive (lowest threshold) around 2-5kHz.
///
/// # Panics
///
/// Panics if `freq_hz <= 0` (the first term is singular at 0).
pub fn threshold_in_quiet<F: Float>(freq_hz: F) -> F {
    assert!(freq_hz > F::zero(), "frequency must be positive");
    let f_khz = freq_hz / F::from(1000.0).unwrap();
    let term1 = F::from(3.64).unwrap() * f_khz.powf(F::from(-0.8).unwrap());
    let diff = f_khz - F::from(3.3).unwrap();
    let term2 = F::from(6.5).unwrap() * (F::from(-0.6).unwrap() * diff * diff).exp();
    let term3 = F::from(0.001).unwrap() * f_khz.powi(4);
    term1 - term2 + term3
}

/// Is `power[bin]` a "tonal" local peak — strictly greater than both
/// immediate neighbors, and by at least `prominence_db` decibels?
/// (A simplified stand-in for MPEG psychoacoustic model 1's
/// frequency-dependent multi-neighbor tonality test: the same core idea —
/// a sharp local maximum indicates a sinusoidal component — without the
/// frequency-region-specific neighbor windows.)
///
/// Always `false` at the spectrum's endpoints (`bin == 0` or
/// `bin == power.len()-1`), since there's no interior neighbor on one
/// side.
///
/// # Panics
///
/// Panics if `bin >= power.len()`.
pub fn is_tonal_peak<F: Float>(power: &[F], bin: usize, prominence_db: F) -> bool {
    assert!(bin < power.len(), "bin out of range");
    if bin == 0 || bin == power.len() - 1 {
        return false;
    }
    let center = power[bin];
    let left = power[bin - 1];
    let right = power[bin + 1];
    if center <= left || center <= right {
        return false;
    }
    let ratio = db_to_power(prominence_db);
    center >= left * ratio && center >= right * ratio
}

/// Classify every interior bin of `power` via [`is_tonal_peak`], writing
/// the result to `out` (endpoints always `false`).
///
/// # Panics
///
/// Panics if `out.len() != power.len()`.
pub fn classify_tonal_bins<F: Float>(power: &[F], prominence_db: F, out: &mut [bool]) {
    assert_eq!(out.len(), power.len(), "out length must match power length");
    for (bin, slot) in out.iter_mut().enumerate() {
        *slot = is_tonal_peak(power, bin, prominence_db);
    }
}

/// Schroeder (1979) masking-spread function, in dB, at a Bark-domain
/// distance `dz = z_maskee - z_masker` (positive: maskee is above the
/// masker in frequency; negative: below):
///
/// ```text
/// SF(dz) = 15.81 + 7.5*(dz+0.474) - 17.5*sqrt(1+(dz+0.474)^2)
/// ```
///
/// Peaks at `dz = 0` (the masker's own band: setting `u = dz+0.474`, the
/// derivative `7.5 - 17.5u/sqrt(1+u²)` is zero at `u = 0.474`, i.e.
/// `dz = 0`) and falls off in both directions, asymmetrically — masking
/// spreads further toward higher frequencies than lower, matching the
/// basilar membrane's known asymmetric excitation pattern.
pub fn spreading_function_db<F: Float>(dz: F) -> F {
    let shifted = dz + F::from(0.474).unwrap();
    F::from(15.81).unwrap() + F::from(7.5).unwrap() * shifted
        - F::from(17.5).unwrap() * (F::one() + shifted * shifted).sqrt()
}

/// Masking offset (dB) subtracted from a tonal masker's spread excitation
/// level to get its actual masking contribution — tonal maskers are
/// (perceptually) weaker maskers of noise than noise maskers are of
/// tones, so they need a *larger* offset. Standard MPEG psychoacoustic
/// model 1 value: `14.5 + z` (grows with the masker's own Bark position).
pub fn tonal_masking_offset_db<F: Float>(masker_bark: F) -> F {
    F::from(14.5).unwrap() + masker_bark
}

/// Masking offset (dB) subtracted from a noise masker's spread excitation
/// level. Standard MPEG psychoacoustic model 1 value: a constant `5.5`.
pub fn noise_masking_offset_db<F: Float>() -> F {
    F::from(5.5).unwrap()
}

/// Compute the simultaneous-masking excitation (linear power, *not* yet
/// combined with the threshold in quiet — see [`combined_threshold_db`])
/// at each of `out_bark`'s Bark positions, given a set of masker bands
/// (`band_energy` linear power, `band_bark` Bark center, `band_tonal`
/// tonal/noise classification — all the same length).
///
/// For each output position `z_j`, sums every masker's contribution:
/// `energy_i * 10^((spreading_function_db(z_j - z_i) - offset_i) / 10)`.
///
/// `out` is *not* zeroed first, so callers can accumulate multiple
/// masker sets into the same output; zero it yourself for a fresh
/// computation.
///
/// # Panics
///
/// Panics if `band_energy`, `band_bark`, and `band_tonal` don't all have
/// the same length.
pub fn simultaneous_masking<F: Float>(
    band_energy: &[F],
    band_bark: &[F],
    band_tonal: &[bool],
    out_bark: &[F],
    out: &mut [F],
) {
    assert_eq!(band_energy.len(), band_bark.len(), "band arrays must be the same length");
    assert_eq!(band_energy.len(), band_tonal.len(), "band arrays must be the same length");
    assert_eq!(out.len(), out_bark.len(), "out must match out_bark length");

    for (slot, &zj) in out.iter_mut().zip(out_bark.iter()) {
        let mut acc = F::zero();
        for ((&energy, &zi), &tonal) in band_energy.iter().zip(band_bark.iter()).zip(band_tonal.iter()) {
            let offset = if tonal { tonal_masking_offset_db(zi) } else { noise_masking_offset_db() };
            let spread_db = spreading_function_db(zj - zi) - offset;
            acc = acc + energy * db_to_power(spread_db);
        }
        *slot = *slot + acc;
    }
}

/// Combine a simultaneous-masking excitation (linear power, from
/// [`simultaneous_masking`]) with the absolute threshold in quiet
/// (dB, from [`threshold_in_quiet`]) into the final masking threshold, in
/// dB: whichever is higher wins, since a masker can only raise the
/// audibility threshold above what it already is in silence.
///
/// # Panics
///
/// Panics if `masking_excitation_linear <= 0`.
pub fn combined_threshold_db<F: Float>(masking_excitation_linear: F, quiet_threshold_db: F) -> F {
    assert!(masking_excitation_linear > F::zero(), "masking excitation must be positive");
    power_to_db(masking_excitation_linear).max(quiet_threshold_db)
}

/// Signal-to-mask ratio (SMR), in dB: how far the signal's own energy
/// sits above (positive) or below (negative) the masking threshold at
/// the same position. This is the standard "perceptual weight" a bit
/// allocator uses: a high SMR means the ear would notice quantization
/// noise there (needs more bits/finer quantization); a low or negative
/// SMR means noise can hide under the mask (can tolerate a coarser
/// quantization step).
pub fn perceptual_weight_db<F: Float>(signal_energy_db: F, masking_threshold_db: F) -> F {
    signal_energy_db - masking_threshold_db
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bark::hz_to_bark;

    #[test]
    fn threshold_in_quiet_is_lowest_around_a_few_khz() {
        // The ear is most sensitive (lowest absolute threshold) somewhere
        // in the 2-5kHz region, and much less sensitive at the extremes.
        let low = threshold_in_quiet(100.0f64);
        let mid = threshold_in_quiet(3000.0f64);
        let high = threshold_in_quiet(15000.0f64);
        assert!(mid < low, "mid={mid} low={low}");
        assert!(mid < high, "mid={mid} high={high}");
    }

    #[test]
    fn is_tonal_peak_finds_a_sharp_spike_but_not_a_flat_region() {
        let power = [1.0f64, 1.0, 100.0, 1.0, 1.0, 1.0, 1.0];
        let mut classified = [false; 7];
        classify_tonal_bins(&power, 7.0, &mut classified);
        assert_eq!(classified, [false, false, true, false, false, false, false]);
    }

    #[test]
    fn is_tonal_peak_rejects_a_shoulder_below_the_prominence_threshold() {
        // A gentle local max (2x neighbors, well under a 7dB/~5x ratio)
        // must not be classified tonal.
        let power = [1.0f64, 1.0, 2.0, 1.0, 1.0];
        assert!(!is_tonal_peak(&power, 2, 7.0));
    }

    #[test]
    fn spreading_function_peaks_at_zero_and_decays_outward() {
        // Peak is exactly at dz=0 (see the function's doc comment for the
        // calculus): the derivative 7.5 - 17.5u/sqrt(1+u^2), u=dz+0.474,
        // is zero at u=0.474 i.e. dz=0.
        let peak = spreading_function_db(0.0f64);
        assert!(spreading_function_db(1.0f64) < peak, "must decrease moving away from dz=0");
        assert!(spreading_function_db(-1.0f64) < peak, "must decrease moving away from dz=0");
        assert!(spreading_function_db(-10.0f64) < peak - 20.0, "must decay well below the peak far away");
        assert!(spreading_function_db(10.0f64) < peak - 20.0, "must decay well below the peak far away");
    }

    #[test]
    fn power_db_round_trip() {
        for &p in &[0.001f64, 1.0, 100.0, 1.0e6] {
            let db = power_to_db(p);
            assert!((db_to_power(db) - p).abs() / p < 1e-9, "p={p}");
        }
    }

    #[test]
    fn model_validation_masker_raises_threshold_near_itself_and_decays_away() {
        // A single strong tonal masker at 1kHz: the combined threshold
        // near its own Bark position must sit well above the threshold
        // in quiet there, and decay back toward the quiet threshold
        // several Bark away.
        let masker_hz = 1000.0f64;
        let masker_bark = hz_to_bark(masker_hz);
        let masker_energy_linear = 1.0e6; // a loud tone
        let band_energy = [masker_energy_linear];
        let band_bark = [masker_bark];
        let band_tonal = [true];

        let near_hz = 1050.0f64; // close by
        let far_hz = 4000.0f64; // several critical bands away
        let out_bark = [hz_to_bark(near_hz), hz_to_bark(far_hz)];
        let mut excitation = [0.0f64; 2];
        simultaneous_masking(&band_energy, &band_bark, &band_tonal, &out_bark, &mut excitation);

        let near_threshold = combined_threshold_db(excitation[0], threshold_in_quiet(near_hz));
        let far_threshold = combined_threshold_db(excitation[1], threshold_in_quiet(far_hz));

        assert!(
            near_threshold > threshold_in_quiet(near_hz) + 10.0,
            "a loud nearby masker must raise the threshold well above quiet: {near_threshold} vs {}",
            threshold_in_quiet(near_hz)
        );
        assert!(
            near_threshold > far_threshold,
            "masking must be stronger near the masker than far from it: near={near_threshold} far={far_threshold}"
        );
    }

    #[test]
    fn perceptual_weight_sign_matches_signal_vs_threshold() {
        assert!(perceptual_weight_db(50.0f64, 30.0) > 0.0, "signal above threshold -> positive SMR");
        assert!(perceptual_weight_db(10.0f64, 30.0) < 0.0, "signal below threshold -> negative SMR");
    }
}
