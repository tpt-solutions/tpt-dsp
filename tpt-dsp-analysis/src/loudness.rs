//! Integrated loudness (ITU-R BS.1770 style, mono).
//!
//! The signal passes a two-stage K-weighting filter (a high shelf followed by
//! a high-pass). Mean-square energy is taken over 400 ms blocks overlapping by
//! 75%, and the integrated loudness applies the absolute (−70 LUFS) and
//! relative (−10 LU) gates from the standard.
//!
//! **Calibration caveat:** the K-weighting stages are RBJ-cookbook designs at
//! the standard's corner frequencies, not the reference coefficients from the
//! standard. Readings are consistent with each other and suitable for
//! comparison, but verify against the reference before a compliance use.

use tpt_dsp_core::{Biquad, BiquadCoeffs, BiquadType};

const BLOCK_SECONDS: f32 = 0.4;
const HOP_FRACTION: f32 = 0.25;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;

/// Mono integrated-loudness meter.
pub struct LoudnessMeter {
    stage_shelf: Biquad<f32>,
    stage_hp: Biquad<f32>,
    block_len: usize,
    hop: usize,
    /// Ring of the most recent K-weighted samples (length `block_len`).
    ring: Vec<f32>,
    write: usize,
    /// Samples until the next block is due.
    until_next: usize,
    filled: usize,
    /// Mean-square of each completed 400 ms block.
    block_powers: Vec<f64>,
}

impl LoudnessMeter {
    /// Create a meter for `sample_rate` Hz.
    pub fn new(sample_rate: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        // Stage 1: high shelf, +4 dB at 1681.97 Hz.
        let shelf = BiquadCoeffs::design(
            BiquadType::HighShelf,
            sr,
            1681.97,
            core::f32::consts::FRAC_1_SQRT_2,
            4.0,
        );
        // Stage 2: high-pass at 38.13 Hz, Q ≈ 0.5.
        let hp = BiquadCoeffs::design(BiquadType::HighPass, sr, 38.13, 0.5003, 0.0);
        let block_len = (BLOCK_SECONDS * sr).round() as usize;
        Self {
            stage_shelf: Biquad::from_coeffs(shelf),
            stage_hp: Biquad::from_coeffs(hp),
            block_len,
            hop: ((block_len as f32) * HOP_FRACTION).round().max(1.0) as usize,
            ring: vec![0.0; block_len],
            write: 0,
            until_next: block_len,
            filled: 0,
            block_powers: Vec::new(),
        }
    }

    /// Feed samples. Each completed 400 ms block contributes one power value.
    pub fn push(&mut self, samples: &[f32]) {
        for &x in samples {
            let x = if x.is_finite() { x } else { 0.0 };
            let k = self.stage_hp.tick(self.stage_shelf.tick(x));
            self.ring[self.write] = k;
            self.write = (self.write + 1) % self.block_len;
            self.filled = (self.filled + 1).min(self.block_len);
            if self.filled == self.block_len {
                self.until_next -= 1;
                if self.until_next == 0 {
                    self.until_next = self.hop;
                    let energy: f64 = self.ring.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
                    self.block_powers.push(energy / self.block_len as f64);
                }
            }
        }
    }

    /// Integrated loudness in LUFS, or `None` if no block passes the gates.
    pub fn integrated(&self) -> Option<f64> {
        let loud = |p: f64| -0.691 + 10.0 * p.max(1e-20).log10();
        let above_abs: Vec<f64> = self
            .block_powers
            .iter()
            .copied()
            .filter(|&p| loud(p) > ABSOLUTE_GATE_LUFS)
            .collect();
        if above_abs.is_empty() {
            return None;
        }
        let mean: f64 = above_abs.iter().sum::<f64>() / above_abs.len() as f64;
        let relative_gate = loud(mean) + RELATIVE_GATE_LU;
        let gated: Vec<f64> = above_abs
            .into_iter()
            .filter(|&p| loud(p) > relative_gate)
            .collect();
        if gated.is_empty() {
            return None;
        }
        let mean_gated: f64 = gated.iter().sum::<f64>() / gated.len() as f64;
        Some(loud(mean_gated))
    }

    /// Clear all state.
    pub fn reset(&mut self) {
        self.stage_shelf.reset();
        self.stage_hp.reset();
        self.ring.iter_mut().for_each(|v| *v = 0.0);
        self.write = 0;
        self.until_next = self.block_len;
        self.filled = 0;
        self.block_powers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn sine(amp: f32, secs: f32) -> Vec<f32> {
        (0..(SR * secs) as usize)
            .map(|i| amp * (i as f32 * 1000.0 * core::f32::consts::TAU / SR).sin())
            .collect()
    }

    #[test]
    fn silence_has_no_reading() {
        let mut m = LoudnessMeter::new(SR);
        m.push(&vec![0.0; (SR * 2.0) as usize]);
        assert_eq!(m.integrated(), None);
    }

    #[test]
    fn doubling_amplitude_raises_loudness_by_about_six_db() {
        let read = |amp: f32| {
            let mut m = LoudnessMeter::new(SR);
            m.push(&sine(amp, 3.0));
            m.integrated().expect("gated in")
        };
        let quiet = read(0.1);
        let loud = read(0.2);
        assert!((loud - quiet - 6.02).abs() < 0.1, "delta {}", loud - quiet);
    }

    #[test]
    fn relative_gate_drops_a_quiet_passage() {
        // A loud section followed by a passage 30 dB quieter: the quiet part
        // is below the relative gate and must not drag the reading down.
        let mut mixed = sine(0.2, 3.0);
        mixed.extend(sine(0.2 * 0.0316, 3.0));
        let mut m = LoudnessMeter::new(SR);
        m.push(&mixed);
        let mut loud_only = LoudnessMeter::new(SR);
        loud_only.push(&sine(0.2, 3.0));
        let a = m.integrated().unwrap();
        let b = loud_only.integrated().unwrap();
        assert!((a - b).abs() < 0.5, "gated {a} vs loud-only {b}");
    }

    #[test]
    fn invalid_sample_rate_falls_back() {
        let m = LoudnessMeter::new(f32::NAN);
        assert_eq!(m.block_len, 19_200);
    }
}
