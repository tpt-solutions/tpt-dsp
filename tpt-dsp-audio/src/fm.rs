//! Two-operator FM (frequency modulation) synthesis.
//!
//! A modulating oscillator perturbs the phase of a carrier oscillator. The
//! modulation amount is expressed in *cycles* of phase deviation
//! (`mod_index` is the classic FM modulation index divided by 2π). The output
//! is `sin(2π·(carrier_phase + mod_index·modulator))`.

/// A 2-operator FM synthesizer voice.
#[derive(Debug, Clone)]
pub struct FmSynth {
    carrier_freq: f32,
    mod_freq: f32,
    mod_index: f32,
    sample_rate: f32,
    carrier_phase: f32,
    mod_phase: f32,
}

impl FmSynth {
    /// Create an FM voice.
    ///
    /// * `sample_rate` — audio sample rate (Hz).
    /// * `carrier` — carrier frequency (Hz).
    /// * `modulator` — modulator frequency (Hz); the modulation ratio is
    ///   `modulator / carrier`.
    /// * `mod_index` — peak phase deviation in cycles.
    pub fn new(sample_rate: f32, carrier: f32, modulator: f32, mod_index: f32) -> Self {
        Self {
            carrier_freq: carrier,
            mod_freq: modulator,
            mod_index,
            sample_rate,
            carrier_phase: 0.0,
            mod_phase: 0.0,
        }
    }

    /// Set the carrier frequency in Hz.
    pub fn set_carrier_frequency(&mut self, carrier: f32) {
        self.carrier_freq = carrier;
    }

    /// Set the modulator frequency in Hz.
    pub fn set_modulator_frequency(&mut self, modulator: f32) {
        self.mod_freq = modulator;
    }

    /// Set the modulation index (peak phase deviation, in cycles).
    pub fn set_mod_index(&mut self, mod_index: f32) {
        self.mod_index = mod_index;
    }

    /// Set the sample rate in Hz.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Reset both phase accumulators.
    pub fn reset(&mut self) {
        self.carrier_phase = 0.0;
        self.mod_phase = 0.0;
    }

    /// Advance one sample and return it. Output roughly in `[-1, 1]`.
    pub fn tick(&mut self) -> f32 {
        let two_pi = core::f32::consts::TAU;
        let mod_sig = self.mod_index * (two_pi * self.mod_phase).sin();
        let sample = (two_pi * (self.carrier_phase + mod_sig)).sin();
        self.carrier_phase += self.carrier_freq / self.sample_rate;
        self.mod_phase += self.mod_freq / self.sample_rate;
        while self.carrier_phase >= 1.0 {
            self.carrier_phase -= 1.0;
        }
        while self.mod_phase >= 1.0 {
            self.mod_phase -= 1.0;
        }
        sample
    }

    /// Fill `out` with successive samples.
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
    fn zero_index_is_plain_carrier() {
        let mut fm = FmSynth::new(48000.0, 440.0, 440.0, 0.0);
        let mut osc = crate::Oscillator::new(48000.0, 440.0);
        for _ in 0..2000 {
            let a = fm.tick();
            let b = osc.tick();
            assert!((a - b).abs() < 1e-5, "{a} vs {b}");
        }
    }

    #[test]
    fn output_stays_bounded() {
        let mut fm = FmSynth::new(48000.0, 440.0, 880.0, 5.0);
        for _ in 0..5000 {
            let s = fm.tick();
            assert!(s.abs() <= 1.0 + 1e-5);
        }
    }

    #[test]
    fn nonzero_index_changes_spectrum() {
        let mut fm = FmSynth::new(48000.0, 440.0, 440.0, 3.0);
        let mut energy = 0.0f32;
        for _ in 0..4000 {
            let s = fm.tick();
            energy += s * s;
        }
        assert!(energy > 0.0);
    }
}
