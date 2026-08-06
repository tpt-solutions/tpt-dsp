//! Sample-rate conversion: integer decimation with an anti-alias FIR.
//!
//! [`FIRDecimator`] pairs a windowed-sinc low-pass with an integer rate
//! reduction by `M`: the filter is only evaluated on the samples that
//! survive downsampling, so the cost is `taps / M` multiply-accumulates per
//! input sample instead of `taps`.
//!
//! The delay line is sized once at construction and the filter phase is
//! kept across calls, so a stream can be pushed through in arbitrarily
//! sized blocks without allocating or losing carry-over samples.
//!
//! Requires the `alloc` feature for the tap / delay-line storage; the
//! processing path itself never allocates.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use num_traits::Float;

use crate::filters::FirDesign;

/// An integer-factor decimator: anti-alias low-pass followed by keeping
/// every `M`-th sample.
///
/// The anti-alias filter must cut off below the *decimated* Nyquist
/// frequency (`0.5 / M` in cycles per input sample), otherwise the
/// discarded spectrum folds back into the output band.
///
/// ```
/// # use tpt_dsp_core::resample::FIRDecimator;
/// let mut dec = FIRDecimator::<f32>::design(4, 0.1, 127);
/// let input = [0.0f32; 1024];
/// let mut output = [0.0f32; 256];
/// assert_eq!(dec.process(&input, &mut output), 256);
/// ```
#[derive(Clone)]
pub struct FIRDecimator<F: Float> {
    taps: alloc::vec::Vec<F>,
    history: alloc::vec::Vec<F>,
    pos: usize,
    phase: usize,
    factor: usize,
}

impl<F: Float> FIRDecimator<F> {
    /// Create a decimator from explicit FIR taps (`h[0]` multiplies the
    /// newest sample) and a decimation factor.
    ///
    /// # Panics
    ///
    /// Panics if `taps` is empty or `factor` is zero.
    pub fn new(taps: alloc::vec::Vec<F>, factor: usize) -> Self {
        assert!(!taps.is_empty(), "decimator needs at least one tap");
        assert!(factor > 0, "decimation factor must be at least 1");
        let history = alloc::vec![F::zero(); taps.len()];
        Self {
            taps,
            history,
            pos: 0,
            phase: 0,
            factor,
        }
    }

    /// Create a decimator with a windowed-sinc anti-alias low-pass.
    ///
    /// * `factor` — integer decimation factor `M`.
    /// * `cutoff` — low-pass cutoff as a fraction of the *input* sample
    ///   rate; must be below the decimated Nyquist `0.5 / M`.
    /// * `taps` — filter length; must be odd (linear phase).
    ///
    /// # Panics
    ///
    /// Panics if `factor` is zero, `cutoff` is not in `(0, 0.5 / factor)`,
    /// or `taps` is zero or even.
    pub fn design(factor: usize, cutoff: f32, taps: usize) -> Self {
        assert!(factor > 0, "decimation factor must be at least 1");
        assert!(
            cutoff > 0.0 && cutoff < 0.5 / factor as f32,
            "cutoff must lie below the decimated Nyquist frequency"
        );
        Self::new(FirDesign::LowPass(cutoff).design::<F>(taps).taps, factor)
    }

    /// Like [`design`](Self::design) with a cutoff at 80 % of the decimated
    /// Nyquist frequency, leaving a transition band before the fold point.
    ///
    /// # Panics
    ///
    /// Panics under the same conditions as [`design`](Self::design).
    pub fn design_default(factor: usize, taps: usize) -> Self {
        assert!(factor > 0, "decimation factor must be at least 1");
        Self::design(factor, 0.4 / factor as f32, taps)
    }

    /// The decimation factor `M`.
    #[inline]
    pub fn factor(&self) -> usize {
        self.factor
    }

    /// Number of filter taps.
    #[inline]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// Always `false`: a decimator is constructed with at least one tap.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    /// The filter taps.
    #[inline]
    pub fn taps(&self) -> &[F] {
        &self.taps
    }

    /// Group delay of the anti-alias filter, in input samples.
    #[inline]
    pub fn latency(&self) -> usize {
        (self.taps.len() - 1) / 2
    }

    /// Clear the delay line and reset the decimation phase.
    pub fn reset(&mut self) {
        for h in self.history.iter_mut() {
            *h = F::zero();
        }
        self.pos = 0;
        self.phase = 0;
    }

    /// How many output samples a block of `input_len` samples will produce
    /// given the current decimation phase.
    pub fn output_len(&self, input_len: usize) -> usize {
        let first = if self.phase == 0 {
            0
        } else {
            self.factor - self.phase
        };
        if first >= input_len {
            0
        } else {
            (input_len - first).div_ceil(self.factor)
        }
    }

    /// Push one input sample; returns the filtered sample on the input
    /// samples that survive decimation and `None` otherwise.
    pub fn tick(&mut self, x: F) -> Option<F> {
        self.history[self.pos] = x;
        let out = if self.phase == 0 {
            Some(self.filter())
        } else {
            None
        };
        self.pos += 1;
        if self.pos == self.history.len() {
            self.pos = 0;
        }
        self.phase += 1;
        if self.phase == self.factor {
            self.phase = 0;
        }
        out
    }

    /// Filter and decimate one block, returning the number of samples
    /// written to `output`.
    ///
    /// State carries over between calls, so blocks may have any length.
    /// Allocation-free.
    ///
    /// # Panics
    ///
    /// Panics if `output` is shorter than
    /// [`output_len`](Self::output_len) for this block.
    pub fn process(&mut self, input: &[F], output: &mut [F]) -> usize {
        let produced = self.output_len(input.len());
        assert!(output.len() >= produced, "output too small for decimator");
        let mut written = 0;
        for &x in input.iter() {
            if let Some(y) = self.tick(x) {
                output[written] = y;
                written += 1;
            }
        }
        written
    }

    fn filter(&self) -> F {
        let (head, tail) = self.history.split_at(self.pos + 1);
        let mut acc = F::zero();
        for (&h, &x) in self
            .taps
            .iter()
            .zip(head.iter().rev().chain(tail.iter().rev()))
        {
            acc = acc + h * x;
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(len: usize, freq: f32) -> Vec<f32> {
        (0..len)
            .map(|i| (core::f32::consts::TAU * freq * i as f32).sin())
            .collect()
    }

    fn peak(xs: &[f32]) -> f32 {
        xs.iter().fold(0.0f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn decimator_downsamples_by_factor() {
        let mut d = FIRDecimator::new(vec![1.0f32], 3);
        let input: Vec<f32> = (0..30).map(|i| i as f32).collect();
        let mut out = vec![0.0f32; 10];
        assert_eq!(d.output_len(30), 10);
        assert_eq!(d.process(&input, &mut out), 10);
        for (k, y) in out.iter().enumerate() {
            assert_eq!(*y, (k * 3) as f32);
        }
    }

    #[test]
    fn decimator_attenuates_above_new_nyquist() {
        // M = 4 → the decimated Nyquist sits at 0.125 cycles/sample.
        let n = 4_096;
        let mut out = vec![0.0f32; n / 4];

        let mut lo = FIRDecimator::<f32>::design(4, 0.1, 127);
        assert_eq!(lo.process(&tone(n, 0.01), &mut out), n / 4);
        let passband = peak(&out[64..]);

        let mut hi = FIRDecimator::<f32>::design(4, 0.1, 127);
        assert_eq!(hi.process(&tone(n, 0.2), &mut out), n / 4);
        let stopband = peak(&out[64..]);

        assert!(passband > 0.8, "passband gain {passband}");
        assert!(stopband < 0.01, "aliased stopband energy {stopband}");
    }

    #[test]
    fn decimator_output_length_tracks_phase() {
        let mut d = FIRDecimator::<f32>::design(4, 0.1, 31);
        assert_eq!(d.output_len(0), 0);
        assert_eq!(d.output_len(4), 1);
        assert_eq!(d.output_len(9), 3);
        let mut out = [0.0f32; 1];
        assert_eq!(d.process(&[0.0], &mut out), 1);
        assert_eq!(d.output_len(2), 0);
        assert_eq!(d.output_len(4), 1);
    }

    #[test]
    fn decimator_state_carries_across_blocks() {
        let input: Vec<f32> = (0..600).map(|i| (i as f32 * 0.03).sin()).collect();

        let mut whole = FIRDecimator::<f32>::design(5, 0.08, 63);
        let mut expected = vec![0.0f32; 120];
        assert_eq!(whole.process(&input, &mut expected), 120);

        let mut chunked = FIRDecimator::<f32>::design(5, 0.08, 63);
        let mut got = vec![0.0f32; 120];
        let mut written = 0;
        for chunk in input.chunks(37) {
            written += chunked.process(chunk, &mut got[written..]);
        }

        assert_eq!(written, 120);
        for (a, b) in expected.iter().zip(got.iter()) {
            assert!((a - b).abs() < 1e-6, "{a} != {b}");
        }
    }

    #[test]
    fn decimator_supports_f64() {
        let mut d = FIRDecimator::<f64>::design_default(2, 63);
        assert_eq!(d.factor(), 2);
        assert_eq!(d.len(), 63);
        assert_eq!(d.latency(), 31);
        assert!(!d.is_empty());
        assert_eq!(d.taps().len(), 63);
        let input: Vec<f64> = (0..512).map(|i| (i as f64 * 0.01).sin()).collect();
        let mut out = vec![0.0f64; 256];
        assert_eq!(d.process(&input, &mut out), 256);
        assert!(out.iter().all(|x| x.is_finite()));
        d.reset();
        assert_eq!(d.output_len(4), 2);
    }
}
