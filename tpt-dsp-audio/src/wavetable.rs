//! Wavetable synthesis.
//!
//! A wavetable stores one period of a waveform as a fixed-size lookup table
//! and reads it back with linear interpolation. Band-limited, low-CPU
//! oscillator replacement for naive per-sample math, and the basis for more
//! elaborate synthesis (morphing, single-cycle samples).

/// A single-cycle wavetable oscillator.
#[derive(Debug, Clone)]
pub struct Wavetable {
    table: Vec<f32>,
    phase: f32,
    frequency: f32,
    sample_rate: f32,
}

impl Wavetable {
    /// Build a table of `size` samples by sampling `f`, which maps a phase
    /// in `[0, 1)` to a sample value.
    pub fn from_fn(size: usize, sample_rate: f32, frequency: f32, f: impl Fn(f32) -> f32) -> Self {
        assert!(size >= 2, "wavetable must have at least 2 entries");
        let table = (0..size).map(|i| f(i as f32 / size as f32)).collect();
        Self {
            table,
            phase: 0.0,
            frequency,
            sample_rate,
        }
    }

    /// Build a table from one of the basic [`Waveform`](crate::Waveform)s.
    pub fn from_waveform(
        size: usize,
        sample_rate: f32,
        frequency: f32,
        waveform: crate::Waveform,
    ) -> Self {
        let two_pi = core::f32::consts::TAU;
        Self::from_fn(size, sample_rate, frequency, move |p| match waveform {
            crate::Waveform::Sine => (two_pi * p).sin(),
            crate::Waveform::Sawtooth => 2.0 * p - 1.0,
            crate::Waveform::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            crate::Waveform::Triangle => 1.0 - 4.0 * (p - 0.5).abs(),
        })
    }

    /// Build a table from an existing one-period sample buffer. The buffer is
    /// copied; `table[0]` corresponds to phase 0.
    pub fn from_samples(samples: &[f32], sample_rate: f32, frequency: f32) -> Self {
        assert!(samples.len() >= 2, "wavetable must have at least 2 entries");
        Self {
            table: samples.to_vec(),
            phase: 0.0,
            frequency,
            sample_rate,
        }
    }

    /// Set the playback frequency in Hz.
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the sample rate in Hz.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Reset the read phase to the start of the table.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Table length in samples.
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// `true` if the table is empty (it never is for a valid wavetable).
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// Advance one sample with linear interpolation, returning it.
    pub fn tick(&mut self) -> f32 {
        let len = self.table.len() as f32;
        let pos = self.phase * len;
        let i0 = pos.floor() as usize % self.table.len();
        let i1 = (i0 + 1) % self.table.len();
        let frac = pos - pos.floor();
        let s = self.table[i0] * (1.0 - frac) + self.table[i1] * frac;
        self.phase += self.frequency / self.sample_rate;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        s
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
    use crate::Waveform;

    #[test]
    fn sine_table_matches_direct_sine() {
        let mut wt = Wavetable::from_waveform(1024, 48000.0, 1000.0, Waveform::Sine);
        let mut osc = crate::Oscillator::with_waveform(48000.0, 1000.0, Waveform::Sine);
        for _ in 0..1000 {
            let a = wt.tick();
            let b = osc.tick();
            assert!((a - b).abs() < 0.02, "mismatch {a} vs {b}");
        }
    }

    #[test]
    fn table_is_periodic() {
        let mut wt = Wavetable::from_waveform(256, 48000.0, 440.0, Waveform::Sawtooth);
        for _ in 0..48000 {
            wt.tick();
        }
        assert!(wt.phase >= 0.0 && wt.phase < 1.0);
    }
}
