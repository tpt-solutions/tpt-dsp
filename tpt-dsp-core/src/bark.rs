//! Perceptual frequency scales: Bark and ERB (Equivalent Rectangular
//! Bandwidth), plus critical-band mapping and aggregation for a linear
//! (FFT-bin) spectrum.
//!
//! Required primarily by Aura's psychoacoustic model (§10.2), but these
//! are generic frequency-scale utilities, not Aura-specific — any codec
//! or analysis tool that needs a perceptual frequency axis can use them.
//! Allocation-free; generic over `f32`/`f64` via [`num_traits::Float`].

use num_traits::Float;

/// Convert a frequency in Hz to the Bark scale, using the Traunmüller
/// (1990) formula with its standard low/high-frequency corrections:
///
/// ```text
/// z = 26.81*f/(1960+f) - 0.53
/// if z < 2:    z += 0.15*(2 - z)
/// if z > 20.1: z += 0.22*(z - 20.1)
/// ```
///
/// # Panics
///
/// Panics if `freq_hz < 0`.
pub fn hz_to_bark<F: Float>(freq_hz: F) -> F {
    assert!(freq_hz >= F::zero(), "frequency must be non-negative");
    let c1 = F::from(26.81).unwrap();
    let c2 = F::from(1960.0).unwrap();
    let c3 = F::from(0.53).unwrap();
    let mut z = c1 * freq_hz / (c2 + freq_hz) - c3;
    let low = F::from(2.0).unwrap();
    let high = F::from(20.1).unwrap();
    if z < low {
        z = z + F::from(0.15).unwrap() * (low - z);
    } else if z > high {
        z = z + F::from(0.22).unwrap() * (z - high);
    }
    z
}

/// Inverse of [`hz_to_bark`]: recover a frequency in Hz from a Bark value,
/// via bisection (the corrected Traunmüller formula has no closed-form
/// inverse, but [`hz_to_bark`] is monotonically increasing, so bisection
/// converges reliably).
///
/// # Panics
///
/// Panics if `bark < 0`.
pub fn bark_to_hz<F: Float>(bark: F) -> F {
    assert!(bark >= F::zero(), "bark value must be non-negative");
    let mut lo = F::zero();
    let mut hi = F::from(30_000.0).unwrap(); // comfortably above audible range
    for _ in 0..60 {
        let mid = (lo + hi) / F::from(2).unwrap();
        if hz_to_bark(mid) < bark {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / F::from(2).unwrap()
}

/// Equivalent Rectangular Bandwidth (Glasberg & Moore, 1990) of the
/// auditory filter centered at `freq_hz`, in Hz: `24.7*(4.37*f/1000 + 1)`.
///
/// # Panics
///
/// Panics if `freq_hz < 0`.
pub fn erb_bandwidth<F: Float>(freq_hz: F) -> F {
    assert!(freq_hz >= F::zero(), "frequency must be non-negative");
    F::from(24.7).unwrap() * (F::from(4.37 / 1000.0).unwrap() * freq_hz + F::one())
}

/// Convert a frequency in Hz to the ERB-rate scale (Glasberg & Moore):
/// `21.4*log10(4.37*f/1000 + 1)`.
///
/// # Panics
///
/// Panics if `freq_hz < 0`.
pub fn hz_to_erb_rate<F: Float>(freq_hz: F) -> F {
    assert!(freq_hz >= F::zero(), "frequency must be non-negative");
    F::from(21.4).unwrap() * (F::from(4.37 / 1000.0).unwrap() * freq_hz + F::one()).log10()
}

/// Inverse of [`hz_to_erb_rate`] (closed-form, unlike [`bark_to_hz`] —
/// the ERB-rate formula's `log10` is directly invertible):
/// `f = 1000*(10^(rate/21.4) - 1)/4.37`.
///
/// # Panics
///
/// Panics if `erb_rate < 0`.
pub fn erb_rate_to_hz<F: Float>(erb_rate: F) -> F {
    assert!(erb_rate >= F::zero(), "ERB-rate value must be non-negative");
    let ten = F::from(10.0).unwrap();
    let exponent = erb_rate / F::from(21.4).unwrap();
    F::from(1000.0 / 4.37).unwrap() * (ten.powf(exponent) - F::one())
}

/// For each FFT bin `k` of an `fft_size`-point real spectrum sampled at
/// `sample_rate` Hz, write its Bark critical-band index (`floor(hz_to_bark(f_k))`,
/// clamped to `>= 0`) to `out[k]`. `out.len()` must equal
/// `fft_size / 2 + 1` (the number of non-redundant bins of a real FFT).
///
/// # Panics
///
/// Panics if `fft_size == 0` or `out.len() != fft_size/2 + 1`.
pub fn fft_bin_bark_bands<F: Float>(sample_rate: F, fft_size: usize, out: &mut [usize]) {
    assert!(fft_size > 0, "fft_size must be positive");
    let bins = fft_size / 2 + 1;
    assert_eq!(out.len(), bins, "out length must equal fft_size/2 + 1");
    for (k, slot) in out.iter_mut().enumerate() {
        let freq = F::from(k).unwrap() * sample_rate / F::from(fft_size).unwrap();
        let z = hz_to_bark(freq);
        *slot = if z <= F::zero() { 0 } else { z.to_usize().unwrap_or(usize::MAX) };
    }
}

/// Sum `spectrum[k]` into `out[band_of_bin[k]]` for every bin — critical
/// band energy aggregation. `out` is not zeroed first (so callers can
/// accumulate across multiple calls); zero it yourself for a fresh
/// aggregation. `band_of_bin.len()` must equal `spectrum.len()`, and every
/// entry of `band_of_bin` must be `< out.len()`.
///
/// # Panics
///
/// Panics if `band_of_bin.len() != spectrum.len()`, or any
/// `band_of_bin[k] >= out.len()`.
pub fn aggregate_bands<F: Float>(spectrum: &[F], band_of_bin: &[usize], out: &mut [F]) {
    assert_eq!(band_of_bin.len(), spectrum.len(), "band_of_bin must have one entry per spectrum bin");
    for (&value, &band) in spectrum.iter().zip(band_of_bin.iter()) {
        assert!(band < out.len(), "band index out of range");
        out[band] = out[band] + value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hz_to_bark_matches_known_reference_points() {
        // Widely-cited reference points for the Bark scale (e.g. Zwicker &
        // Fastl's critical-band table): 1kHz ~ 8.5 Bark, 5kHz ~ 19 Bark.
        assert!((hz_to_bark(1000.0f64) - 8.5).abs() < 0.5, "{}", hz_to_bark(1000.0f64));
        assert!((hz_to_bark(5000.0f64) - 19.0).abs() < 1.0, "{}", hz_to_bark(5000.0f64));
    }

    #[test]
    fn hz_to_bark_is_monotonically_increasing() {
        let mut prev = hz_to_bark(0.0f64);
        for hz in (1..24000).step_by(50) {
            let z = hz_to_bark(hz as f64);
            assert!(z > prev, "not monotonic at {hz}Hz: {z} vs prev {prev}");
            prev = z;
        }
    }

    #[test]
    fn bark_hz_round_trip() {
        for &hz in &[100.0f64, 500.0, 1000.0, 4000.0, 12000.0] {
            let z = hz_to_bark(hz);
            let back = bark_to_hz(z);
            assert!((back - hz).abs() < 1.0, "hz={hz}: round trip gave {back}");
        }
    }

    #[test]
    fn erb_rate_hz_round_trip_is_exact_closed_form() {
        for &hz in &[100.0f64, 500.0, 1000.0, 4000.0, 12000.0] {
            let rate = hz_to_erb_rate(hz);
            let back = erb_rate_to_hz(rate);
            assert!((back - hz).abs() < 1e-6, "hz={hz}: round trip gave {back}");
        }
    }

    #[test]
    fn erb_bandwidth_increases_with_frequency() {
        assert!(erb_bandwidth(100.0f64) < erb_bandwidth(1000.0f64));
        assert!(erb_bandwidth(1000.0f64) < erb_bandwidth(8000.0f64));
    }

    #[test]
    fn fft_bin_bark_bands_are_nondecreasing() {
        let sample_rate = 48_000.0f64;
        let fft_size = 1024;
        let mut bands = vec![0usize; fft_size / 2 + 1];
        fft_bin_bark_bands(sample_rate, fft_size, &mut bands);
        assert_eq!(bands[0], 0);
        for w in bands.windows(2) {
            assert!(w[1] >= w[0], "band index must be non-decreasing with frequency: {w:?}");
        }
        // Nyquist (24kHz) should land near the top of the Bark scale (~24).
        assert!(*bands.last().unwrap() >= 20);
    }

    #[test]
    fn aggregate_bands_sums_into_correct_band() {
        let spectrum = [1.0f64, 2.0, 3.0, 4.0];
        let band_of_bin = [0usize, 0, 1, 2];
        let mut out = [0.0f64; 3];
        aggregate_bands(&spectrum, &band_of_bin, &mut out);
        assert_eq!(out, [3.0, 3.0, 4.0]);
    }
}
