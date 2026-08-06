//! FM demodulation from complex (IQ) baseband.
//!
//! The classic phase-delta (arctangent) discriminator: the instantaneous
//! frequency of a sample is the phase advance since the previous sample,
//! obtained as `angle(x[n] · conj(x[n-1]))`. Forming the product first
//! keeps the result inherently unwrapped — it is the principal value in
//! `(-π, π]`, which is the true phase step whenever the instantaneous
//! frequency stays below Nyquist.
//!
//! [`FmDemodulator`] holds the single sample of state this needs, so it is
//! fully `no_std`, `Copy`, and allocation-free on every path.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use num_complex::Complex;
use num_traits::Float;

use crate::complex::{pi, tau};

/// Phase advance from `previous` to `current`, in radians.
///
/// Equivalent to `angle(current · conj(previous))`, returned in `(-π, π]`.
#[inline]
pub fn phase_delta<F: Float>(current: Complex<F>, previous: Complex<F>) -> F {
    (current * previous.conj()).arg()
}

/// Convert a raw phase difference in radians to a baseband audio sample
/// normalized to `-1..=1`.
///
/// A full half-turn per sample (the maximum a discriminator can represent)
/// maps to full scale.
#[inline]
pub fn phase_to_audio<F: Float>(delta: F) -> F {
    delta / pi::<F>()
}

/// Phase-delta FM discriminator for complex baseband samples.
///
/// Each output sample is `gain · angle(x[n] · conj(x[n-1]))`. With the
/// default unity gain the output is the raw phase step in radians; use
/// [`with_deviation`](Self::with_deviation) to scale full deviation to
/// `±1.0` instead.
///
/// ```
/// # use tpt_dsp_core::{exp_i, C32, demod::FmDemodulator};
/// let mut fm = FmDemodulator::new(1.0f32);
/// let input: Vec<C32> = (0..8).map(|i| exp_i(0.25 * i as f32)).collect();
/// let mut audio = vec![0.0f32; input.len()];
/// fm.process(&input, &mut audio);
/// assert!((audio[4] - 0.25).abs() < 1e-5);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct FmDemodulator<F: Float> {
    prev: Complex<F>,
    gain: F,
}

impl<F: Float> FmDemodulator<F> {
    /// Create a demodulator that scales each phase step by `gain`.
    pub fn new(gain: F) -> Self {
        Self {
            prev: Complex::new(F::zero(), F::zero()),
            gain,
        }
    }

    /// Create a demodulator scaled so that a tone at the full frequency
    /// `deviation` (Hz) demodulates to `±1.0`.
    ///
    /// # Panics
    ///
    /// Panics if `sample_rate` or `deviation` is not positive.
    pub fn with_deviation(sample_rate: F, deviation: F) -> Self {
        assert!(sample_rate > F::zero(), "sample rate must be positive");
        assert!(deviation > F::zero(), "deviation must be positive");
        Self::new(sample_rate / (tau::<F>() * deviation))
    }

    /// The current output gain.
    #[inline]
    pub fn gain(&self) -> F {
        self.gain
    }

    /// Replace the output gain (keeps the previous sample).
    #[inline]
    pub fn set_gain(&mut self, gain: F) {
        self.gain = gain;
    }

    /// Forget the previous sample; the next [`tick`](Self::tick) returns
    /// zero.
    pub fn reset(&mut self) {
        self.prev = Complex::new(F::zero(), F::zero());
    }

    /// Demodulate one complex sample.
    pub fn tick(&mut self, z: Complex<F>) -> F {
        let delta = phase_delta(z, self.prev);
        self.prev = z;
        self.gain * delta
    }

    /// Demodulate one block. State carries over between calls, so blocks
    /// may have any length. Allocation-free.
    ///
    /// # Panics
    ///
    /// Panics if `output` is shorter than `input`.
    pub fn process(&mut self, input: &[Complex<F>], output: &mut [F]) {
        assert!(
            output.len() >= input.len(),
            "output too small for FM demodulator"
        );
        for (o, &z) in output.iter_mut().zip(input.iter()) {
            *o = self.tick(z);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{exp_i, C32, C64};

    const FS: f32 = 48_000.0;
    const TAU: f32 = core::f32::consts::TAU;

    #[test]
    fn demod_tracks_constant_frequency_offset() {
        let f0 = 1_000.0f32;
        let input: Vec<C32> = (0..256).map(|i| exp_i(TAU * f0 * i as f32 / FS)).collect();
        let mut fm = FmDemodulator::new(1.0f32);
        let mut out = vec![0.0f32; input.len()];
        fm.process(&input, &mut out);

        let expected = TAU * f0 / FS;
        assert!(out[0].abs() < 1e-6, "first sample {}", out[0]);
        for y in &out[1..] {
            assert!((y - expected).abs() < 1e-5, "got {y} want {expected}");
        }
    }

    #[test]
    fn demod_recovers_modulating_tone() {
        // Sine-modulated carrier: φ(t) = (dev/fm)·sin(2π·fm·t), so the
        // instantaneous frequency is dev·cos(2π·fm·t).
        let fm_hz = 1_000.0f32;
        let deviation = 5_000.0f32;
        let n = 2_048;
        let input: Vec<C32> = (0..n)
            .map(|i| {
                let t = i as f32 / FS;
                exp_i((deviation / fm_hz) * (TAU * fm_hz * t).sin())
            })
            .collect();

        let mut fm = FmDemodulator::with_deviation(FS, deviation);
        let mut out = vec![0.0f32; n];
        fm.process(&input, &mut out);

        // A first difference estimates the derivative half a sample back.
        for (i, y) in out.iter().enumerate().skip(1) {
            let t = (i as f32 - 0.5) / FS;
            let expected = (TAU * fm_hz * t).cos();
            assert!((y - expected).abs() < 0.02, "i={i} got {y} want {expected}");
        }
    }

    #[test]
    fn unmodulated_carrier_demodulates_to_zero() {
        let input = vec![C64::new(0.5, -0.25); 64];
        let mut fm = FmDemodulator::<f64>::new(1.0);
        let mut out = vec![0.0f64; input.len()];
        fm.process(&input, &mut out);
        assert!(out.iter().all(|y| y.abs() < 1e-12), "{out:?}");
    }

    #[test]
    fn tick_matches_process_across_blocks() {
        let input: Vec<C32> = (0..96).map(|i| exp_i(0.07 * i as f32).scale(2.0)).collect();

        let mut whole = FmDemodulator::new(0.5f32);
        let mut expected = vec![0.0f32; input.len()];
        whole.process(&input, &mut expected);

        let mut chunked = FmDemodulator::new(0.5f32);
        let got: Vec<f32> = input.iter().map(|&z| chunked.tick(z)).collect();

        assert_eq!(expected, got);
        assert!((chunked.gain() - 0.5).abs() < 1e-9);
        chunked.set_gain(1.0);
        chunked.reset();
        assert!(chunked.tick(input[0]).abs() < 1e-9);
    }

    #[test]
    fn phase_to_audio_normalizes_to_full_scale() {
        assert!((phase_to_audio(core::f32::consts::PI) - 1.0).abs() < 1e-6);
        assert!((phase_to_audio(-core::f32::consts::FRAC_PI_2) + 0.5).abs() < 1e-6);
        assert!(phase_to_audio(0.0f64).abs() < 1e-12);
    }

    #[test]
    fn phase_delta_wraps_correctly() {
        let a = exp_i(3.0f32);
        let b = exp_i(-3.0f32);
        // 3.0 → -3.0 is a step of +0.2832 rad, not -6.0.
        let d = phase_delta(b, a);
        assert!((d - (TAU - 6.0)).abs() < 1e-5, "delta {d}");
    }
}
