//! Integrated loudness (ITU-R BS.1770, mono).
//!
//! The signal passes a two-stage K-weighting filter (a high shelf followed by
//! a high-pass). Mean-square energy is taken over 400 ms blocks overlapping by
//! 75%, and the integrated loudness applies the absolute (−70 LUFS) and
//! relative (−10 LU) gates from the standard.
//!
//! The K-weighting uses the BS.1770 coefficient set (the bilinear form with
//! the standard's corner frequencies and Q values). It is evaluated in `f64`.
//! The design is checked against pyloudnorm's `"DeMan"` class in
//! `tests/golden_lufs.rs`: steady tones agree to 0.001 LU, and a mixed-level
//! programme agrees to about 0.01 LU, where a block sits on a gate edge.
//! pyloudnorm's default `"K-weighting"` class is a different, older design
//! (1500 Hz, 4 dB shelf); it is not the standard, and is not what this meter
//! implements.
//!
//! This is a measurement tool, not a certified meter. Check it against the
//! reference before any compliance use.

/// Direct-form-II-transposed second-order section in `f64`.
#[derive(Debug, Clone, Copy)]
struct Section {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Section {
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Stage 1 of the K-weighting: the high shelf (+3.99984 dB at 1681.97 Hz,
/// Q 0.7071752). The bilinear form uses `Vb = Vh^0.4996…`.
fn k_shelf(fs: f64) -> Section {
    const G_DB: f64 = 3.999_843_853_973_347;
    const Q: f64 = 0.707_175_236_955_419_3;
    const FC: f64 = 1_681.974_450_955_533;
    let k = (core::f64::consts::PI * FC / fs).tan();
    let vh = 10.0f64.powf(G_DB / 20.0);
    let vb = vh.powf(0.499_666_774_155);
    let a0 = 1.0 + k / Q + k * k;
    Section {
        b0: (vh + vb * k / Q + k * k) / a0,
        b1: 2.0 * (k * k - vh) / a0,
        b2: (vh - vb * k / Q + k * k) / a0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / Q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

/// Stage 2 of the K-weighting: the high-pass at 38.135 Hz (Q 0.5003270).
fn k_highpass(fs: f64) -> Section {
    const Q: f64 = 0.500_327_037_325_395_3;
    const FC: f64 = 38.135_470_876_139_82;
    let k = (core::f64::consts::PI * FC / fs).tan();
    let a0 = 1.0 + k / Q + k * k;
    Section {
        b0: 1.0,
        b1: -2.0,
        b2: 1.0,
        a1: 2.0 * (k * k - 1.0) / a0,
        a2: (1.0 - k / Q + k * k) / a0,
        z1: 0.0,
        z2: 0.0,
    }
}

const BLOCK_SECONDS: f32 = 0.4;
const HOP_FRACTION: f32 = 0.25;
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;

/// Mono integrated-loudness meter.
pub struct LoudnessMeter {
    stage_shelf: Section,
    stage_hp: Section,
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
        let block_len = (BLOCK_SECONDS * sr).round() as usize;
        Self {
            stage_shelf: k_shelf(f64::from(sr)),
            stage_hp: k_highpass(f64::from(sr)),
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
            let x = if x.is_finite() { f64::from(x) } else { 0.0 };
            let k = self.stage_hp.tick(self.stage_shelf.tick(x)) as f32;
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
