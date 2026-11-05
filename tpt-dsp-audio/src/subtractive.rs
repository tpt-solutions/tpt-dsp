//! Subtractive synthesis voice: oscillator → low-pass filter → ADSR gain.
//!
//! The canonical "analog" synth topology. A single oscillator is shaped by a
//! resonant low-pass filter and an amplitude envelope. Starting a note sets
//! both the filter and amplitude envelopes; the filter cutoff can track the
//! envelope for a classic "pluck" response.

use tpt_dsp_core::{Biquad, BiquadType};

use crate::envelope::Adsr;

/// Number of samples between filter coefficient redesigns.
const COEFF_UPDATE_INTERVAL: u32 = 32;

/// A subtractive-synthesis voice.
#[derive(Debug, Clone)]
pub struct SubtractiveVoice {
    osc: crate::Oscillator,
    filter: Biquad<f32>,
    amp_env: Adsr,
    filter_env: Adsr,
    /// How much the filter envelope opens the cutoff, in Hz.
    filter_env_amount: f32,
    base_cutoff: f32,
    sample_rate: f32,
    /// Samples remaining until the next coefficient redesign.
    coeff_countdown: u32,
}

impl SubtractiveVoice {
    /// Create a voice with a sawtooth oscillator and sensible defaults.
    ///
    /// A non-finite or non-positive `sample_rate` falls back to 48 kHz.
    pub fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let osc = crate::Oscillator::with_waveform(sample_rate, 220.0, crate::Waveform::Sawtooth);
        let filter = Biquad::<f32>::design(BiquadType::LowPass, sample_rate, 1200.0, 4.0, 0.0);
        let amp_env = Adsr::new(sample_rate, 0.01, 0.2, 0.7, 0.3);
        let filter_env = Adsr::new(sample_rate, 0.01, 0.2, 0.0, 0.3);
        Self {
            osc,
            filter,
            amp_env,
            filter_env,
            filter_env_amount: 3000.0,
            base_cutoff: 1200.0,
            sample_rate,
            coeff_countdown: 0,
        }
    }

    /// Start a note at `frequency` Hz. Non-finite or negative frequencies
    /// are treated as silence (0 Hz).
    pub fn note_on(&mut self, frequency: f32) {
        self.osc.set_frequency(frequency);
        self.amp_env.note_on();
        self.filter_env.note_on();
    }

    /// Release the note.
    pub fn note_off(&mut self) {
        self.amp_env.note_off();
        self.filter_env.note_off();
    }

    /// `true` when the amplitude envelope has fully returned to idle.
    pub fn is_finished(&self) -> bool {
        self.amp_env.state() == crate::envelope::EnvelopeState::Idle
    }

    /// Set the sample rate in Hz (e.g. after a device switch). Non-finite or
    /// non-positive values are ignored.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate.is_finite() && sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            self.osc.set_sample_rate(sample_rate);
            self.amp_env.set_sample_rate(sample_rate);
            self.filter_env.set_sample_rate(sample_rate);
            // Force a coefficient refresh on the next tick.
            self.coeff_countdown = 0;
        }
    }

    /// Advance one sample and return it.
    ///
    /// The filter coefficients are redesigned once every
    /// `COEFF_UPDATE_INTERVAL` samples rather than every sample; the cutoff
    /// modulation is slow enough that this is inaudible, and it avoids a
    /// trigonometric design call per sample on the audio thread.
    pub fn tick(&mut self) -> f32 {
        let env = self.filter_env.tick();
        if self.coeff_countdown == 0 {
            let cutoff = self.base_cutoff + self.filter_env_amount * env;
            // `max` before `min` so a very low sample rate can't invert the
            // clamp bounds (which would panic).
            let cutoff = cutoff.max(20.0).min(self.sample_rate * 0.45);
            let c = Biquad::<f32>::design(BiquadType::LowPass, self.sample_rate, cutoff, 4.0, 0.0);
            self.filter.set_coeffs(*c.coeffs());
            self.coeff_countdown = COEFF_UPDATE_INTERVAL;
        }
        self.coeff_countdown -= 1;
        let s = self.osc.tick();
        let mut filtered = [0.0f32; 1];
        self.filter.process(&[s], &mut filtered);
        filtered[0] * self.amp_env.tick()
    }

    /// Render `out` with successive samples.
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
    fn note_produces_sound_then_decays() {
        let mut v = SubtractiveVoice::new(48000.0);
        v.note_on(220.0);
        let mut peak = 0.0f32;
        for _ in 0..1000 {
            peak = peak.max(v.tick().abs());
        }
        assert!(peak > 0.1, "peak {peak}");
        // Release and let it fade (release is 0.3 s = 14 400 samples).
        v.note_off();
        for _ in 0..16000 {
            v.tick();
        }
        assert!(v.is_finished());
    }

    #[test]
    fn output_is_finite() {
        let mut v = SubtractiveVoice::new(44100.0);
        v.note_on(330.0);
        for _ in 0..2000 {
            assert!(v.tick().is_finite());
        }
    }
}
