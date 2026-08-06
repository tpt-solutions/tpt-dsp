//! Basic oscillator waveforms via a phase accumulator.
//!
//! A [`Oscillator`] generates one of several classic periodic waveforms at a
//! configurable frequency and sample rate. All state is a single phase value,
//! so it is allocation-free and real-time safe.

/// The shape produced by an [`Oscillator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waveform {
    /// Sinusoid.
    Sine,
    /// Ramp from -1 to +1 (wraps to -1).
    Sawtooth,
    /// +1 for the first half of the period, -1 for the second.
    Square,
    /// Triangle ranging -1..+1 with the peak at the midpoint.
    Triangle,
}

impl Default for Waveform {
    fn default() -> Self {
        Waveform::Sine
    }
}

/// A phase-accumulator oscillator.
///
/// The phase is stored as a fraction of a period in `[0, 1)`. Each sample the
/// phase advances by `frequency / sample_rate`; the waveform is evaluated at
/// that phase.
#[derive(Debug, Clone)]
pub struct Oscillator {
    waveform: Waveform,
    phase: f32,
    frequency: f32,
    sample_rate: f32,
}

impl Oscillator {
    /// Create a sine oscillator at `frequency` Hz running at `sample_rate` Hz.
    pub fn new(sample_rate: f32, frequency: f32) -> Self {
        Self {
            waveform: Waveform::Sine,
            phase: 0.0,
            frequency,
            sample_rate,
        }
    }

    /// Create an oscillator of an explicit waveform.
    pub fn with_waveform(sample_rate: f32, frequency: f32, waveform: Waveform) -> Self {
        Self {
            waveform,
            phase: 0.0,
            frequency,
            sample_rate,
        }
    }

    /// Set the oscillator frequency in Hz.
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the sample rate in Hz (e.g. after a device switch).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Replace the waveform.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Reset the phase to zero (avoids a discontinuity on note start).
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Advance one sample and return it. Range approximately `[-1, 1]`.
    pub fn tick(&mut self) -> f32 {
        let two_pi = core::f32::consts::TAU;
        let sample = match self.waveform {
            Waveform::Sine => (two_pi * self.phase).sin(),
            Waveform::Sawtooth => 2.0 * self.phase - 1.0,
            Waveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Triangle => 1.0 - 4.0 * (self.phase - 0.5).abs(),
        };
        self.phase += self.frequency / self.sample_rate;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
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
    fn sine_zero_crosses_once_per_period() {
        let mut osc = Oscillator::new(1000.0, 10.0); // 100 samples/period
        let mut prev = osc.tick();
        let mut crossings = 0;
        for _ in 0..1000 {
            let s = osc.tick();
            if prev < 0.0 && s >= 0.0 {
                crossings += 1;
            }
            prev = s;
        }
        // 1000 samples / 100 per period = 10 periods → ~10 rising edges.
        assert!((crossings as i32 - 10).abs() <= 1, "crossings {crossings}");
    }

    #[test]
    fn square_stays_within_bounds() {
        let mut osc = Oscillator::with_waveform(1000.0, 10.0, Waveform::Square);
        for _ in 0..1000 {
            let s = osc.tick();
            assert!(s == 1.0 || s == -1.0);
        }
    }

    #[test]
    fn phase_is_periodic() {
        let mut osc = Oscillator::new(48000.0, 440.0);
        for _ in 0..48000 {
            osc.tick();
        }
        assert!(osc.phase >= 0.0 && osc.phase < 1.0);
    }
}
