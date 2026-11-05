//! Oversampling for nonlinear processing.
//!
//! A nonlinearity (distortion, saturation) creates harmonics above the Nyquist
//! frequency, and those fold back as aliasing. [`Oversampler`] runs the
//! nonlinearity at `factor` times the sample rate: it zero-stuffs the input,
//! low-passes the image with a linear-phase FIR, applies the function at the
//! high rate, then low-passes again and decimates.
//!
//! The processing is allocation-free in steady state. The latency is
//! `(taps − 1) / factor` samples at the base rate.

use tpt_dsp_core::{Fir, FirDesign};

/// Oversampling wrapper around a per-sample nonlinearity.
pub struct Oversampler {
    factor: usize,
    up: Fir<f32>,
    down: Fir<f32>,
    taps: usize,
}

impl Oversampler {
    /// Create an oversampler.
    ///
    /// `factor` is clamped to `2..=16`. `taps` is made odd and at least 3, and
    /// larger values give a steeper anti-alias filter at more cost.
    pub fn new(factor: usize, taps: usize) -> Self {
        let factor = factor.clamp(2, 16);
        let mut taps = taps.max(3);
        if taps % 2 == 0 {
            taps += 1;
        }
        // Cutoff just under the base-rate Nyquist, in cycles per high-rate sample.
        let cutoff = 0.45 / factor as f32;
        let design = || {
            FirDesign::LowPass(cutoff)
                .try_design::<f32>(taps)
                .expect("oversampler design parameters are validated above")
        };
        Self {
            factor,
            up: design(),
            down: design(),
            taps,
        }
    }

    /// The oversampling factor.
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// Group delay in base-rate samples.
    pub fn latency(&self) -> f32 {
        (self.taps as f32 - 1.0) / self.factor as f32
    }

    /// Clear the filter state.
    pub fn reset(&mut self) {
        self.up.reset();
        self.down.reset();
    }

    /// Apply `f` to `buf` at the oversampled rate, in place.
    ///
    /// Each input sample is zero-stuffed into `factor` high-rate samples, each
    /// high-rate sample passes through `f`, and the anti-alias filter output is
    /// decimated back to the base rate. Non-finite input is replaced with zero.
    pub fn process<F: FnMut(f32) -> f32>(&mut self, buf: &mut [f32], mut f: F) {
        let r = self.factor as f32;
        for s in buf.iter_mut() {
            let x = if s.is_finite() { *s } else { 0.0 };
            let mut kept = 0.0f32;
            for i in 0..self.factor {
                // Zero insertion; the gain R compensates for the stuffed zeros.
                let stuffed = if i == 0 { x * r } else { 0.0 };
                let imaged = self.up.tick(stuffed);
                let shaped = f(imaged);
                let filtered = self.down.tick(shaped);
                // Keep the first high-rate sample of each group: it shares the
                // zero-insertion phase, so the composite delay is exactly
                // (taps − 1) / factor base-rate samples.
                if i == 0 {
                    kept = filtered;
                }
            }
            *s = kept;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn identity_nonlinearity_is_a_delayed_copy() {
        // taps = 33 at factor 4 gives an integer latency of 8 samples, so the
        // delayed copy is exact up to the filter's passband ripple.
        let mut os = Oversampler::new(4, 33);
        assert_eq!(os.latency(), 8.0);
        let latency = 8;
        let input: Vec<f32> = (0..2000)
            .map(|i| (i as f32 * 1000.0 * core::f32::consts::TAU / SR).sin())
            .collect();
        let mut buf = input.clone();
        os.process(&mut buf, |x| x);
        for i in 200..input.len() {
            assert!((buf[i] - input[i - latency]).abs() < 0.02, "sample {i}");
        }
    }

    #[test]
    fn dc_passes_through_unity() {
        let mut os = Oversampler::new(2, 21);
        let mut buf = vec![0.5f32; 500];
        os.process(&mut buf, |x| x);
        assert!((buf[400] - 0.5).abs() < 0.01, "{}", buf[400]);
    }

    #[test]
    fn oversampled_clip_has_less_alias_energy_than_naive_clip() {
        // Hard clipping a loud high tone creates odd harmonics that alias at
        // base rate. Oversampling should remove most of that aliased energy.
        let tone: Vec<f32> = (0..8192)
            .map(|i| 3.0 * (i as f32 * 9000.0 * core::f32::consts::TAU / SR).sin())
            .collect();
        let clip = |x: f32| x.clamp(-1.0, 1.0);
        let naive: Vec<f32> = tone.iter().map(|&x| clip(x)).collect();
        let mut os = Oversampler::new(8, 63);
        let mut over = tone.clone();
        os.process(&mut over, clip);
        // Energy of the signal in the 2–4 kHz band, where the clip's aliases of
        // the 9 kHz tone land (and nothing else is present).
        let band = |x: &[f32]| {
            let mut e = 0.0f32;
            for k in 0..200 {
                let f = 2000.0 + 10.0 * k as f32;
                let (mut re, mut im) = (0.0f32, 0.0f32);
                for (n, &v) in x.iter().enumerate().skip(1000) {
                    let w = f * n as f32 * core::f32::consts::TAU / SR;
                    re += v * w.cos();
                    im += v * w.sin();
                }
                e += re * re + im * im;
            }
            e
        };
        let naive_e = band(&naive);
        let over_e = band(&over);
        assert!(
            over_e < naive_e * 0.5,
            "naive {naive_e} oversampled {over_e}"
        );
    }

    #[test]
    fn non_finite_input_is_sanitised() {
        let mut os = Oversampler::new(4, 15);
        let mut buf = [f32::NAN, 0.3, f32::INFINITY, -0.3];
        os.process(&mut buf, |x| x.tanh());
        assert!(buf.iter().all(|v| v.is_finite()));
    }
}
