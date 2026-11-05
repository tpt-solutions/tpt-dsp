//! ADSR amplitude envelope.
//!
//! A classic attack/decay/sustain/release envelope producing a gain in
//! `[0, 1]`. Time constants are in seconds; the envelope is evaluated per
//! sample so it is allocation-free and real-time safe.

use crate::util::{is_valid_sample_rate, sanitize_sample_rate, sanitize_time};

/// The current segment an [`Adsr`] envelope is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeState {
    /// Idle — output is zero.
    Idle,
    /// Rising towards 1.0 over the attack time.
    Attack,
    /// Falling from 1.0 to the sustain level over the decay time.
    Decay,
    /// Held at the sustain level.
    Sustain,
    /// Falling from the current level to zero over the release time.
    Release,
}

/// An ADSR envelope generator.
#[derive(Debug, Clone)]
pub struct Adsr {
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    sample_rate: f32,
    state: EnvelopeState,
    value: f32,
}

impl Adsr {
    /// Create an envelope. Times are seconds; `sustain` is a level in `[0, 1]`.
    ///
    /// Non-finite times are treated as zero and a non-finite `sustain` as
    /// zero, so the output stays finite for any input.
    pub fn new(sample_rate: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> Self {
        Self {
            attack: sanitize_time(attack),
            decay: sanitize_time(decay),
            sustain: if sustain.is_finite() {
                sustain.clamp(0.0, 1.0)
            } else {
                0.0
            },
            release: sanitize_time(release),
            sample_rate: sanitize_sample_rate(sample_rate),
            state: EnvelopeState::Idle,
            value: 0.0,
        }
    }

    /// Set the sample rate in Hz. Non-finite or non-positive values are
    /// ignored, keeping the previous rate.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if is_valid_sample_rate(sample_rate) {
            self.sample_rate = sample_rate;
        }
    }

    /// The current output level.
    pub fn value(&self) -> f32 {
        self.value
    }

    /// The current state.
    pub fn state(&self) -> EnvelopeState {
        self.state
    }

    /// Trigger the note (begin attack). Re-triggers even while held.
    pub fn note_on(&mut self) {
        self.state = EnvelopeState::Attack;
    }

    /// Release the note (begin release from the current level).
    pub fn note_off(&mut self) {
        if self.state != EnvelopeState::Idle {
            self.state = EnvelopeState::Release;
        }
    }

    /// Advance one sample and return the gain.
    pub fn tick(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;
        match self.state {
            EnvelopeState::Idle => self.value = 0.0,
            EnvelopeState::Attack => {
                self.value += dt / self.attack.max(1e-6);
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.state = EnvelopeState::Decay;
                }
            }
            EnvelopeState::Decay => {
                self.value -= dt / self.decay.max(1e-6) * (1.0 - self.sustain);
                if self.value <= self.sustain {
                    self.value = self.sustain;
                    self.state = EnvelopeState::Sustain;
                }
            }
            EnvelopeState::Sustain => self.value = self.sustain,
            EnvelopeState::Release => {
                // Release is measured from the peak (1.0) so the rate is
                // constant regardless of where the release started.
                self.value -= dt / self.release.max(1e-6);
                if self.value <= 0.0 {
                    self.value = 0.0;
                    self.state = EnvelopeState::Idle;
                }
            }
        }
        self.value
    }

    /// Render `out` with successive envelope samples.
    pub fn process(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = self.tick();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_reaches_one() {
        let mut env = Adsr::new(1000.0, 0.1, 0.1, 0.5, 0.1);
        env.note_on();
        let mut peak = 0.0f32;
        for _ in 0..200 {
            peak = peak.max(env.tick());
        }
        assert!((peak - 1.0).abs() < 1e-3);
    }

    #[test]
    fn sustain_holds_level() {
        let mut env = Adsr::new(1000.0, 0.01, 0.01, 0.4, 0.5);
        env.note_on();
        for _ in 0..1000 {
            // 1 second held
            env.tick();
        }
        assert!((env.value() - 0.4).abs() < 1e-3, "got {}", env.value());
    }

    #[test]
    fn release_returns_to_zero() {
        let mut env = Adsr::new(1000.0, 0.01, 0.01, 0.7, 0.1);
        env.note_on();
        for _ in 0..50 {
            env.tick();
        }
        env.note_off();
        for _ in 0..200 {
            env.tick();
        }
        assert_eq!(env.value(), 0.0);
        assert_eq!(env.state(), EnvelopeState::Idle);
    }
}

#[cfg(test)]
mod robustness_tests {
    use super::*;

    #[test]
    fn non_finite_parameters_do_not_poison_output() {
        let mut env = Adsr::new(1000.0, f32::NAN, f32::INFINITY, f32::NAN, -1.0);
        env.note_on();
        for _ in 0..500 {
            assert!(env.tick().is_finite());
        }
    }

    #[test]
    fn set_sample_rate_changes_timing() {
        // Same wall-clock attack (0.1 s) at two rates should reach the peak
        // in the same number of seconds: 100 samples at 1 kHz.
        let mut env = Adsr::new(1000.0, 0.1, 0.1, 0.5, 0.1);
        env.set_sample_rate(2000.0);
        env.note_on();
        let mut n = 0;
        while env.state() == EnvelopeState::Attack && n < 10_000 {
            env.tick();
            n += 1;
        }
        assert!((190..=210).contains(&n), "attack took {n} samples");
    }
}
