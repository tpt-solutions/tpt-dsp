//! Feature extraction: RMS energy, zero-crossing rate and spectral centroid.
//!
//! Lightweight, allocation-free scalar features for audio / RF frames. The
//! spectral features operate on a magnitude spectrum produced by an FFT.

/// Root-mean-square energy of a frame.
pub fn rms(input: &[f32]) -> f32 {
    if input.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f32;
    for &x in input {
        sum += x * x;
    }
    (sum / input.len() as f32).sqrt()
}

/// Zero-crossing rate: the fraction of adjacent sample pairs that change sign.
///
/// Returns `0.0` for an empty or single-sample frame.
pub fn zero_crossing_rate(input: &[f32]) -> f32 {
    if input.len() < 2 {
        return 0.0;
    }
    let mut crossings = 0u32;
    for w in input.windows(2) {
        if w[0] == 0.0 || w[1] == 0.0 {
            if w[0] != w[1] {
                crossings += 1;
            }
        } else if (w[0] < 0.0) != (w[1] < 0.0) {
            crossings += 1;
        }
    }
    crossings as f32 / (input.len() - 1) as f32
}

/// Spectral centroid (in bin units) of a magnitude spectrum.
///
/// The centroid is the energy-weighted average bin index; higher values mean
/// "brighter" spectra. Returns `0.0` for an empty or zero spectrum.
pub fn spectral_centroid(magnitude: &[f32]) -> f32 {
    let mut weighted = 0.0f32;
    let mut total = 0.0f32;
    for (i, &m) in magnitude.iter().enumerate() {
        weighted += i as f32 * m;
        total += m;
    }
    if total <= 0.0 {
        0.0
    } else {
        weighted / total
    }
}

/// Spectral centroid expressed as a fraction of the Nyquist bin (`0..1`).
///
/// Divide the bin centroid by `magnitude.len()` so the result is comparable
/// across spectra of different lengths.
pub fn spectral_centroid_normalized(magnitude: &[f32]) -> f32 {
    let n = magnitude.len();
    if n == 0 {
        0.0
    } else {
        spectral_centroid(magnitude) / n as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_unit_sine_is_about_sqrt_half() {
        let sig: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.3).sin()).collect();
        let r = rms(&sig);
        assert!(
            (r - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02,
            "rms {r}"
        );
    }

    #[test]
    fn rms_of_const_is_abs_value() {
        let sig = [2.0f32; 64];
        assert!((rms(&sig) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zero_crossing_rate_of_alternating() {
        let sig = [-1.0f32, 1.0, -1.0, 1.0, -1.0, 1.0];
        assert!((zero_crossing_rate(&sig) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_crossing_rate_of_dc_is_zero() {
        let sig = [0.5f32; 32];
        assert_eq!(zero_crossing_rate(&sig), 0.0);
    }

    #[test]
    fn centroid_of_low_freq_is_low() {
        // A low-frequency sine concentrates energy in the first bins.
        let n = 64;
        let mut mag = vec![0.0f32; n];
        for (i, m) in mag.iter_mut().enumerate() {
            *m = if i < 4 { 1.0 / (i as f32 + 1.0) } else { 0.0 };
        }
        let c = spectral_centroid(&mag);
        assert!(c < 2.0, "centroid {c}");
    }

    #[test]
    fn centroid_of_high_freq_is_high() {
        let n = 64;
        let mut mag = vec![0.0f32; n];
        for (i, m) in mag.iter_mut().enumerate() {
            // Energy concentrated near the top bins.
            *m = if i >= n - 4 { 1.0 } else { 0.0 };
        }
        let c = spectral_centroid(&mag);
        assert!(c > (n as f32 / 2.0), "centroid {c}");
    }
}
