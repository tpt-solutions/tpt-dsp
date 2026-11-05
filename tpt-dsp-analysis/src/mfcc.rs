//! Mel-frequency cepstral coefficients (MFCC).
//!
//! Pipeline per frame: Hann window → power spectrum → triangular mel
//! filterbank → log → DCT-II. The first few coefficients describe the spectral
//! envelope and are a standard feature for speech and sound classification.
//!
//! [`Mfcc`] owns its FFT plan, window and filterbank, so [`Mfcc::compute`]
//! does not allocate.

use tpt_dsp_core::{dct_ii, windowed, FftPlan, WindowType, C32};

/// Mel scale: Hz → mel (O'Shaughnessy formula).
pub fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Mel scale: mel → Hz (inverse of [`hz_to_mel`]).
pub fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10.0f32.powf(mel / 2595.0) - 1.0)
}

/// MFCC extractor for a fixed frame size and filterbank.
pub struct Mfcc {
    fft: FftPlan,
    window: Vec<f32>,
    buf: Vec<C32>,
    /// Dense triangular filter weights, one row per mel band over the
    /// `fft_size/2 + 1` one-sided bins.
    filters: Vec<Vec<f32>>,
    log_mels: Vec<f32>,
    n_coeffs: usize,
}

impl Mfcc {
    /// Build an extractor.
    ///
    /// * `fft_size` — power of two; frames shorter than this are zero-padded.
    /// * `n_mels` — number of mel filters (≥ 1).
    /// * `n_coeffs` — coefficients to return (`1..=n_mels`).
    ///
    /// # Panics
    ///
    /// Panics if `fft_size` is not a power of two ≥ 4, or if the counts are
    /// out of range.
    pub fn new(sample_rate: f32, fft_size: usize, n_mels: usize, n_coeffs: usize) -> Self {
        assert!(
            fft_size >= 4 && fft_size.is_power_of_two(),
            "fft_size must be a power of two"
        );
        assert!(n_mels >= 1, "need at least one mel band");
        assert!(
            n_coeffs >= 1 && n_coeffs <= n_mels,
            "n_coeffs must be in 1..=n_mels"
        );

        let bins = fft_size / 2 + 1;
        let nyquist = sample_rate / 2.0;
        let mel_max = hz_to_mel(nyquist);
        // n_mels + 2 edge points: each filter spans three consecutive points.
        let edges_hz: Vec<f32> = (0..n_mels + 2)
            .map(|i| mel_to_hz(mel_max * i as f32 / (n_mels + 1) as f32))
            .collect();
        let bin_hz = sample_rate / fft_size as f32;
        let filters = (0..n_mels)
            .map(|m| {
                let (lo, mid, hi) = (edges_hz[m], edges_hz[m + 1], edges_hz[m + 2]);
                (0..bins)
                    .map(|k| {
                        let f = k as f32 * bin_hz;
                        if f <= lo || f >= hi {
                            0.0
                        } else if f <= mid {
                            (f - lo) / (mid - lo).max(1e-9)
                        } else {
                            (hi - f) / (hi - mid).max(1e-9)
                        }
                    })
                    .collect()
            })
            .collect();

        let mut window = vec![0.0f32; fft_size];
        windowed(WindowType::Hann, fft_size, &mut window);
        Self {
            fft: FftPlan::new_forward(fft_size),
            window,
            buf: vec![C32::new(0.0, 0.0); fft_size],
            filters,
            log_mels: vec![0.0; n_mels],
            n_coeffs,
        }
    }

    /// Number of coefficients returned by [`compute`](Self::compute).
    pub fn n_coeffs(&self) -> usize {
        self.n_coeffs
    }

    /// Compute the cepstral coefficients of one frame into `out`.
    ///
    /// # Panics
    ///
    /// Panics if `out.len() < n_coeffs`.
    pub fn compute(&mut self, frame: &[f32], out: &mut [f32]) {
        assert!(out.len() >= self.n_coeffs, "output shorter than n_coeffs");
        let n = self.window.len();
        for (i, slot) in self.buf.iter_mut().enumerate() {
            let x = if i < frame.len() && i < n {
                frame[i] * self.window[i]
            } else {
                0.0
            };
            *slot = C32::new(if x.is_finite() { x } else { 0.0 }, 0.0);
        }
        self.fft.process_inplace(&mut self.buf);

        for (m, filt) in self.filters.iter().enumerate() {
            let mut energy = 0.0f32;
            for (k, &w) in filt.iter().enumerate() {
                if w != 0.0 {
                    energy += w * self.buf[k].norm_sqr();
                }
            }
            self.log_mels[m] = energy.max(1e-10).ln();
        }

        // DCT-II of the log mel energies; the first `n_coeffs` are the MFCCs.
        let mut dct = vec![0.0f32; self.log_mels.len()];
        dct_ii(&self.log_mels, &mut dct);
        out[..self.n_coeffs].copy_from_slice(&dct[..self.n_coeffs]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 16_000.0;

    fn tone(f: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * f * core::f32::consts::TAU / SR).sin())
            .collect()
    }

    #[test]
    fn mel_scale_round_trips() {
        for hz in [0.0f32, 100.0, 1000.0, 7999.0] {
            assert!((mel_to_hz(hz_to_mel(hz)) - hz).abs() < 0.01);
        }
    }

    #[test]
    fn coefficients_are_finite_and_distinguish_tones() {
        let mut m = Mfcc::new(SR, 512, 26, 13);
        let mut a = [0.0f32; 13];
        let mut b = [0.0f32; 13];
        m.compute(&tone(300.0, 512), &mut a);
        m.compute(&tone(3000.0, 512), &mut b);
        assert!(a.iter().chain(b.iter()).all(|x| x.is_finite()));
        let diff: f32 = a.iter().zip(b.iter()).map(|(x, y)| (x - y).abs()).sum();
        assert!(diff > 1.0, "tones indistinguishable: {diff}");
    }

    #[test]
    fn silence_gives_finite_coefficients() {
        let mut m = Mfcc::new(SR, 256, 20, 12);
        let mut out = [0.0f32; 12];
        m.compute(&[0.0; 256], &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn first_coefficient_tracks_loudness() {
        let mut m = Mfcc::new(SR, 512, 26, 13);
        let mut quiet = [0.0f32; 13];
        let mut loud = [0.0f32; 13];
        m.compute(
            &tone(500.0, 512).iter().map(|x| x * 0.1).collect::<Vec<_>>(),
            &mut quiet,
        );
        m.compute(&tone(500.0, 512), &mut loud);
        assert!(loud[0] > quiet[0] + 3.0, "{} vs {}", loud[0], quiet[0]);
    }
}
