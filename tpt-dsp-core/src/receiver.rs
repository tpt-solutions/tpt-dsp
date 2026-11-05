//! Receiver building blocks: a second-order phase-locked loop (PLL) with a
//! Costas (BPSK) detector option, automatic gain control, a squelch, and AM and
//! SSB product demodulators.
//!
//! All types operate on complex baseband samples ([`C32`]) and are
//! allocation-free. Loop coefficients are derived from a natural frequency and
//! damping ratio, so the behaviour is set in hertz rather than raw gains.

// `no_std` builds have no inherent float math; see `windows.rs`.
#[allow(unused_imports)]
use num_traits::Float;

use crate::C32;

/// Wrap an angle into `[0, 2π)`. Uses floor rather than `rem_euclid`, which
/// is not available on `no_std` float types without an extra trait.
fn wrap_tau(x: f32) -> f32 {
    let t = core::f32::consts::TAU;
    let y = x - t * (x / t).floor();
    if y >= t {
        0.0
    } else {
        y
    }
}

/// Phase detector used by [`Pll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detector {
    /// Argument of the mixed sample: tracks any carrier.
    Complex,
    /// Costas loop for suppressed-carrier BPSK: `I·Q`, which is insensitive to
    /// the data sign.
    CostasBpsk,
}

/// Second-order PLL tracking the carrier phase and frequency of a complex
/// baseband signal.
#[derive(Debug, Clone)]
pub struct Pll {
    detector: Detector,
    kp: f32,
    ki: f32,
    /// Phase of the internal oscillator, radians.
    theta: f32,
    /// Oscillator frequency, radians per sample.
    omega: f32,
    /// Initial frequency, radians per sample (kept for `reset`).
    omega0: f32,
    sample_rate: f32,
    last_error: f32,
}

impl Pll {
    /// Create a loop.
    ///
    /// * `freq_hz` — initial frequency guess.
    /// * `bandwidth_hz` — natural frequency of the loop (how fast it locks).
    /// * `damping` — damping ratio, typically `0.5`–`1.0`.
    ///
    /// Invalid inputs fall back to safe values: a non-finite sample rate uses
    /// 48 kHz, and a non-positive bandwidth or damping is clamped to a small
    /// positive value.
    pub fn new(
        sample_rate: f32,
        freq_hz: f32,
        bandwidth_hz: f32,
        damping: f32,
        detector: Detector,
    ) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let wn = core::f32::consts::TAU * bandwidth_hz.max(1e-3) / sr;
        let zeta = damping.max(1e-3);
        let omega0 = core::f32::consts::TAU * freq_hz / sr;
        Self {
            detector,
            kp: 2.0 * zeta * wn,
            ki: wn * wn,
            theta: 0.0,
            omega: omega0,
            omega0,
            sample_rate: sr,
            last_error: 0.0,
        }
    }

    /// Current estimate of the input frequency in Hz.
    pub fn frequency_hz(&self) -> f32 {
        self.omega * self.sample_rate / core::f32::consts::TAU
    }

    /// Current oscillator phase in radians.
    pub fn phase(&self) -> f32 {
        self.theta
    }

    /// The most recent phase error (radians, or the Costas discriminant).
    pub fn last_error(&self) -> f32 {
        self.last_error
    }

    /// Reset to the initial frequency and zero phase.
    pub fn reset(&mut self) {
        self.theta = 0.0;
        self.omega = self.omega0;
        self.last_error = 0.0;
    }

    /// Process one sample: returns the derotated (locked) baseband sample.
    pub fn tick(&mut self, x: C32) -> C32 {
        let x = if x.re.is_finite() && x.im.is_finite() {
            x
        } else {
            C32::new(0.0, 0.0)
        };
        let (s, c) = self.theta.sin_cos();
        // Derotate by −θ.
        let y = C32::new(x.re * c + x.im * s, x.im * c - x.re * s);
        let err = match self.detector {
            Detector::Complex => y.im.atan2(y.re),
            Detector::CostasBpsk => y.re * y.im,
        };
        self.last_error = err;
        self.omega += self.ki * err;
        self.theta += self.omega + self.kp * err;
        // Keep the phase bounded so it never loses float precision.
        self.theta = wrap_tau(self.theta);
        y
    }
}

/// Feed-forward automatic gain control with separate attack and release.
#[derive(Debug, Clone)]
pub struct Agc {
    target: f32,
    attack: f32,
    release: f32,
    max_gain: f32,
    envelope: f32,
}

impl Agc {
    /// Create an AGC that drives the envelope toward `target` (linear, e.g.
    /// `0.5`). `attack_s` and `release_s` are time constants in seconds;
    /// `max_gain` caps the gain (linear).
    pub fn new(
        sample_rate: f32,
        target: f32,
        attack_s: f32,
        release_s: f32,
        max_gain: f32,
    ) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let coeff = |t: f32| {
            if t <= 0.0 {
                0.0
            } else {
                (-1.0 / (t * sr)).exp()
            }
        };
        Self {
            target: if target.is_finite() && target > 0.0 {
                target
            } else {
                0.5
            },
            attack: coeff(attack_s),
            release: coeff(release_s),
            max_gain: if max_gain.is_finite() && max_gain >= 1.0 {
                max_gain
            } else {
                1.0e4
            },
            envelope: 0.0,
        }
    }

    /// Process one sample.
    pub fn tick(&mut self, x: C32) -> C32 {
        let mag = x.norm();
        let mag = if mag.is_finite() { mag } else { 0.0 };
        let coeff = if mag > self.envelope {
            self.attack
        } else {
            self.release
        };
        self.envelope = mag + coeff * (self.envelope - mag);
        let gain = if self.envelope > 1e-12 {
            (self.target / self.envelope).min(self.max_gain)
        } else {
            self.max_gain
        };
        x * gain
    }
}

/// Squelch with hysteresis: opens above `threshold_db`, closes below
/// `threshold_db − hysteresis_db`.
#[derive(Debug, Clone)]
pub struct Squelch {
    open_db: f32,
    close_db: f32,
    coeff: f32,
    power: f32,
    open: bool,
}

impl Squelch {
    /// Create a squelch. `threshold_db` is relative to full scale (0 dBFS =
    /// unit magnitude). `tau_s` is the power-averaging time constant.
    pub fn new(sample_rate: f32, threshold_db: f32, hysteresis_db: f32, tau_s: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let thr = if threshold_db.is_finite() {
            threshold_db
        } else {
            -40.0
        };
        let hys = if hysteresis_db.is_finite() {
            hysteresis_db.max(0.0)
        } else {
            3.0
        };
        Self {
            open_db: thr,
            close_db: thr - hys,
            coeff: if tau_s > 0.0 {
                (-1.0 / (tau_s * sr)).exp()
            } else {
                0.0
            },
            power: 0.0,
            open: false,
        }
    }

    /// Whether the squelch is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Process one sample: returns the sample (or zero when closed) and the
    /// open state.
    pub fn tick(&mut self, x: C32) -> (C32, bool) {
        let p = x.norm_sqr();
        let p = if p.is_finite() { p } else { 0.0 };
        self.power = p + self.coeff * (self.power - p);
        let db = 10.0 * self.power.max(1e-20).log10();
        if self.open {
            if db < self.close_db {
                self.open = false;
            }
        } else if db > self.open_db {
            self.open = true;
        }
        if self.open {
            (x, true)
        } else {
            (C32::new(0.0, 0.0), false)
        }
    }
}

/// AM envelope demodulator with DC removal.
#[derive(Debug, Clone)]
pub struct AmDemodulator {
    dc: f32,
    coeff: f32,
}

impl AmDemodulator {
    /// Create a demodulator whose DC tracker has time constant `tau_s`
    /// (typically 0.5–2 s, long enough not to remove the audio).
    pub fn new(sample_rate: f32, tau_s: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        Self {
            dc: 0.0,
            coeff: if tau_s > 0.0 {
                (-1.0 / (tau_s * sr)).exp()
            } else {
                0.0
            },
        }
    }

    /// Process one baseband sample; returns the audio.
    pub fn tick(&mut self, x: C32) -> f32 {
        let env = x.norm();
        let env = if env.is_finite() { env } else { 0.0 };
        self.dc = env + self.coeff * (self.dc - env);
        env - self.dc
    }
}

/// SSB product detector: mixes the baseband by a beat-frequency oscillator and
/// takes the real part.
#[derive(Debug, Clone)]
pub struct SsbDemodulator {
    omega: f32,
    theta: f32,
}

impl SsbDemodulator {
    /// Create a detector with BFO offset `bfo_hz`. Use `0` when the baseband
    /// is already centred on the audio.
    pub fn new(sample_rate: f32, bfo_hz: f32) -> Self {
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let bfo = if bfo_hz.is_finite() { bfo_hz } else { 0.0 };
        Self {
            omega: core::f32::consts::TAU * bfo / sr,
            theta: 0.0,
        }
    }

    /// Process one baseband sample; returns the audio.
    pub fn tick(&mut self, x: C32) -> f32 {
        let (s, c) = self.theta.sin_cos();
        self.theta = wrap_tau(self.theta + self.omega);
        // Re(x · e^{−jθ}).
        let x = if x.re.is_finite() && x.im.is_finite() {
            x
        } else {
            C32::new(0.0, 0.0)
        };
        x.re * c + x.im * s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn pll_locks_onto_a_frequency_offset() {
        // Tone at 1000 Hz, loop starts at 1100 Hz.
        let mut pll = Pll::new(FS, 1100.0, 100.0, 0.707, Detector::Complex);
        let w = core::f32::consts::TAU * 1000.0 / FS;
        for i in 0..48_000 {
            let phase = w * i as f32;
            pll.tick(C32::new(phase.cos(), phase.sin()));
        }
        assert!(
            (pll.frequency_hz() - 1000.0).abs() < 0.5,
            "{}",
            pll.frequency_hz()
        );
    }

    #[test]
    fn costas_loop_locks_bpsk_and_aligns_the_axis() {
        // BPSK carrier at 2000 Hz with a 20 Hz offset and a 0.4 rad phase.
        let mut pll = Pll::new(FS, 2000.0, 60.0, 0.707, Detector::CostasBpsk);
        let w = core::f32::consts::TAU * 2020.0 / FS;
        let mut q_sum = 0.0f32;
        let mut i_sum = 0.0f32;
        let n = 96_000;
        for i in 0..n {
            let data = if (i / 480) % 2 == 0 { 1.0 } else { -1.0 };
            let phase = w * i as f32 + 0.4;
            let x = C32::new(data * phase.cos(), data * phase.sin());
            let y = pll.tick(x);
            if i > n - 9600 {
                q_sum += y.im.abs();
                i_sum += y.re.abs();
            }
        }
        assert!(
            (pll.frequency_hz() - 2020.0).abs() < 1.0,
            "{}",
            pll.frequency_hz()
        );
        assert!(q_sum < 0.1 * i_sum, "quadrature leakage {q_sum} vs {i_sum}");
    }

    #[test]
    fn agc_brings_loud_and_quiet_inputs_to_target() {
        let mut agc = Agc::new(FS, 0.5, 0.005, 0.2, 1e4);
        let mut last = 0.0f32;
        for i in 0..96_000 {
            let amp = if i < 48_000 { 0.05 } else { 3.0 };
            let x = C32::new(amp, 0.0);
            last = agc.tick(x).norm();
        }
        assert!((last - 0.5).abs() < 0.05, "output {last}");
    }

    #[test]
    fn squelch_opens_and_closes_with_hysteresis() {
        let mut sq = Squelch::new(FS, -20.0, 3.0, 0.01);
        let loud = C32::new(0.5, 0.0); // −6 dBFS
        let quiet = C32::new(0.01, 0.0); // −40 dBFS
        for _ in 0..4800 {
            sq.tick(loud);
        }
        assert!(sq.is_open());
        for _ in 0..4800 {
            sq.tick(quiet);
        }
        assert!(!sq.is_open());
        // Output is muted while closed.
        let (y, open) = sq.tick(quiet);
        assert!(!open && y.norm() == 0.0);
    }

    #[test]
    fn am_demod_recovers_the_envelope() {
        // The DC tracker needs a few time constants (0.05 s here) to settle
        // before the envelope is measured.
        let mut am = AmDemodulator::new(FS, 0.05);
        let mut peak = 0.0f32;
        for i in 0..96_000 {
            let env = 1.0 + 0.5 * (i as f32 * 1000.0 * core::f32::consts::TAU / FS).sin();
            let y = am.tick(C32::new(env, 0.0));
            if i > 72_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!((peak - 0.5).abs() < 0.02, "peak {peak}");
    }

    #[test]
    fn ssb_product_detector_shifts_the_beat() {
        // A complex tone at +1000 Hz with a 1000 Hz BFO becomes DC-like at 0 Hz.
        let mut ssb = SsbDemodulator::new(FS, 1000.0);
        let w = core::f32::consts::TAU * 1000.0 / FS;
        let mut out = Vec::new();
        for i in 0..4800 {
            let ph = w * i as f32;
            out.push(ssb.tick(C32::new(ph.cos(), ph.sin())));
        }
        let tail = &out[2400..];
        let mean: f32 = tail.iter().sum::<f32>() / tail.len() as f32;
        let var: f32 =
            tail.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / tail.len() as f32;
        assert!(var < 1e-4, "not settled: variance {var}");
        assert!((mean - 1.0).abs() < 0.01, "level {mean}");
    }
}
