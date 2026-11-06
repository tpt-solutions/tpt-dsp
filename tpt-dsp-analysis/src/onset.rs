//! Onset detection: find the starts of notes and transients.
//!
//! The detector works on a spectral-flux curve, one value per STFT frame. A
//! frame is an onset when it is the largest value within a short window, it
//! rises above the local mean by `delta`, and it is at least `wait` frames
//! after the previous onset. This is the usual peak-picking scheme, with the
//! defaults chosen for frame-level use.
//!
//! [`pick_onsets`] works on any flux curve. [`detect_onsets`] runs the whole
//! path from samples, and allocates its STFT, so it is for offline analysis.

use crate::features::spectral_flux;
use crate::stft::stft;

/// Peak-picking parameters, in frames of the flux curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnsetConfig {
    /// Frames before the candidate that must all be lower than it.
    pub pre_max: usize,
    /// Frames after the candidate that must all be lower than it.
    pub post_max: usize,
    /// Frames before the candidate that form the local mean.
    pub pre_avg: usize,
    /// Frames after the candidate that form the local mean.
    pub post_avg: usize,
    /// How far above the local mean the candidate must rise, in flux units.
    pub delta: f32,
    /// An onset must come more than `wait` frames after the previous one.
    pub wait: usize,
}

impl Default for OnsetConfig {
    fn default() -> Self {
        Self {
            pre_max: 3,
            post_max: 1,
            pre_avg: 10,
            post_avg: 10,
            delta: 0.1,
            wait: 3,
        }
    }
}

/// Pick onset frames from a flux curve.
///
/// Returns the indices into `flux` of the onset frames, in increasing order.
/// Non-finite flux values are treated as zero.
pub fn pick_onsets(flux: &[f32], config: &OnsetConfig) -> Vec<usize> {
    let len = flux.len();
    let value = |i: usize| {
        let v = flux[i];
        if v.is_finite() {
            v
        } else {
            0.0
        }
    };
    let mut onsets = Vec::new();
    let mut last: Option<usize> = None;
    for n in 0..len {
        let v = value(n);
        let max_lo = n.saturating_sub(config.pre_max);
        let max_hi = (n + config.post_max + 1).min(len);
        let is_peak = (max_lo..max_hi).all(|i| value(i) <= v);
        if !is_peak {
            continue;
        }
        let avg_lo = n.saturating_sub(config.pre_avg);
        let avg_hi = (n + config.post_avg + 1).min(len);
        let sum: f32 = (avg_lo..avg_hi).map(value).sum();
        let mean = sum / (avg_hi - avg_lo) as f32;
        if v < mean + config.delta {
            continue;
        }
        if let Some(prev) = last {
            if n - prev <= config.wait {
                continue;
            }
        }
        onsets.push(n);
        last = Some(n);
    }
    onsets
}

/// Detect onsets in `input`, returning their sample positions.
///
/// The flux of frame `k` is the half-wave-rectified magnitude increase from
/// frame `k-1` (frame 0 has no predecessor, so it has zero flux). Each onset is
/// reported at the start of its STFT frame, `frame * hop`.
///
/// # Panics
///
/// Panics under the same conditions as [`stft`](crate::stft).
pub fn detect_onsets(
    input: &[f32],
    fft_size: usize,
    hop: usize,
    config: &OnsetConfig,
) -> Vec<usize> {
    let mut frames = stft(input, fft_size, hop);
    // Keep only frames that lie entirely inside the input. The STFT pads the
    // end with zeros, and a partial frame would show up as a false onset.
    let full = if input.len() >= fft_size {
        (input.len() - fft_size) / hop + 1
    } else {
        0
    };
    frames.truncate(full);
    let magnitudes: Vec<Vec<f32>> = frames
        .iter()
        .map(|frame| frame.iter().map(|c| c.norm()).collect())
        .collect();
    let mut flux = vec![0.0f32; magnitudes.len()];
    for k in 1..magnitudes.len() {
        flux[k] = spectral_flux(&magnitudes[k - 1], &magnitudes[k]);
    }
    pick_onsets(&flux, config)
        .into_iter()
        .map(|frame| frame * hop)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flux_with_spikes(len: usize, spikes: &[usize], height: f32) -> Vec<f32> {
        let mut flux = vec![0.01f32; len];
        for (i, v) in flux.iter_mut().enumerate() {
            // A small deterministic ripple so the mean is not flat.
            *v += 0.005 * ((i as f32) * 0.7).sin().abs();
        }
        for &s in spikes {
            flux[s] = height;
        }
        flux
    }

    #[test]
    fn isolated_spikes_are_picked_exactly() {
        let spikes = [20, 60, 110];
        let flux = flux_with_spikes(150, &spikes, 1.0);
        assert_eq!(pick_onsets(&flux, &OnsetConfig::default()), spikes.to_vec());
    }

    #[test]
    fn small_bumps_below_delta_are_ignored() {
        let flux = flux_with_spikes(100, &[50], 0.05);
        let config = OnsetConfig {
            delta: 0.1,
            ..OnsetConfig::default()
        };
        assert!(pick_onsets(&flux, &config).is_empty());
    }

    #[test]
    fn wait_suppresses_double_triggers() {
        let mut flux = flux_with_spikes(60, &[], 0.0);
        // A peak at 20 and a larger one two frames later: only the first
        // survives the wait, because the second is inside its window.
        flux[20] = 0.8;
        flux[22] = 1.0;
        let config = OnsetConfig {
            pre_max: 0,
            post_max: 0,
            wait: 3,
            ..OnsetConfig::default()
        };
        assert_eq!(pick_onsets(&flux, &config), vec![20]);
    }

    #[test]
    fn empty_and_non_finite_input_is_safe() {
        assert!(pick_onsets(&[], &OnsetConfig::default()).is_empty());
        let flux = [f32::NAN, f32::INFINITY, 0.0];
        assert!(pick_onsets(&flux, &OnsetConfig::default()).is_empty());
    }

    #[test]
    fn click_train_is_detected_at_its_sample_positions() {
        let sr = 16_000usize;
        let mut x = vec![0.0f32; sr];
        let clicks = [4000usize, 8000, 12000];
        for &c in &clicks {
            for i in 0..64 {
                x[c + i] = 0.9 * (-(i as f32) / 8.0).exp();
            }
        }
        let found = detect_onsets(&x, 256, 128, &OnsetConfig::default());
        assert_eq!(found.len(), clicks.len(), "found {found:?}");
        for (&f, &c) in found.iter().zip(&clicks) {
            // The flux rises at the frame containing the click, which starts
            // at most one hop before it.
            assert!(f <= c && c - f < 256, "onset {f} for click {c}");
        }
    }

    #[test]
    fn steady_tone_has_no_onsets_after_the_first_frame() {
        let sr = 16_000.0f32;
        let x: Vec<f32> = (0..sr as usize)
            .map(|i| (i as f32 * 440.0 * core::f32::consts::TAU / sr).sin())
            .collect();
        let found = detect_onsets(&x, 512, 256, &OnsetConfig::default());
        assert!(found.is_empty(), "steady tone gave onsets {found:?}");
    }
}
