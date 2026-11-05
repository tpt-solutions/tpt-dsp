//! YIN fundamental-frequency (F0) estimator.
//!
//! Implements de Cheveigné & Kawahara's YIN: a squared-difference function,
//! cumulative-mean normalisation (which suppresses the octave errors of plain
//! autocorrelation), an absolute threshold, and parabolic refinement of the lag.
//!
//! The estimator allocates one difference buffer per call, so it is meant for
//! analysis frames, not for the audio callback.

/// Estimate the fundamental frequency of `frame` in Hz.
///
/// * `fmin` / `fmax` — search range in Hz.
/// * `threshold` — absolute threshold on the normalised difference (typically
///   `0.1`–`0.2`). Lower values are stricter about voicing.
///
/// Returns `None` when the frame is too short for the requested range, is
/// silent, or has no lag below `threshold` (unvoiced or noise).
pub fn yin_pitch(
    frame: &[f32],
    sample_rate: f32,
    fmin: f32,
    fmax: f32,
    threshold: f32,
) -> Option<f32> {
    if !(sample_rate.is_finite() && sample_rate > 0.0 && fmin > 0.0 && fmax > fmin) {
        return None;
    }
    let tau_min = ((sample_rate / fmax).floor() as usize).max(2);
    let tau_max = ((sample_rate / fmin).ceil() as usize).min(frame.len() / 2);
    if tau_max <= tau_min + 1 {
        return None;
    }
    let window = frame.len() - tau_max;

    // Difference function d(τ) = Σ_j (x_j − x_{j+τ})² over a fixed window.
    let mut d = vec![0.0f32; tau_max + 1];
    for (tau, slot) in d.iter_mut().enumerate().skip(1) {
        let mut sum = 0.0f32;
        for j in 0..window {
            let diff = frame[j] - frame[j + tau];
            sum += diff * diff;
        }
        *slot = sum;
    }

    // Cumulative-mean-normalised difference d'(τ).
    let mut cmnd = vec![1.0f32; tau_max + 1];
    let mut running = 0.0f32;
    for tau in 1..=tau_max {
        running += d[tau];
        cmnd[tau] = if running > 0.0 {
            d[tau] * tau as f32 / running
        } else {
            1.0
        };
    }

    // First lag under the threshold, slid down to its local minimum.
    let mut tau = (tau_min..tau_max).find(|&t| cmnd[t] < threshold)?;
    while tau + 1 < tau_max && cmnd[tau + 1] < cmnd[tau] {
        tau += 1;
    }

    // Parabolic interpolation around the minimum.
    let (s0, s1, s2) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
    let denom = 2.0 * (2.0 * s1 - s0 - s2);
    let offset = if denom.abs() > f32::EPSILON {
        ((s0 - s2) / denom).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let lag = tau as f32 + offset;
    if lag <= 0.0 {
        return None;
    }
    Some(sample_rate / lag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, sr: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * freq * core::f32::consts::TAU / sr).sin())
            .collect()
    }

    #[test]
    fn estimates_a_pure_tone() {
        let sr = 16_000.0;
        let frame = tone(220.0, sr, 1024);
        let f0 = yin_pitch(&frame, sr, 60.0, 1000.0, 0.15).expect("voiced");
        assert!((f0 - 220.0).abs() / 220.0 < 0.01, "f0 {f0}");
    }

    #[test]
    fn avoids_octave_error_on_a_harmonic_rich_tone() {
        // A sawtooth-like sum of harmonics: plain autocorrelation often
        // reports 2×F0 here; YIN should report the fundamental.
        let sr = 16_000.0;
        let frame: Vec<f32> = (0..2048)
            .map(|i| {
                (1..=8)
                    .map(|h| {
                        (i as f32 * 150.0 * h as f32 * core::f32::consts::TAU / sr).sin() / h as f32
                    })
                    .sum()
            })
            .collect();
        let f0 = yin_pitch(&frame, sr, 60.0, 600.0, 0.15).expect("voiced");
        assert!((f0 - 150.0).abs() / 150.0 < 0.02, "f0 {f0}");
    }

    #[test]
    fn silence_is_unvoiced() {
        assert_eq!(yin_pitch(&[0.0; 1024], 16_000.0, 60.0, 1000.0, 0.15), None);
    }

    #[test]
    fn invalid_arguments_return_none() {
        let frame = tone(220.0, 16_000.0, 1024);
        assert_eq!(yin_pitch(&frame, 0.0, 60.0, 1000.0, 0.15), None);
        assert_eq!(yin_pitch(&frame, 16_000.0, 1000.0, 60.0, 0.15), None);
        assert_eq!(yin_pitch(&frame[..10], 16_000.0, 60.0, 1000.0, 0.15), None);
    }
}
