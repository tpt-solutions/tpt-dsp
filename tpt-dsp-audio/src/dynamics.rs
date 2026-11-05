//! Dynamics processors: feed-forward compressor, limiter, and noise gate.
//!
//! All three share one structure: a peak level detector (a dB-domain envelope
//! follower with separate attack and release), a static gain curve, and a
//! smoothed gain applied to the input. State is fixed at construction, so
//! processing is allocation-free.
//!
//! The limiter has **no lookahead** and an instant attack: each sample's gain
//! is set from that sample's own level, so the output never exceeds the
//! ceiling, but the gain can step abruptly at a transient (audible as a click on
//! hard material). A lookahead buffer in front of it would smooth this.

use crate::util::{is_valid_sample_rate, sanitize_time};

/// Level floor in dB; anything quieter is treated as this level.
const FLOOR_DB: f32 = -120.0;

fn level_db(x: f32) -> f32 {
    (20.0 * x.abs().max(1.0e-9).log10()).max(FLOOR_DB)
}

fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

/// One-pole smoothing coefficient for a time constant `tau` (seconds).
fn smoothing(tau: f32, sample_rate: f32) -> f32 {
    if tau <= 0.0 {
        0.0
    } else {
        (-1.0 / (tau * sample_rate)).exp()
    }
}

/// Feed-forward downward compressor with makeup gain.
#[derive(Debug, Clone)]
pub struct Compressor {
    sample_rate: f32,
    threshold_db: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    makeup_db: f32,
    env_db: f32,
    gain_reduction_db: f32,
}

impl Compressor {
    /// Create a compressor.
    ///
    /// * `threshold_db` — level above which gain reduction starts (dBFS).
    /// * `ratio` — input-over-output slope above threshold (`>= 1`; non-finite
    ///   values become `1`, i.e. no compression).
    /// * `attack_s` / `release_s` — envelope time constants in seconds.
    ///
    /// A non-finite or non-positive `sample_rate` falls back to 48 kHz.
    pub fn new(
        sample_rate: f32,
        threshold_db: f32,
        ratio: f32,
        attack_s: f32,
        release_s: f32,
    ) -> Self {
        let sample_rate = if is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            48_000.0
        };
        let ratio = if ratio.is_finite() {
            ratio.max(1.0)
        } else {
            1.0
        };
        Self {
            sample_rate,
            threshold_db: if threshold_db.is_finite() {
                threshold_db
            } else {
                0.0
            },
            ratio,
            attack_coeff: smoothing(sanitize_time(attack_s), sample_rate),
            release_coeff: smoothing(sanitize_time(release_s), sample_rate),
            makeup_db: 0.0,
            env_db: FLOOR_DB,
            gain_reduction_db: 0.0,
        }
    }

    /// Set makeup gain in dB (applied after reduction). Non-finite values are
    /// ignored.
    pub fn set_makeup_gain_db(&mut self, db: f32) {
        if db.is_finite() {
            self.makeup_db = db;
        }
    }

    /// The sample rate in Hz this processor was built with. The smoothing
    /// coefficients are fixed at construction, so build a new processor to
    /// change the rate.
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Current gain reduction in dB (positive means the signal is being
    /// reduced). Useful for a meter.
    pub fn gain_reduction_db(&self) -> f32 {
        self.gain_reduction_db
    }

    /// Reset the detector to silence.
    pub fn reset(&mut self) {
        self.env_db = FLOOR_DB;
        self.gain_reduction_db = 0.0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let level = level_db(x);
        let coeff = if level > self.env_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.env_db = level + coeff * (self.env_db - level);

        let over = self.env_db - self.threshold_db;
        let reduction = if over > 0.0 {
            over * (1.0 - 1.0 / self.ratio)
        } else {
            0.0
        };
        self.gain_reduction_db = reduction;
        x * db_to_gain(self.makeup_db - reduction)
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

/// Peak limiter: a compressor with infinite ratio that holds the output at or
/// below `ceiling_db`. Has no lookahead (see the module docs).
#[derive(Debug, Clone)]
pub struct Limiter {
    inner: Compressor,
    ceiling_db: f32,
}

impl Limiter {
    /// Create a limiter with the given output ceiling (dBFS) and release time.
    /// The attack is instant (see the module docs).
    pub fn new(sample_rate: f32, ceiling_db: f32, release_s: f32) -> Self {
        let ceiling_db = if ceiling_db.is_finite() {
            ceiling_db
        } else {
            0.0
        };
        let mut inner = Compressor::new(sample_rate, ceiling_db, f32::INFINITY, 0.0, release_s);
        inner.ratio = f32::INFINITY;
        Self { inner, ceiling_db }
    }

    /// Output ceiling in dBFS.
    pub fn ceiling_db(&self) -> f32 {
        self.ceiling_db
    }

    /// Current gain reduction in dB.
    pub fn gain_reduction_db(&self) -> f32 {
        self.inner.gain_reduction_db()
    }

    /// Reset the detector.
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        self.inner.tick(x)
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        self.inner.process(buf);
    }
}

/// Downward noise gate (expander below threshold).
///
/// Signals below `threshold_db` are attenuated by `(threshold − level) ·
/// (ratio − 1)` dB, so the attenuation deepens smoothly rather than switching
/// off abruptly.
#[derive(Debug, Clone)]
pub struct Gate {
    threshold_db: f32,
    ratio: f32,
    attack_coeff: f32,
    release_coeff: f32,
    env_db: f32,
    gain_db: f32,
}

impl Gate {
    /// Create a gate. `ratio` is the expansion slope (`>= 1`).
    pub fn new(
        sample_rate: f32,
        threshold_db: f32,
        ratio: f32,
        attack_s: f32,
        release_s: f32,
    ) -> Self {
        let sample_rate = if is_valid_sample_rate(sample_rate) {
            sample_rate
        } else {
            48_000.0
        };
        let ratio = if ratio.is_finite() {
            ratio.max(1.0)
        } else {
            1.0
        };
        Self {
            threshold_db: if threshold_db.is_finite() {
                threshold_db
            } else {
                FLOOR_DB
            },
            ratio,
            attack_coeff: smoothing(sanitize_time(attack_s), sample_rate),
            release_coeff: smoothing(sanitize_time(release_s), sample_rate),
            env_db: FLOOR_DB,
            gain_db: 0.0,
        }
    }

    /// Current applied gain in dB (`0` when open, negative when closed).
    pub fn gain_db(&self) -> f32 {
        self.gain_db
    }

    /// Reset the detector.
    pub fn reset(&mut self) {
        self.env_db = FLOOR_DB;
        self.gain_db = 0.0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let level = level_db(x);
        let coeff = if level > self.env_db {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.env_db = level + coeff * (self.env_db - level);

        let below = self.env_db - self.threshold_db;
        self.gain_db = if below < 0.0 {
            below * (self.ratio - 1.0)
        } else {
            0.0
        };
        x * db_to_gain(self.gain_db)
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn steady_peak(mut f: impl FnMut(f32) -> f32, amp: f32) -> f32 {
        let mut peak = 0.0f32;
        for i in 0..SR as usize {
            let x = amp * (i as f32 * 1000.0 * core::f32::consts::TAU / SR).sin();
            let y = f(x);
            if i > SR as usize / 2 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn compressor_leaves_quiet_signal_alone() {
        let mut c = Compressor::new(SR, -10.0, 4.0, 0.005, 0.1);
        // 0.1 amplitude is about -20 dBFS, below the -10 dB threshold.
        let peak = steady_peak(|x| c.tick(x), 0.1);
        assert!((peak - 0.1).abs() < 0.005, "peak {peak}");
    }

    #[test]
    fn compressor_reduces_loud_signal_by_the_ratio() {
        // Amplitude 1.0 is 0 dBFS; threshold -20 dB, ratio 4 → 20·(1−¼)=15 dB
        // reduction, so the steady output should sit near −15 dBFS.
        let mut c = Compressor::new(SR, -20.0, 4.0, 0.001, 0.05);
        let peak = steady_peak(|x| c.tick(x), 1.0);
        let out_db = 20.0 * peak.log10();
        assert!((out_db + 15.0).abs() < 0.8, "output {out_db} dBFS");
        assert!((c.gain_reduction_db() - 15.0).abs() < 1.0);
    }

    #[test]
    fn makeup_gain_is_applied() {
        let mut c = Compressor::new(SR, 0.0, 1.0, 0.001, 0.05);
        c.set_makeup_gain_db(6.0);
        let peak = steady_peak(|x| c.tick(x), 0.25);
        assert!((peak - 0.5).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn limiter_holds_the_ceiling() {
        let mut l = Limiter::new(SR, -6.0, 0.05);
        let peak = steady_peak(|x| l.tick(x), 1.0);
        let ceiling = db_to_gain(-6.0);
        assert!(peak < ceiling * 1.05, "peak {peak} ceiling {ceiling}");
        assert!(peak > ceiling * 0.9);
    }

    #[test]
    fn gate_mutes_below_threshold_and_passes_above() {
        let mut g = Gate::new(SR, -30.0, 10.0, 0.001, 0.01);
        let quiet = steady_peak(|x| g.tick(x), 0.001); // about −60 dBFS
        assert!(quiet < 0.001 * 0.2, "quiet passed: {quiet}");
        let mut g = Gate::new(SR, -30.0, 10.0, 0.001, 0.01);
        let loud = steady_peak(|x| g.tick(x), 0.5); // about −6 dBFS
        assert!((loud - 0.5).abs() < 0.01, "loud altered: {loud}");
    }

    #[test]
    fn non_finite_input_does_not_poison_state() {
        let mut c = Compressor::new(SR, -10.0, 4.0, 0.005, 0.1);
        assert!(c.tick(f32::NAN).is_finite());
        assert!(c.tick(0.5).is_finite());
    }
}
