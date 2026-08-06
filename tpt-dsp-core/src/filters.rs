//! Filters: biquad (RBJ), FIR, and cascaded IIR.
//!
//! All processing functions operate on caller-provided slices and are
//! allocation-free. Coefficient-owning structs ([`Fir`], [`IirFilter`])
//! require the `alloc` feature; the single-stage [`Biquad`] is fully
//! `no_std`.

use num_traits::Float;

use crate::complex::pi;

/// Which response shape a biquad should have (RBJ audio EQ cookbook).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiquadType {
    /// Low-pass with `Q` controlling resonance at the cutoff.
    LowPass,
    /// High-pass.
    HighPass,
    /// Band-pass with constant 0 dB peak gain.
    BandPass,
    /// Notch (band-reject).
    Notch,
    /// All-pass (phase shifter).
    AllPass,
    /// Peaking EQ with `gain_db` boost/cut.
    Peaking,
    /// Low-shelf EQ.
    LowShelf,
    /// High-shelf EQ.
    HighShelf,
}

/// Normalized second-order section coefficients
/// (`a0 == 1`, `y = b0·x + b1·z⁻¹ + b2·z⁻² - a1·y·z⁻¹ - a2·y·z⁻²`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiquadCoeffs<F: Float> {
    /// Numerator tap for `x[n]`.
    pub b0: F,
    /// Numerator tap for `x[n-1]`.
    pub b1: F,
    /// Numerator tap for `x[n-2]`.
    pub b2: F,
    /// Denominator tap for `y[n-1]`.
    pub a1: F,
    /// Denominator tap for `y[n-2]`.
    pub a2: F,
}

impl<F: Float> BiquadCoeffs<F> {
    /// Design coefficients using the RBJ audio EQ cookbook.
    ///
    /// * `kind` — response shape.
    /// * `fs` — sample rate in Hz.
    /// * `f0` — corner / center frequency in Hz.
    /// * `q` — filter Q. Ignored by shelving filters.
    /// * `gain_db` — peak/shelf gain in dB. Ignored by LP/HP/BP/Notch/AP.
    ///
    /// # Panics
    ///
    /// Panics if `fs <= 0` or `f0` is not in `(0, fs/2]`.
    pub fn design(kind: BiquadType, fs: F, f0: F, q: F, gain_db: F) -> Self {
        let half = F::from(0.5).unwrap();
        let two = F::from(2.0).unwrap();
        assert!(fs > F::zero(), "sample rate must be positive");
        assert!(f0 > F::zero() && f0 <= fs * half, "frequency out of range");

        let w0 = pi::<F>() * F::from(2.0).unwrap() * f0 / fs;
        let cosw = w0.cos();
        let sinw = w0.sin();

        let a_db = F::from(10.0)
            .unwrap()
            .powf(gain_db / F::from(40.0).unwrap());
        let sq_a = a_db.sqrt();

        let alpha = match kind {
            BiquadType::LowShelf | BiquadType::HighShelf | BiquadType::Peaking => {
                sinw * half * (a_db + a_db.recip() - two).sqrt()
            }
            _ => sinw / (two * q),
        };

        let one = F::one();
        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadType::LowPass => {
                let c = one - cosw;
                (c * half, c, c * half, one + alpha, -two * cosw, one - alpha)
            }
            BiquadType::HighPass => {
                let c = one + cosw;
                (
                    c * half,
                    -c,
                    c * half,
                    one + alpha,
                    -two * cosw,
                    one - alpha,
                )
            }
            BiquadType::BandPass => (
                alpha,
                F::zero(),
                -alpha,
                one + alpha,
                -two * cosw,
                one - alpha,
            ),
            BiquadType::Notch => (one, -two * cosw, one, one + alpha, -two * cosw, one - alpha),
            BiquadType::AllPass => (
                one - alpha,
                -two * cosw,
                one + alpha,
                one + alpha,
                -two * cosw,
                one - alpha,
            ),
            BiquadType::Peaking => (
                one + alpha * a_db,
                -two * cosw,
                one - alpha * a_db,
                one + alpha / a_db,
                -two * cosw,
                one - alpha / a_db,
            ),
            BiquadType::LowShelf => {
                let s = sq_a * alpha;
                (
                    a_db * ((a_db + one) - (a_db - one) * cosw + two * s),
                    two * a_db * ((a_db - one) - (a_db + one) * cosw),
                    a_db * ((a_db + one) - (a_db - one) * cosw - two * s),
                    (a_db + one) + (a_db - one) * cosw + two * s,
                    -two * ((a_db - one) + (a_db + one) * cosw),
                    (a_db + one) + (a_db - one) * cosw - two * s,
                )
            }
            BiquadType::HighShelf => {
                let s = sq_a * alpha;
                (
                    a_db * ((a_db + one) + (a_db - one) * cosw + two * s),
                    -two * a_db * ((a_db - one) + (a_db + one) * cosw),
                    a_db * ((a_db + one) + (a_db - one) * cosw - two * s),
                    (a_db + one) - (a_db - one) * cosw + two * s,
                    two * ((a_db - one) - (a_db + one) * cosw),
                    (a_db + one) - (a_db - one) * cosw - two * s,
                )
            }
        };

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }
}

/// Process one block through a biquad (transposed direct-form II).
///
/// `state` is a `[x1, x2, y1, y2]` delay line; pass a persistent array and
/// it is updated in place. `input`/`output` may alias. Allocation-free.
pub fn process_biquad<F: Float>(
    coeffs: &BiquadCoeffs<F>,
    state: &mut [F; 4],
    input: &[F],
    output: &mut [F],
) {
    let n = input.len();
    assert!(output.len() >= n, "output too small for biquad");
    let mut x1 = state[0];
    let mut x2 = state[1];
    let mut y1 = state[2];
    let mut y2 = state[3];
    for i in 0..n {
        let x0 = input[i];
        let y0 = coeffs.b0 * x0 + x1;
        x1 = coeffs.b1 * x0 - coeffs.a1 * y0 + x2;
        x2 = coeffs.b2 * x0 - coeffs.a2 * y0;
        y2 = y1;
        y1 = y0;
        output[i] = y0;
    }
    state[0] = x1;
    state[1] = x2;
    state[2] = y1;
    state[3] = y2;
}

/// Single biquad stage with embedded state. Fully `no_std`.
#[derive(Debug, Clone, Copy)]
pub struct Biquad<F: Float> {
    coeffs: BiquadCoeffs<F>,
    state: [F; 4],
}

impl<F: Float> Biquad<F> {
    /// Create a biquad from explicit coefficients (reset state).
    pub fn from_coeffs(coeffs: BiquadCoeffs<F>) -> Self {
        Self {
            coeffs,
            state: [F::zero(); 4],
        }
    }

    /// Create a biquad designed with the RBJ cookbook (reset state).
    pub fn design(kind: BiquadType, fs: F, f0: F, q: F, gain_db: F) -> Self {
        Self::from_coeffs(BiquadCoeffs::design(kind, fs, f0, q, gain_db))
    }

    /// The current coefficients.
    #[inline]
    pub fn coeffs(&self) -> &BiquadCoeffs<F> {
        &self.coeffs
    }

    /// Replace coefficients (keeps filter state).
    pub fn set_coeffs(&mut self, coeffs: BiquadCoeffs<F>) {
        self.coeffs = coeffs;
    }

    /// Reset internal state to zero.
    pub fn reset(&mut self) {
        self.state = [F::zero(); 4];
    }

    /// Process one block in place of the given slice. Allocation-free.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        process_biquad(&self.coeffs, &mut self.state, input, output);
    }

    /// Process one sample, returning the filtered sample.
    pub fn tick(&mut self, x: F) -> F {
        let mut out = [F::zero()];
        process_biquad(
            &self.coeffs,
            &mut self.state,
            core::slice::from_ref(&x),
            &mut out,
        );
        out[0]
    }
}

#[cfg(feature = "alloc")]
/// A generic FIR filter: `y[n] = Σ_k h[k]·x[n-k]`.
#[derive(Clone)]
pub struct Fir<F: Float> {
    /// Filter taps, `h[0]` first.
    pub taps: alloc::vec::Vec<F>,
    history: alloc::vec::Vec<F>,
    pos: usize,
}

#[cfg(feature = "alloc")]
impl<F: Float> Fir<F> {
    /// Create a filter from explicit taps.
    pub fn new(taps: alloc::vec::Vec<F>) -> Self {
        let history = alloc::vec![F::zero(); taps.len()];
        Self {
            taps,
            history,
            pos: 0,
        }
    }

    /// Number of taps.
    #[inline]
    pub fn len(&self) -> usize {
        self.taps.len()
    }

    /// True if there are no taps.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.taps.is_empty()
    }

    /// Reset the delay line.
    pub fn reset(&mut self) {
        for h in self.history.iter_mut() {
            *h = F::zero();
        }
        self.pos = 0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: F) -> F {
        self.history[self.pos] = x;
        self.pos = (self.pos + 1) % self.taps.len();
        let mut acc = F::zero();
        let mut idx = self.pos;
        for &h in self.taps.iter() {
            acc = acc + h * self.history[idx];
            idx = (idx + 1) % self.taps.len();
        }
        acc
    }

    /// Process a whole block. `input` and `output` may alias.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        assert!(output.len() >= input.len(), "output too small for FIR");
        for (i, &x) in input.iter().enumerate() {
            output[i] = self.tick(x);
        }
    }
}

/// Windowed-sinc FIR design helpers.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FirDesign {
    /// Low-pass with the given cutoff (`0..0.5` cycles/sample).
    LowPass(f32),
    /// High-pass with the given cutoff.
    HighPass(f32),
    /// Band-pass with the given center and bandwidth.
    BandPass {
        /// Center frequency as a fraction of the sample rate (`0..0.5`).
        center: f32,
        /// Bandwidth as a fraction of the sample rate (`0..0.5`).
        bandwidth: f32,
    },
}

#[cfg(feature = "alloc")]
impl FirDesign {
    /// Design an odd-length (`taps`) windowed-sinc filter.
    ///
    /// `taps` should be odd. Uses a Blackman window for low sidelobes.
    ///
    /// # Panics
    ///
    /// Panics if `taps == 0`, is even, or cutoffs are out of range.
    pub fn design<F: Float>(self, taps: usize) -> Fir<F> {
        assert!(
            taps > 0 && taps % 2 == 1,
            "FIR tap count must be odd and nonzero"
        );
        let half = (taps / 2) as f32;

        let (lo, hi) = match self {
            FirDesign::LowPass(fc) => (0.0, fc),
            FirDesign::HighPass(fc) => (fc, 0.5),
            FirDesign::BandPass { center, bandwidth } => {
                (center - bandwidth / 2.0, center + bandwidth / 2.0)
            }
        };
        assert!(lo >= 0.0 && hi <= 0.5 && lo < hi, "bad filter design band");

        let mut coeffs = alloc::vec![F::zero(); taps];
        for (i, c) in coeffs.iter_mut().enumerate() {
            let n = i as f32 - half;
            // Blackman window.
            let w = 0.42 - 0.5 * (core::f32::consts::TAU * i as f32 / (taps as f32 - 1.0)).cos()
                + 0.08 * (core::f32::consts::TAU * 2.0 * i as f32 / (taps as f32 - 1.0)).cos();
            let mut h = if n == 0.0 {
                2.0 * (hi - lo)
            } else {
                let x = core::f32::consts::TAU * n;
                (x * hi).sin() / (core::f32::consts::PI * n)
                    - (x * lo).sin() / (core::f32::consts::PI * n)
            };
            h *= w;
            *c = F::from(h).unwrap();
        }
        Fir::new(coeffs)
    }
}

#[cfg(feature = "alloc")]
/// Coefficients for one biquad stage of an IIR cascade.
pub type IirCoeffs<F> = BiquadCoeffs<F>;

#[cfg(feature = "alloc")]
/// A single biquad stage in an IIR cascade.
pub type IirStage<F> = Biquad<F>;

#[cfg(feature = "alloc")]
/// A cascade of biquad stages implementing an arbitrary-order IIR filter.
pub struct IirFilter<F: Float> {
    stages: alloc::vec::Vec<IirStage<F>>,
    scratch: alloc::vec::Vec<F>,
}

#[cfg(feature = "alloc")]
impl<F: Float> IirFilter<F> {
    /// Create a filter from a cascade of biquad stages.
    pub fn new(stages: alloc::vec::Vec<IirStage<F>>) -> Self {
        Self {
            stages,
            scratch: alloc::vec::Vec::new(),
        }
    }

    /// Number of biquad stages.
    #[inline]
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// Reset all stage state.
    pub fn reset(&mut self) {
        for s in self.stages.iter_mut() {
            s.reset();
        }
    }

    /// Process one block through every stage.
    ///
    /// Uses a single internal scratch buffer that is (re)allocated only when
    /// the block length grows, so steady-state real-time use is allocation
    /// free.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        assert!(output.len() >= input.len(), "output too small for IIR");
        if self.scratch.len() < input.len() {
            self.scratch = alloc::vec![F::zero(); input.len()];
        }
        output[..input.len()].copy_from_slice(input);
        for stage in self.stages.iter_mut() {
            self.scratch[..input.len()].copy_from_slice(&output[..input.len()]);
            stage.process(&self.scratch[..input.len()], &mut output[..input.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn lowpass_attenuates_high_frequency() {
        let mut f = Biquad::<f32>::design(BiquadType::LowPass, FS, 1_000.0, 0.707, 0.0);
        let mut gain_high = 0.0f32;
        for i in 0..20_000 {
            let x = (i as f32 * 20_000.0 * core::f32::consts::TAU / FS).sin();
            gain_high = gain_high.max(f.tick(x).abs());
        }
        let mut f2 = Biquad::<f32>::design(BiquadType::LowPass, FS, 1_000.0, 0.707, 0.0);
        let mut gain_low = 0.0f32;
        for i in 0..20_000 {
            let x = (i as f32 * 100.0 * core::f32::consts::TAU / FS).sin();
            gain_low = gain_low.max(f2.tick(x).abs());
        }
        assert!(gain_low.abs() > 0.8, "low gain {gain_low}");
        assert!(gain_high.abs() < 0.1, "high gain {gain_high}");
    }

    #[test]
    fn allpass_preserves_magnitude() {
        let mut f = Biquad::<f64>::design(BiquadType::AllPass, 48_000.0, 500.0, 1.0, 0.0);
        let mut amp = 0.0f64;
        for i in 0..10_000 {
            let x = (i as f64 * 1_000.0 * std::f64::consts::TAU / 48_000.0).sin();
            // Skip the turn-on transient (which can overshoot a pure allpass).
            if i > 4_000 {
                amp = amp.max(f.tick(x).abs());
            } else {
                f.tick(x);
            }
        }
        assert!((amp - 1.0).abs() < 1e-3, "allpass amplitude {amp}");
    }

    #[test]
    fn peaking_gain_is_applied() {
        // A peaking filter at +12 dB should amplify the centre frequency.
        let mut f = Biquad::<f32>::design(BiquadType::Peaking, FS, 1_000.0, 1.0, 12.0);
        let mut peak = 0.0f32;
        for i in 0..20_000 {
            let x = (i as f32 * 1_000.0 * core::f32::consts::TAU / FS).sin();
            peak = peak.max(f.tick(x).abs());
        }
        assert!(peak > 3.0, "peaking gain too low: {peak}");
    }

    #[test]
    fn biquad_dc_gain_of_lowpass_is_one() {
        let c = BiquadCoeffs::<f64>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
        let dc = (c.b0 + c.b1 + c.b2) / (1.0 + c.a1 + c.a2);
        assert!((dc - 1.0).abs() < 1e-6, "DC gain {dc}");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn fir_lowpass_rejects_high_frequencies() {
        let f = FirDesign::LowPass(0.05).design::<f32>(63);
        assert_eq!(f.len(), 63);
        let mut lo = f.clone();
        let mut hi = f.clone();
        let mut lo_out = 0.0f32;
        let mut hi_out = 0.0f32;
        for i in 0..4_000 {
            lo_out = lo_out.max(
                lo.tick((i as f32 * 0.02 * core::f32::consts::TAU).sin())
                    .abs(),
            );
            hi_out = hi_out.max(
                hi.tick((i as f32 * 0.2 * core::f32::consts::TAU).sin())
                    .abs(),
            );
        }
        assert!(lo_out.abs() > 0.8, "low passed {lo_out}");
        assert!(hi_out.abs() < 0.1, "high passed {hi_out}");
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn iir_cascade_runs_and_resets() {
        let stage1 = Biquad::<f32>::design(BiquadType::LowPass, FS, 1_000.0, 0.707, 0.0);
        let stage2 = Biquad::<f32>::design(BiquadType::LowPass, FS, 4_000.0, 0.707, 0.0);
        let mut f = IirFilter::new(vec![stage1, stage2]);
        assert_eq!(f.stage_count(), 2);
        let input: Vec<f32> = (0..128).map(|i| (i as f32).sin()).collect();
        let mut out = vec![0.0f32; 128];
        f.process(&input, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
        f.reset();
    }
}
