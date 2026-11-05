//! Simple IIR building blocks and classic analog-prototype designs.
//!
//! - [`OnePole`]: first-order low-pass / high-pass.
//! - [`DcBlocker`]: first-order high-pass with a DC zero, for removing offset.
//! - [`Svf`]: TPT (zero-delay) state-variable filter giving LP, BP and HP at
//!   once, stable under per-sample modulation of the cutoff.
//! - [`butterworth`]: order-N Butterworth low-/high-pass as a biquad cascade
//!   (alloc-gated).
//! - [`magnitude_response`] / [`cascade_magnitude_response`]: evaluate the
//!   response of a biquad or cascade at a frequency.
//!
//! The Butterworth biquads are RBJ cookbook sections placed at the analog
//! prototype's Q values, so the cutoff is at the RBJ (bilinear, unwarped)
//! frequency. That is accurate well below Nyquist; close to Nyquist the cutoff
//! sits slightly high.

// `no_std` builds have no inherent float math, so bring in the trait; `std`
// builds use the inherent methods and the import is unused.
#[allow(unused_imports)]
use num_traits::Float;

use crate::filters::{BiquadCoeffs, BiquadType, FilterDesignError};

fn check_cutoff(fs: f32, fc: f32) -> Result<(), FilterDesignError> {
    // Reuse the RBJ validation so every filter enforces the same rules.
    BiquadCoeffs::<f32>::try_design(BiquadType::LowPass, fs, fc, 0.707, 0.0).map(|_| ())
}

/// First-order low-pass or high-pass filter.
#[derive(Debug, Clone, Copy)]
pub struct OnePole {
    a: f32,
    y: f32,
    high_pass: bool,
}

impl OnePole {
    /// First-order low-pass with cutoff `fc` Hz.
    pub fn low_pass(fs: f32, fc: f32) -> Result<Self, FilterDesignError> {
        check_cutoff(fs, fc)?;
        let a = 1.0 - (-core::f32::consts::TAU * fc / fs).exp();
        Ok(Self {
            a,
            y: 0.0,
            high_pass: false,
        })
    }

    /// First-order high-pass with cutoff `fc` Hz (complement of the low-pass).
    pub fn high_pass(fs: f32, fc: f32) -> Result<Self, FilterDesignError> {
        let mut f = Self::low_pass(fs, fc)?;
        f.high_pass = true;
        Ok(f)
    }

    /// Clear the state.
    pub fn reset(&mut self) {
        self.y = 0.0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        if self.high_pass {
            x - self.y
        } else {
            self.y
        }
    }
}

/// First-order DC blocker: `y[n] = x[n] - x[n-1] + R·y[n-1]`.
#[derive(Debug, Clone, Copy)]
pub struct DcBlocker {
    r: f32,
    x1: f32,
    y1: f32,
}

impl DcBlocker {
    /// A DC blocker whose -3 dB corner is near `fc` Hz (default use: 5–20 Hz).
    pub fn new(fs: f32, fc: f32) -> Result<Self, FilterDesignError> {
        check_cutoff(fs, fc)?;
        let r = (-core::f32::consts::TAU * fc / fs).exp();
        Ok(Self {
            r,
            x1: 0.0,
            y1: 0.0,
        })
    }

    /// Clear the state.
    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.y1 = 0.0;
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = x - self.x1 + self.r * self.y1;
        self.x1 = x;
        self.y1 = y;
        y
    }
}

/// Outputs of one [`Svf`] step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvfOutput {
    /// Low-pass output.
    pub low: f32,
    /// Band-pass output.
    pub band: f32,
    /// High-pass output.
    pub high: f32,
}

/// TPT state-variable filter (Zavalishin's zero-delay form).
///
/// Stable for any cutoff below Nyquist, and safe to modulate sample by sample.
#[derive(Debug, Clone, Copy)]
pub struct Svf {
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Create an SVF with cutoff `fc` Hz and quality factor `q` (`> 0`).
    pub fn new(fs: f32, fc: f32, q: f32) -> Result<Self, FilterDesignError> {
        check_cutoff(fs, fc)?;
        if !(q.is_finite() && q > 0.0) {
            return Err(FilterDesignError::Q);
        }
        let mut s = Self {
            g: 0.0,
            k: 1.0 / q,
            a1: 0.0,
            a2: 0.0,
            a3: 0.0,
            ic1: 0.0,
            ic2: 0.0,
        };
        s.set_cutoff(fs, fc);
        Ok(s)
    }

    /// Re-tune the cutoff without resetting state. `fc` must be below Nyquist;
    /// invalid values are ignored.
    pub fn set_cutoff(&mut self, fs: f32, fc: f32) {
        if check_cutoff(fs, fc).is_err() {
            return;
        }
        self.g = (core::f32::consts::PI * fc / fs).tan();
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    /// Clear the integrator state.
    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }

    /// Process one sample, returning all three outputs.
    pub fn tick(&mut self, x: f32) -> SvfOutput {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOutput {
            low: v2,
            band: v1,
            high: x - self.k * v1 - v2,
        }
    }
}

/// Butterworth low-pass or high-pass.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// Low-pass.
    LowPass,
    /// High-pass.
    HighPass,
}

/// Design an order-`order` Butterworth filter as a cascade of biquads.
///
/// Even orders give `order/2` second-order sections; odd orders add one
/// first-order section. The overall gain at `fc` is exactly −3 dB.
///
/// # Errors
/// Returns an error for a zero order or an invalid `fs`/`fc`.
#[cfg(feature = "alloc")]
pub fn butterworth(
    response: Response,
    order: usize,
    fs: f32,
    fc: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, fc)?;
    let n = order as f32;
    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    for k in 1..=order / 2 {
        // Pole angle on the unit circle; its quadratic has Q = -1 / (2 cos φ).
        let phi = core::f32::consts::PI * (2.0 * k as f32 + n - 1.0) / (2.0 * n);
        let q = -1.0 / (2.0 * phi.cos());
        let kind = match response {
            Response::LowPass => BiquadType::LowPass,
            Response::HighPass => BiquadType::HighPass,
        };
        sections.push(BiquadCoeffs::<f32>::try_design(kind, fs, fc, q, 0.0)?);
    }
    if order % 2 == 1 {
        // First-order bilinear section for the real pole at s = -1.
        let kk = (core::f32::consts::PI * fc / fs).tan();
        let c = 1.0 / (kk + 1.0);
        let a1 = (kk - 1.0) / (kk + 1.0);
        let coeffs = match response {
            Response::LowPass => BiquadCoeffs {
                b0: kk * c,
                b1: kk * c,
                b2: 0.0,
                a1,
                a2: 0.0,
            },
            Response::HighPass => BiquadCoeffs {
                b0: c,
                b1: -c,
                b2: 0.0,
                a1,
                a2: 0.0,
            },
        };
        sections.push(coeffs);
    }
    Ok(sections)
}

/// Magnitude `|H(e^{jω})|` of one biquad at frequency `f` Hz.
pub fn magnitude_response(coeffs: &BiquadCoeffs<f32>, fs: f32, f: f32) -> f32 {
    let w = core::f32::consts::TAU * f / fs;
    // z⁻¹ = cos w − j sin w.
    let (c1, s1) = (w.cos(), -w.sin());
    let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
    // Numerator b0 + b1 z⁻¹ + b2 z⁻².
    let nr = coeffs.b0 + coeffs.b1 * c1 + coeffs.b2 * c2;
    let ni = coeffs.b1 * s1 + coeffs.b2 * s2;
    // Denominator 1 + a1 z⁻¹ + a2 z⁻².
    let dr = 1.0 + coeffs.a1 * c1 + coeffs.a2 * c2;
    let di = coeffs.a1 * s1 + coeffs.a2 * s2;
    let num = (nr * nr + ni * ni).sqrt();
    let den = (dr * dr + di * di).sqrt();
    num / den
}

/// Magnitude of a cascade of biquads at frequency `f` Hz (product of stages).
pub fn cascade_magnitude_response(sections: &[BiquadCoeffs<f32>], fs: f32, f: f32) -> f32 {
    sections
        .iter()
        .map(|c| magnitude_response(c, fs, f))
        .product()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn db(x: f32) -> f32 {
        20.0 * x.log10()
    }

    #[test]
    fn one_pole_low_pass_is_unity_at_dc_and_attenuates_high() {
        let mut f = OnePole::low_pass(FS, 1000.0).unwrap();
        let mut y = 0.0;
        for _ in 0..20_000 {
            y = f.tick(1.0);
        }
        assert!((y - 1.0).abs() < 1e-3, "dc {y}");
        let mut f = OnePole::low_pass(FS, 1000.0).unwrap();
        let mut peak = 0.0f32;
        for i in 0..4800 {
            let x = (i as f32 * 20_000.0 * core::f32::consts::TAU / FS).sin();
            if i > 1000 {
                peak = peak.max(f.tick(x).abs());
            } else {
                f.tick(x);
            }
        }
        assert!(peak < 0.2, "20 kHz leaked: {peak}");
    }

    #[test]
    fn dc_blocker_removes_offset() {
        let mut d = DcBlocker::new(FS, 10.0).unwrap();
        let mut y = 1.0;
        for _ in 0..96_000 {
            y = d.tick(0.5);
        }
        assert!(y.abs() < 1e-3, "residual dc {y}");
    }

    #[test]
    fn svf_low_pass_matches_cutoff_gain() {
        // At fc the TPT SVF low-pass with Q = 1/√2 has gain 1/√2 (−3 dB).
        let mut s = Svf::new(FS, 1000.0, core::f32::consts::FRAC_1_SQRT_2).unwrap();
        let mut peak = 0.0f32;
        for i in 0..48_000 {
            let x = (i as f32 * 1000.0 * core::f32::consts::TAU / FS).sin();
            let y = s.tick(x).low;
            if i > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(
            (peak - core::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "{peak}"
        );
    }

    #[test]
    fn svf_rejects_bad_parameters() {
        assert!(Svf::new(FS, 30_000.0, 0.7).is_err());
        assert!(matches!(
            Svf::new(FS, 1000.0, 0.0),
            Err(FilterDesignError::Q)
        ));
    }

    #[test]
    fn butterworth_is_minus_3db_at_cutoff_for_any_order() {
        for order in 1..=8 {
            let sections = butterworth(Response::LowPass, order, FS, 1000.0).unwrap();
            let g = cascade_magnitude_response(&sections, FS, 1000.0);
            assert!(
                (g - core::f32::consts::FRAC_1_SQRT_2).abs() < 0.005,
                "order {order}: |H(fc)| = {g}"
            );
        }
    }

    #[test]
    fn butterworth_is_flat_in_passband_and_steep_in_stopband() {
        let sections = butterworth(Response::LowPass, 4, FS, 1000.0).unwrap();
        assert!(db(cascade_magnitude_response(&sections, FS, 1.0)).abs() < 0.01);
        // Fourth order: roughly −80 dB at a decade above cutoff.
        let stop = db(cascade_magnitude_response(&sections, FS, 10_000.0));
        assert!(stop < -70.0, "stopband {stop} dB");
    }

    #[test]
    fn butterworth_high_pass_mirrors_low_pass() {
        let sections = butterworth(Response::HighPass, 4, FS, 1000.0).unwrap();
        let g = cascade_magnitude_response(&sections, FS, 1000.0);
        assert!((g - core::f32::consts::FRAC_1_SQRT_2).abs() < 0.005, "{g}");
        let low = db(cascade_magnitude_response(&sections, FS, 10.0));
        assert!(low < -70.0, "high-pass leaks DC: {low} dB");
    }

    #[test]
    fn butterworth_rejects_zero_order() {
        assert!(matches!(
            butterworth(Response::LowPass, 0, FS, 1000.0),
            Err(FilterDesignError::InvalidOrder)
        ));
    }
}

/// First-order bilinear low-pass section with cutoff `f1` Hz.
#[cfg(feature = "alloc")]
fn first_order_lowpass(fs: f32, f1: f32) -> Result<BiquadCoeffs<f32>, FilterDesignError> {
    check_cutoff(fs, f1)?;
    let kk = (core::f32::consts::PI * f1 / fs).tan();
    let c = 1.0 / (kk + 1.0);
    Ok(BiquadCoeffs {
        b0: kk * c,
        b1: kk * c,
        b2: 0.0,
        a1: (kk - 1.0) / (kk + 1.0),
        a2: 0.0,
    })
}

/// Chebyshev type I low-pass of order `order`, as a biquad cascade.
///
/// `ripple_db` is the passband ripple (for example `0.5` or `1.0`). The
/// design is normalised to unity DC gain, so the passband ripples between 1 and
/// `10^(−ripple_db/20)`. For odd orders the response is `−ripple_db` dB at
/// `fc`. For even orders DC is the ripple minimum, so after normalising DC to
/// 1 the response at `fc` is 0 dB. Each
/// pole pair is an RBJ low-pass at the prototype frequency scaled by `fc`, so
/// the cutoff is at the bilinear (unwarped) frequency, as for
/// [`butterworth`](crate::butterworth).
///
/// # Errors
/// Returns an error for a zero order, a non-positive ripple, or an invalid
/// `fs`/`fc`.
#[cfg(feature = "alloc")]
pub fn chebyshev1_lowpass(
    order: usize,
    fs: f32,
    fc: f32,
    ripple_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, fc)?;
    if !(ripple_db.is_finite() && ripple_db > 0.0) {
        return Err(FilterDesignError::Gain);
    }
    let eps = (10.0f32.powf(ripple_db / 10.0) - 1.0).sqrt();
    let n = order as f32;
    let a = (1.0 / eps).asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    for k in 0..order / 2 {
        // Prototype pole pair on the left half-plane ellipse.
        let theta = core::f32::consts::PI * (2.0 * k as f32 + 1.0) / (2.0 * n);
        let sigma = -sh * theta.sin();
        let omega = ch * theta.cos();
        let w0 = (sigma * sigma + omega * omega).sqrt();
        let q = w0 / (-2.0 * sigma);
        sections.push(BiquadCoeffs::<f32>::try_design(
            BiquadType::LowPass,
            fs,
            fc * w0,
            q,
            0.0,
        )?);
    }
    if order % 2 == 1 {
        // The real prototype pole sits at −sinh(a).
        sections.push(first_order_lowpass(fs, fc * sh)?);
    }
    // Normalise to unity DC gain: product of each section's DC gain.
    let dc: f32 = sections
        .iter()
        .map(|c| (c.b0 + c.b1 + c.b2) / (1.0 + c.a1 + c.a2))
        .product();
    if dc.is_finite() && dc.abs() > 1e-12 {
        let g = 1.0 / dc;
        let first = &mut sections[0];
        first.b0 *= g;
        first.b1 *= g;
        first.b2 *= g;
    }
    Ok(sections)
}

#[cfg(test)]
mod chebyshev_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn chebyshev_has_unity_dc_and_ripple_level_at_cutoff() {
        for &order in &[2usize, 3, 4, 5] {
            let ripple = 1.0;
            let s = chebyshev1_lowpass(order, FS, 1000.0, ripple).unwrap();
            let dc = cascade_magnitude_response(&s, FS, 0.5);
            assert!((dc - 1.0).abs() < 0.01, "order {order}: dc {dc}");
            let at_fc = cascade_magnitude_response(&s, FS, 1000.0);
            // Odd orders sit at −ripple dB at cutoff; even orders at 0 dB
            // (DC is the ripple minimum and is normalised to unity).
            let want = if order % 2 == 1 {
                10.0f32.powf(-ripple / 20.0)
            } else {
                1.0
            };
            assert!(
                (at_fc - want).abs() < 0.03,
                "order {order}: |H(fc)| {at_fc} vs {want}"
            );
        }
    }

    #[test]
    fn chebyshev_is_steeper_than_butterworth_in_the_stopband() {
        let cheb = chebyshev1_lowpass(4, FS, 1000.0, 1.0).unwrap();
        let butter = butterworth(Response::LowPass, 4, FS, 1000.0).unwrap();
        let c = cascade_magnitude_response(&cheb, FS, 1500.0);
        let b = cascade_magnitude_response(&butter, FS, 1500.0);
        assert!(c < b, "chebyshev {c} not below butterworth {b}");
    }

    #[test]
    fn chebyshev_rejects_bad_arguments() {
        assert!(matches!(
            chebyshev1_lowpass(0, FS, 1000.0, 1.0),
            Err(FilterDesignError::InvalidOrder)
        ));
        assert!(matches!(
            chebyshev1_lowpass(4, FS, 1000.0, 0.0),
            Err(FilterDesignError::Gain)
        ));
    }
}

/// Bilinear transform of an analog second-order section.
///
/// `num` and `den` hold polynomial coefficients in `s` as `[s⁰, s¹, s²]`. The
/// analog frequencies must already be pre-warped, so `s = 2·fs·(z−1)/(z+1)` is
/// the only substitution left.
#[cfg(feature = "alloc")]
fn bilinear_biquad(num: [f64; 3], den: [f64; 3], fs: f64) -> BiquadCoeffs<f32> {
    let c = 2.0 * fs;
    let c2 = c * c;
    let [n0, n1, n2] = num;
    let [d0, d1, d2] = den;
    // Coefficients of z², z¹, z⁰ after multiplying through by (z+1)².
    let bh = n0 + n1 * c + n2 * c2;
    let b1 = 2.0 * n0 - 2.0 * n2 * c2;
    let bl = n0 - n1 * c + n2 * c2;
    let ah = d0 + d1 * c + d2 * c2;
    let a1 = 2.0 * d0 - 2.0 * d2 * c2;
    let al = d0 - d1 * c + d2 * c2;
    BiquadCoeffs {
        b0: (bh / ah) as f32,
        b1: (b1 / ah) as f32,
        b2: (bl / ah) as f32,
        a1: (a1 / ah) as f32,
        a2: (al / ah) as f32,
    }
}

/// Chebyshev type II (inverse Chebyshev) low-pass, as a biquad cascade.
///
/// `fstop` is the stopband edge and `atten_db` the minimum stopband
/// attenuation. The passband is maximally flat (no ripple), and the response
/// is `−atten_db` dB at `fstop`. The stopband has equiripple zeros. The
/// design is normalised to unity DC gain. The analog prototype is
/// pre-warped at `fstop`, so the stopband edge lands exactly where asked.
///
/// # Errors
/// Returns an error for a zero order, a non-positive attenuation, or an
/// invalid `fs`/`fstop`.
#[cfg(feature = "alloc")]
pub fn chebyshev2_lowpass(
    order: usize,
    fs: f32,
    fstop: f32,
    atten_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, fstop)?;
    if !(atten_db.is_finite() && atten_db > 0.0) {
        return Err(FilterDesignError::Gain);
    }
    let fs64 = f64::from(fs);
    let n = order as f64;
    // Parameter: 1/ε = sqrt(10^{A/10} − 1).
    let inv_eps = (10.0f64.powf(f64::from(atten_db) / 10.0) - 1.0).sqrt();
    let a = inv_eps.asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    // Pre-warped stopband frequency, in analog rad/s.
    let wa = 2.0 * fs64 * (core::f64::consts::PI * f64::from(fstop) / fs64).tan();

    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    for k in 0..order / 2 {
        let theta = core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        // Chebyshev I pole, then inverted (the type II poles are 1/p).
        let (pr, pi) = (-sh * theta.sin(), ch * theta.cos());
        let mag2 = pr * pr + pi * pi;
        let (ir, ii) = (pr / mag2, -pi / mag2);
        let p_mag2 = (ir * ir + ii * ii) * wa * wa;
        let p_re = ir * wa;
        // Zero pair at ±j·wa/cos θ.
        let z = wa / theta.cos().abs().max(1e-12);
        // Denominator: s² − 2·Re(p)·s + |p|²; numerator: s² + z².
        sections.push(bilinear_biquad(
            [z * z, 0.0, 1.0],
            [p_mag2, -2.0 * p_re, 1.0],
            fs64,
        ));
    }
    if order % 2 == 1 {
        // Real pole at 1/(−sh), scaled; no finite zero for this section.
        let p_real = -wa / sh;
        sections.push(bilinear_biquad(
            [-p_real, 0.0, 0.0],
            [-p_real, 1.0, 0.0],
            fs64,
        ));
    }

    let dc: f32 = sections
        .iter()
        .map(|c| (c.b0 + c.b1 + c.b2) / (1.0 + c.a1 + c.a2))
        .product();
    if dc.is_finite() && dc.abs() > 1e-12 {
        let g = 1.0 / dc;
        let first = &mut sections[0];
        first.b0 *= g;
        first.b1 *= g;
        first.b2 *= g;
    }
    Ok(sections)
}

#[cfg(test)]
mod chebyshev2_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn chebyshev2_hits_the_stopband_attenuation_at_the_edge() {
        for &order in &[2usize, 3, 4, 5] {
            let s = chebyshev2_lowpass(order, FS, 3000.0, 40.0).unwrap();
            let dc = cascade_magnitude_response(&s, FS, 1.0);
            assert!((dc - 1.0).abs() < 0.01, "order {order}: dc {dc}");
            let at_stop = cascade_magnitude_response(&s, FS, 3000.0);
            let want = 10.0f32.powf(-40.0 / 20.0);
            assert!(
                (at_stop - want).abs() < 0.2 * want + 1e-4,
                "order {order}: |H(fstop)| {at_stop} vs {want}"
            );
        }
    }

    #[test]
    fn chebyshev2_passband_is_flat() {
        let s = chebyshev2_lowpass(4, FS, 3000.0, 40.0).unwrap();
        for f in [50.0f32, 200.0, 800.0] {
            let g = cascade_magnitude_response(&s, FS, f);
            assert!((g - 1.0).abs() < 0.01, "passband {f} Hz: {g}");
        }
    }

    #[test]
    fn chebyshev2_rejects_bad_arguments() {
        assert!(matches!(
            chebyshev2_lowpass(0, FS, 3000.0, 40.0),
            Err(FilterDesignError::InvalidOrder)
        ));
        assert!(matches!(
            chebyshev2_lowpass(4, FS, 3000.0, 0.0),
            Err(FilterDesignError::Gain)
        ));
    }
}
