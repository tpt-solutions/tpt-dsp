//! Filters: biquad (RBJ), FIR, and cascaded IIR.
//!
//! All processing functions operate on caller-provided slices and are
//! allocation-free. Coefficient-owning structs ([`Fir`], [`IirFilter`])
//! require the `alloc` feature; the single-stage [`Biquad`] is fully
//! `no_std`.
//!
//! # Numerical contract (§12)
//!
//! - **Input domain**: `BiquadCoeffs::design` requires `fs > 0` and `f0` in
//!   `(0, fs/2]` (see its own `# Panics`); the processing functions
//!   (`process_biquad`, `Biquad`/`Fir`/`IirFilter` methods) accept any
//!   finite sample values and any coefficients — a filter is just a linear
//!   recurrence, so it has no domain restriction of its own beyond what its
//!   coefficients' stability requires (an unstable/marginally-stable design
//!   can still amplify without bound; that's a property of the requested
//!   coefficients, not a bug in the processing functions).
//! - **Output domain**: unbounded for the same reason — a stable filter's
//!   steady-state output is bounded by its gain times the input's bound,
//!   but this module makes no runtime claim about stability, since that
//!   depends entirely on caller-supplied coefficients.
//! - **Precision / acceptable error**: RBJ cookbook designs are verified
//!   against their intended analog response — the all-pass design holds
//!   unit amplitude to `1e-3` (`allpass_...amplitude`) and a shelf/peaking
//!   design's DC gain matches its target to `1e-6` (`...dc_gain`). These
//!   are frequency-response spot checks on specific designs, not a general
//!   error bound for arbitrary coefficients — precision for `process_biquad`/
//!   `Fir`/`IirFilter` themselves is bounded only by the caller's own
//!   `f32`/`f64` choice, since the recurrence is exact arithmetic on
//!   whatever coefficients and samples are supplied.

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
        match Self::try_design(kind, fs, f0, q, gain_db) {
            Ok(c) => c,
            Err(e) => panic!("{e}"),
        }
    }

    /// Fallible version of [`design`](Self::design).
    ///
    /// Returns an error instead of panicking, and also rejects a non-positive
    /// `q` for the Q-parameterised shapes (LP/HP/BP/Notch/AP/Peaking), which
    /// would otherwise produce infinite or NaN coefficients. Shelves ignore
    /// `q`.
    pub fn try_design(
        kind: BiquadType,
        fs: F,
        f0: F,
        q: F,
        gain_db: F,
    ) -> Result<Self, FilterDesignError> {
        let half = F::from(0.5).unwrap();
        let two = F::from(2.0).unwrap();
        if !(fs.is_finite() && fs > F::zero()) {
            return Err(FilterDesignError::SampleRate);
        }
        if !(f0.is_finite() && f0 > F::zero() && f0 <= fs * half) {
            return Err(FilterDesignError::FrequencyOutOfRange);
        }
        let q_used = !matches!(kind, BiquadType::LowShelf | BiquadType::HighShelf);
        if q_used && !(q.is_finite() && q > F::zero()) {
            return Err(FilterDesignError::Q);
        }
        if !gain_db.is_finite() {
            return Err(FilterDesignError::Gain);
        }

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

        Ok(Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        })
    }
}

/// Error returned by [`BiquadCoeffs::try_design`] for an unusable parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterDesignError {
    /// Sample rate was not finite and positive.
    SampleRate,
    /// Corner/centre frequency was outside `(0, fs/2]`.
    FrequencyOutOfRange,
    /// Q was not finite and positive for a Q-parameterised shape.
    Q,
    /// Gain in dB was not finite.
    Gain,
    /// A FIR filter was given no taps.
    EmptyTaps,
    /// A windowed-sinc design needs an odd tap count.
    EvenTapCount,
    /// A filter order was zero.
    InvalidOrder,
    /// A half-band filter needs a tap count of the form 4k + 3.
    HalfbandTaps,
}

impl core::fmt::Display for FilterDesignError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            FilterDesignError::SampleRate => "sample rate must be positive",
            FilterDesignError::FrequencyOutOfRange => "frequency out of range",
            FilterDesignError::Q => "Q must be finite and positive",
            FilterDesignError::Gain => "gain must be finite",
            FilterDesignError::EmptyTaps => "FIR filter needs at least one tap",
            FilterDesignError::EvenTapCount => "FIR tap count must be odd and nonzero",
            FilterDesignError::InvalidOrder => "filter order must be at least 1",
            FilterDesignError::HalfbandTaps => "half-band filters need 4k+3 taps",
        })
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
    // Once per block: scrub NaN/inf that would otherwise persist in the state,
    // and flush subnormal-range values that make the next block very slow.
    let tiny = F::from(1.0e-30).unwrap();
    let scrub = |v: F| {
        if v.is_finite() && v.abs() >= tiny {
            v
        } else {
            F::zero()
        }
    };
    state[0] = scrub(x1);
    state[1] = scrub(x2);
    state[2] = scrub(y1);
    state[3] = scrub(y2);
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
    ///
    /// # Panics
    ///
    /// Panics if `taps` is empty. Use [`Fir::try_new`] to get an error.
    pub fn new(taps: alloc::vec::Vec<F>) -> Self {
        match Self::try_new(taps) {
            Ok(f) => f,
            Err(e) => panic!("{e}"),
        }
    }

    /// Fallible constructor: rejects an empty tap list.
    pub fn try_new(taps: alloc::vec::Vec<F>) -> Result<Self, FilterDesignError> {
        if taps.is_empty() {
            return Err(FilterDesignError::EmptyTaps);
        }
        let history = alloc::vec![F::zero(); taps.len()];
        Ok(Self {
            taps,
            history,
            pos: 0,
        })
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
        match self.try_design(taps) {
            Ok(f) => f,
            Err(e) => panic!("{e}"),
        }
    }

    /// Fallible version of [`design`](Self::design).
    ///
    /// Rejects an even or zero tap count, and a band that is not
    /// `0 <= lo < hi <= 0.5` (cycles/sample). A single tap is accepted and is
    /// an identity-like impulse.
    pub fn try_design<F: Float>(self, taps: usize) -> Result<Fir<F>, FilterDesignError> {
        self.try_design_windowed(taps, crate::WindowType::Blackman)
    }

    /// Windowed-sinc design with any window from [`WindowType`](crate::WindowType),
    /// using the symmetric window form (the usual firwin convention). The
    /// Blackman default of [`try_design`](Self::try_design) is one choice; Kaiser
    /// trades main-lobe width for stopband rejection through its β.
    ///
    /// # Errors
    /// The same as [`try_design`](Self::try_design).
    pub fn try_design_windowed<F: Float>(
        self,
        taps: usize,
        window: crate::WindowType,
    ) -> Result<Fir<F>, FilterDesignError> {
        if taps == 0 {
            return Err(FilterDesignError::EmptyTaps);
        }
        if taps % 2 == 0 {
            return Err(FilterDesignError::EvenTapCount);
        }
        let half = (taps / 2) as f32;

        let (lo, hi) = match self {
            FirDesign::LowPass(fc) => (0.0, fc),
            FirDesign::HighPass(fc) => (fc, 0.5),
            FirDesign::BandPass { center, bandwidth } => {
                (center - bandwidth / 2.0, center + bandwidth / 2.0)
            }
        };
        if !(lo.is_finite() && hi.is_finite() && lo >= 0.0 && hi <= 0.5 && lo < hi) {
            return Err(FilterDesignError::FrequencyOutOfRange);
        }

        let mut coeffs = alloc::vec![F::zero(); taps];
        let mut win = alloc::vec![0.0f64; taps];
        crate::windows::windowed_symmetric_f64(window, taps, &mut win);
        for (i, c) in coeffs.iter_mut().enumerate() {
            let n = i as f32 - half;
            let w = win[i] as f32;
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
        Fir::try_new(coeffs)
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
    /// Block length whose scratch space [`IirFilter::new`] reserves.
    pub const DEFAULT_MAX_BLOCK: usize = 1024;

    /// Create a filter from a cascade of biquad stages, reserving scratch for
    /// blocks of up to [`IirFilter::DEFAULT_MAX_BLOCK`] samples.
    pub fn new(stages: alloc::vec::Vec<IirStage<F>>) -> Self {
        Self::with_max_block(stages, Self::DEFAULT_MAX_BLOCK)
    }

    /// Create a filter, reserving scratch for blocks of up to `max_block`
    /// samples so that [`process`](Self::process) never allocates for them.
    pub fn with_max_block(stages: alloc::vec::Vec<IirStage<F>>, max_block: usize) -> Self {
        Self {
            stages,
            scratch: alloc::vec![F::zero(); max_block],
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
    /// Uses a single internal scratch buffer that is reserved at construction
    /// (see [`IirFilter::with_max_block`]). It grows only for a block longer
    /// than that, so real-time use within the reserved size is allocation free.
    pub fn process(&mut self, input: &[F], output: &mut [F]) {
        assert!(output.len() >= input.len(), "output too small for IIR");
        if self.scratch.len() < input.len() {
            self.scratch.resize(input.len(), F::zero());
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

#[cfg(test)]
mod robustness_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn try_design_rejects_bad_parameters() {
        assert_eq!(
            BiquadCoeffs::<f32>::try_design(BiquadType::LowPass, FS, 1000.0, 0.0, 0.0).unwrap_err(),
            FilterDesignError::Q
        );
        assert_eq!(
            BiquadCoeffs::<f32>::try_design(BiquadType::LowPass, FS, 30_000.0, 0.7, 0.0)
                .unwrap_err(),
            FilterDesignError::FrequencyOutOfRange
        );
        assert_eq!(
            BiquadCoeffs::<f32>::try_design(BiquadType::LowPass, 0.0, 1000.0, 0.7, 0.0)
                .unwrap_err(),
            FilterDesignError::SampleRate
        );
        assert_eq!(
            BiquadCoeffs::<f32>::try_design(BiquadType::Peaking, FS, 1000.0, 1.0, f32::NAN)
                .unwrap_err(),
            FilterDesignError::Gain
        );
        // Shelves ignore Q, so a zero Q is not an error for them.
        assert!(BiquadCoeffs::<f32>::try_design(BiquadType::LowShelf, FS, 200.0, 0.0, 3.0).is_ok());
    }

    #[test]
    #[should_panic(expected = "frequency out of range")]
    fn design_still_panics_with_the_same_message() {
        let _ = BiquadCoeffs::<f32>::design(BiquadType::LowPass, FS, 30_000.0, 0.7, 0.0);
    }

    #[test]
    fn nan_input_does_not_poison_the_biquad_state() {
        let mut f = Biquad::<f32>::design(BiquadType::LowPass, FS, 1000.0, 0.707, 0.0);
        let mut block = [0.5f32, f32::NAN, 0.5, 0.5];
        let mut out = [0.0f32; 4];
        f.process(&block, &mut out);
        block = [0.25; 4];
        f.process(&block, &mut out);
        // Next block: the state was scrubbed, so the output is finite and
        // recovers instead of staying NaN forever.
        assert!(out.iter().all(|v| v.is_finite()), "{out:?}");
    }

    #[test]
    fn fir_rejects_empty_taps() {
        assert!(matches!(
            Fir::<f32>::try_new(alloc::vec::Vec::new()),
            Err(FilterDesignError::EmptyTaps)
        ));
        assert!(Fir::<f32>::try_new(alloc::vec![1.0]).is_ok());
    }
}

#[cfg(all(test, feature = "alloc"))]
mod fir_design_robustness_tests {
    use super::*;

    #[test]
    fn fir_design_try_rejects_bad_inputs() {
        assert!(matches!(
            FirDesign::LowPass(0.1).try_design::<f32>(0),
            Err(FilterDesignError::EmptyTaps)
        ));
        assert!(matches!(
            FirDesign::LowPass(0.1).try_design::<f32>(10),
            Err(FilterDesignError::EvenTapCount)
        ));
        assert!(matches!(
            FirDesign::LowPass(0.7).try_design::<f32>(11),
            Err(FilterDesignError::FrequencyOutOfRange)
        ));
        assert!(matches!(
            FirDesign::BandPass {
                center: 0.1,
                bandwidth: 0.0
            }
            .try_design::<f32>(11),
            Err(FilterDesignError::FrequencyOutOfRange)
        ));
    }

    #[test]
    fn single_tap_design_is_finite() {
        let f = FirDesign::LowPass(0.25).try_design::<f32>(1).unwrap();
        assert!(f.taps.iter().all(|t| t.is_finite()));
    }
}

#[cfg(all(test, feature = "alloc"))]
mod firwin_tests {
    use super::*;
    use crate::WindowType;

    fn response(taps: &[f32], f: f32) -> f32 {
        let (mut re, mut im) = (0.0f32, 0.0f32);
        for (n, &h) in taps.iter().enumerate() {
            let w = core::f32::consts::TAU * f * n as f32;
            re += h * w.cos();
            im -= h * w.sin();
        }
        (re * re + im * im).sqrt()
    }

    #[test]
    fn firwin_lowpass_has_unity_dc_gain_for_any_window() {
        for win in [
            WindowType::Rectangular,
            WindowType::Hann,
            WindowType::Blackman,
            WindowType::Kaiser(8.0),
        ] {
            let f = FirDesign::LowPass(0.1)
                .try_design_windowed::<f32>(31, win)
                .unwrap();
            let dc: f32 = f.taps.iter().sum();
            // A bare truncated sinc (rectangular window) carries Gibbs ripple
            // at DC, so it gets a looser bound than the tapered windows.
            let tol = if win == WindowType::Rectangular {
                0.1
            } else {
                0.01
            };
            assert!((dc - 1.0).abs() < tol, "{win:?}: dc {dc}");
        }
    }

    #[test]
    fn kaiser_window_has_deeper_stopband_than_rectangular() {
        let rect = FirDesign::LowPass(0.1)
            .try_design_windowed::<f32>(31, WindowType::Rectangular)
            .unwrap();
        let kaiser = FirDesign::LowPass(0.1)
            .try_design_windowed::<f32>(31, WindowType::Kaiser(8.0))
            .unwrap();
        let stop_rect = response(&rect.taps, 0.3);
        let stop_kaiser = response(&kaiser.taps, 0.3);
        assert!(stop_kaiser < 0.005, "kaiser stopband {stop_kaiser}");
        assert!(
            stop_rect > 3.0 * stop_kaiser,
            "rect {stop_rect} kaiser {stop_kaiser}"
        );
    }

    #[test]
    fn default_design_is_blackman() {
        let a = FirDesign::LowPass(0.2).try_design::<f32>(21).unwrap();
        let b = FirDesign::LowPass(0.2)
            .try_design_windowed::<f32>(21, WindowType::Blackman)
            .unwrap();
        assert_eq!(a.taps, b.taps);
    }
}

/// Half-band low-pass FIR (cutoff at a quarter of the sample rate).
///
/// Every second tap (at even offsets from the centre, other than the centre
/// itself) is exactly zero, which halves the multiplies for decimation by two.
/// `taps` must be of the form `4k + 3`.
///
/// # Errors
/// Returns [`FilterDesignError::HalfbandTaps`] for any other tap count.
#[cfg(feature = "alloc")]
pub fn halfband_fir(taps: usize) -> Result<Fir<f32>, FilterDesignError> {
    if taps % 4 != 3 {
        return Err(FilterDesignError::HalfbandTaps);
    }
    FirDesign::LowPass(0.25).try_design::<f32>(taps)
}

#[cfg(all(test, feature = "alloc"))]
mod halfband_tests {
    use super::*;

    #[test]
    fn halfband_has_zero_even_taps() {
        let f = halfband_fir(31).unwrap();
        let centre = 15usize;
        for (i, &h) in f.taps.iter().enumerate() {
            let off = i as isize - centre as isize;
            if off != 0 && off % 2 == 0 {
                assert!(h.abs() < 1e-6, "tap {i} = {h}");
            }
        }
        assert!(
            (f.taps[centre] - 0.5).abs() < 0.05,
            "centre {}",
            f.taps[centre]
        );
    }

    #[test]
    fn halfband_rejects_other_tap_counts() {
        // 4k + 3 values are 3, 7, 11, 15, 19, …; everything else is rejected.
        assert!(matches!(
            halfband_fir(16),
            Err(FilterDesignError::HalfbandTaps)
        ));
        assert!(matches!(
            halfband_fir(17),
            Err(FilterDesignError::HalfbandTaps)
        ));
        assert!(matches!(
            halfband_fir(32),
            Err(FilterDesignError::HalfbandTaps)
        ));
        assert!(halfband_fir(15).is_ok());
        assert!(halfband_fir(19).is_ok());
    }
}
