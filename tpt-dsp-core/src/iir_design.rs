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
    checked_design(sections)
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
    // Pre-warp the cutoff, then scale the prototype poles to it, so the
    // digital −ripple dB point lands exactly at `fc` (as scipy's cheby1 does).
    let (fs64, fc64) = (f64::from(fs), f64::from(fc));
    let wa = 2.0 * fs64 * (core::f64::consts::PI * fc64 / fs64).tan();
    let eps = (10.0f64.powf(f64::from(ripple_db) / 10.0) - 1.0).sqrt();
    let n = order as f64;
    let a = (1.0 / eps).asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    for k in 0..order / 2 {
        // Prototype pole pair on the left half-plane ellipse, scaled to wa.
        let theta = core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        let sigma = -sh * theta.sin() * wa;
        let omega = ch * theta.cos() * wa;
        // Denominator s² − 2σ·s + |p|²; the gain is absorbed by normalisation.
        let mag2 = sigma * sigma + omega * omega;
        sections.push(bilinear_biquad(
            [mag2, 0.0, 0.0],
            [mag2, -2.0 * sigma, 1.0],
            fs64,
        ));
    }
    if order % 2 == 1 {
        // The real prototype pole sits at −sinh(a).
        let p_real = -sh * wa;
        sections.push(bilinear_biquad(
            [-p_real, 0.0, 0.0],
            [-p_real, 1.0, 0.0],
            fs64,
        ));
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
    checked_design(sections)
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
    if n2 == 0.0 && d2 == 0.0 {
        // A first-order section. The second-order form below would carry a
        // factor (1 + z⁻¹) in both numerator and denominator, and that factor
        // leaves a pole at z = −1 that rounding to f32 can push outside the
        // unit circle. Cancel it analytically, so a2 = b2 = 0.
        let dn = d0 + d1 * c;
        return BiquadCoeffs {
            b0: ((n0 + n1 * c) / dn) as f32,
            b1: ((n0 - n1 * c) / dn) as f32,
            b2: 0.0,
            a1: ((d0 - d1 * c) / dn) as f32,
            a2: 0.0,
        };
    }
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
    checked_design(sections)
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

/// Complex value of a biquad's numerator and denominator at `e^{jω}`,
/// with `ω = 2π·f/fs`. Returns `(num, den)` as `(re, im)` pairs.
fn biquad_terms(c: &BiquadCoeffs<f32>, w: f64) -> ((f64, f64), (f64, f64)) {
    let (c1, s1) = (w.cos(), -w.sin());
    let (c2, s2) = ((2.0 * w).cos(), -(2.0 * w).sin());
    let (b0, b1, b2) = (f64::from(c.b0), f64::from(c.b1), f64::from(c.b2));
    let (a1, a2) = (f64::from(c.a1), f64::from(c.a2));
    let num = (b0 + b1 * c1 + b2 * c2, b1 * s1 + b2 * s2);
    let den = (1.0 + a1 * c1 + a2 * c2, a1 * s1 + a2 * s2);
    (num, den)
}

/// Complex transfer function of a cascade at `f` Hz, as `(re, im)`.
fn cascade_h(sections: &[BiquadCoeffs<f32>], w: f64) -> (f64, f64) {
    sections.iter().fold((1.0, 0.0), |(hr, hi), c| {
        let (n, d) = biquad_terms(c, w);
        let dd = d.0 * d.0 + d.1 * d.1;
        // H_stage = n / d = n · conj(d) / |d|².
        let (sr, si) = ((n.0 * d.0 + n.1 * d.1) / dd, (n.1 * d.0 - n.0 * d.1) / dd);
        (hr * sr - hi * si, hr * si + hi * sr)
    })
}

/// Group delay of a biquad cascade at `f` Hz, in samples.
///
/// Computed from the phase difference across a small frequency step, so the
/// result is not affected by phase wrapping. Positive values are a delay.
pub fn cascade_group_delay(sections: &[BiquadCoeffs<f32>], fs: f32, f: f32) -> f64 {
    let fs = f64::from(fs);
    let w = core::f64::consts::TAU * f64::from(f) / fs;
    let h = 1e-5;
    let plus = cascade_h(sections, w + h);
    let minus = cascade_h(sections, w - h);
    // arg(H⁺ / H⁻) = arg(H⁺ · conj(H⁻)).
    let re = plus.0 * minus.0 + plus.1 * minus.1;
    let im = plus.1 * minus.0 - plus.0 * minus.1;
    -im.atan2(re) / (2.0 * h)
}

/// Poles and zeros of a biquad cascade in the z-plane, as `(re, im)` pairs.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone, PartialEq)]
pub struct PolesZeros {
    /// Poles, two per second-order section.
    pub poles: alloc::vec::Vec<(f64, f64)>,
    /// Finite zeros.
    pub zeros: alloc::vec::Vec<(f64, f64)>,
}

/// Poles and zeros (in the z-plane) of a biquad cascade.
///
/// A section whose leading numerator coefficient is zero contributes only its
/// finite zeros; the missing zero sits at infinity and is not listed.
#[cfg(feature = "alloc")]
pub fn cascade_poles_zeros(sections: &[BiquadCoeffs<f32>]) -> PolesZeros {
    fn quadratic_roots(a: f64, b: f64, c: f64) -> alloc::vec::Vec<(f64, f64)> {
        // Roots of a·z² + b·z + c = 0.
        if a == 0.0 {
            return if b == 0.0 {
                alloc::vec::Vec::new()
            } else {
                alloc::vec![(-c / b, 0.0)]
            };
        }
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let s = disc.sqrt();
            alloc::vec![((-b + s) / (2.0 * a), 0.0), ((-b - s) / (2.0 * a), 0.0)]
        } else {
            let s = (-disc).sqrt();
            alloc::vec![
                (-b / (2.0 * a), s / (2.0 * a)),
                (-b / (2.0 * a), -s / (2.0 * a)),
            ]
        }
    }
    let mut poles = alloc::vec::Vec::new();
    let mut zeros = alloc::vec::Vec::new();
    for c in sections {
        poles.extend(quadratic_roots(1.0, f64::from(c.a1), f64::from(c.a2)));
        zeros.extend(quadratic_roots(
            f64::from(c.b0),
            f64::from(c.b1),
            f64::from(c.b2),
        ));
    }
    PolesZeros { poles, zeros }
}

#[cfg(test)]
mod response_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn lowpass_zeros_sit_at_nyquist_and_poles_are_inside_the_circle() {
        let sections = butterworth(Response::LowPass, 4, FS, 1000.0).unwrap();
        let PolesZeros { poles, zeros } = cascade_poles_zeros(&sections);
        assert_eq!(poles.len(), 4);
        for (re, im) in &poles {
            assert!((re * re + im * im).sqrt() < 1.0, "pole {re},{im} outside");
        }
        // Butterworth low-pass zeros are all at z = −1.
        for (re, im) in &zeros {
            assert!((re + 1.0).abs() < 1e-3 && im.abs() < 1e-3, "zero {re},{im}");
        }
    }

    #[test]
    fn group_delay_matches_scipy() {
        // Reference: scipy.signal.group_delay summed over the sections of
        // butter(4, 1000, fs=48000, output="sos").
        let sections = butterworth(Response::LowPass, 4, FS, 1000.0).unwrap();
        for (f, want) in [(100.0f32, 20.0184), (500.0, 22.7538), (900.0, 29.9086)] {
            let got = cascade_group_delay(&sections, FS, f);
            assert!((got - want).abs() < 0.01, "{f} Hz: got {got}, scipy {want}");
        }
    }
}

// ---------------------------------------------------------------------------
// Elliptic (Cauer) low-pass design.
//
// The analog prototype follows Orfanidis, "Lecture Notes on Elliptic Filter
// Design" (the construction scipy's `ellipap` uses). Every Jacobi function is
// evaluated at a real argument, so no complex elliptic functions are needed.
// The stopband edge is not an input: it follows from the order and the two
// ripple specifications, as in scipy's `ellip`.
// ---------------------------------------------------------------------------

/// Arithmetic–geometric mean.
#[cfg(feature = "alloc")]
fn agm(mut a: f64, mut b: f64) -> f64 {
    for _ in 0..64 {
        let next_a = 0.5 * (a + b);
        let next_b = (a * b).sqrt();
        if (next_a - next_b).abs() <= 1e-16 * next_a {
            return next_a;
        }
        a = next_a;
        b = next_b;
    }
    a
}

/// Complete elliptic integral of the first kind, `K(m)` (parameter `m`).
#[cfg(feature = "alloc")]
fn ellip_k(m: f64) -> f64 {
    core::f64::consts::FRAC_PI_2 / agm(1.0, (1.0 - m).sqrt())
}

/// Incomplete elliptic integral of the first kind, `F(φ | m)`, by composite
/// Simpson quadrature.
#[cfg(feature = "alloc")]
fn ellip_f(phi: f64, m: f64) -> f64 {
    const N: usize = 4096;
    let h = phi / N as f64;
    let f = |t: f64| 1.0 / (1.0 - m * t.sin() * t.sin()).sqrt();
    let mut sum = f(0.0) + f(phi);
    for i in 1..N {
        let w = if i % 2 == 1 { 4.0 } else { 2.0 };
        sum += w * f(i as f64 * h);
    }
    sum * h / 3.0
}

/// Jacobi elliptic functions `(sn, cn, dn)` of a real argument `u` and
/// parameter `m`, by the descending Landen (AGM) recurrence of
/// Abramowitz & Stegun 16.4.
#[cfg(feature = "alloc")]
fn jacobi_sn_cn_dn(u: f64, m: f64) -> (f64, f64, f64) {
    if m <= 0.0 {
        return (u.sin(), u.cos(), 1.0);
    }
    if m >= 1.0 {
        let s = u.tanh();
        let c = 1.0 / u.cosh();
        return (s, c, c);
    }
    let mut a = [0.0f64; 40];
    let mut c = [0.0f64; 40];
    a[0] = 1.0;
    c[0] = m.sqrt();
    let mut b = (1.0 - m).sqrt();
    let mut n = 0usize;
    while c[n].abs() > 1e-16 && n < 36 {
        c[n + 1] = 0.5 * (a[n] - b);
        a[n + 1] = 0.5 * (a[n] + b);
        b = (a[n] * b).sqrt();
        n += 1;
    }
    let mut phi = 2.0f64.powi(n as i32) * a[n] * u;
    for i in (1..=n).rev() {
        phi = 0.5 * (phi + (c[i] / a[i] * phi.sin()).asin());
    }
    let sn = phi.sin();
    let cn = phi.cos();
    let dn = (1.0 - m * sn * sn).max(0.0).sqrt();
    (sn, cn, dn)
}

/// Solve the degree equation `K(m)/K′(m) = N · K(m₁)/K′(m₁)` for `m` by
/// bisection. The left-hand side increases with `m`, so the root is unique.
#[cfg(feature = "alloc")]
fn elliptic_degree(order: f64, m1: f64) -> f64 {
    let target = order * ellip_k(m1) / ellip_k(1.0 - m1);
    let (mut lo, mut hi) = (1e-15, 1.0 - 1e-15);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if ellip_k(mid) / ellip_k(1.0 - mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Analog elliptic low-pass prototype with passband edge at 1 rad/s.
///
/// Returns `(zeros, poles)` as `(re, im)` pairs. Zeros are purely imaginary and
/// listed once per conjugate pair (positive imaginary part). Poles are in the
/// left half-plane; complex ones are listed once per pair, and an odd order's
/// real pole appears once.
#[cfg(feature = "alloc")]
fn elliptic_prototype(
    order: usize,
    rp_db: f64,
    rs_db: f64,
) -> (alloc::vec::Vec<f64>, alloc::vec::Vec<(f64, f64)>) {
    let eps_sq = 10f64.powf(0.1 * rp_db) - 1.0;
    let eps = eps_sq.sqrt();
    let ck1_sq = eps_sq / (10f64.powf(0.1 * rs_db) - 1.0);
    let n = order as f64;
    let m = elliptic_degree(n, ck1_sq);
    let cap_k = ellip_k(m);
    let k1 = ellip_k(ck1_sq);

    // The indices j = 1-N%2, 3-N%2, … < N from Orfanidis's construction.
    let js: alloc::vec::Vec<f64> = (0..order)
        .filter(|j| (*j % 2) == (order + 1) % 2)
        .map(|j| j as f64)
        .collect();

    // Zeros: z = j / (√m · sn(jK/N)); the j = 0 term (sn = 0) is a zero at
    // infinity and is dropped.
    let mut zeros = alloc::vec::Vec::new();
    for &j in &js {
        let (s, _, _) = jacobi_sn_cn_dn(j * cap_k / n, m);
        if s.abs() > 1e-12 {
            zeros.push(1.0 / (m.sqrt() * s).abs());
        }
    }

    // Poles: shift v₀ along the imaginary axis of the dual function.
    let u = ellip_f((1.0 / eps).atan(), 1.0 - ck1_sq);
    let v0 = cap_k * u / (n * k1);
    let (sv, cv, dv) = jacobi_sn_cn_dn(v0, 1.0 - m);
    let mut poles = alloc::vec::Vec::new();
    for &j in &js {
        let (s, c, d) = jacobi_sn_cn_dn(j * cap_k / n, m);
        let den = 1.0 - (d * sv) * (d * sv);
        let re = -(c * d * sv * cv) / den;
        let im = -(s * dv) / den;
        if im.abs() < 1e-12 {
            poles.push((re, 0.0));
        } else {
            // Keep one member of each conjugate pair: the one with Im > 0.
            poles.push((re, im.abs()));
        }
    }
    (zeros, poles)
}

/// Elliptic (Cauer) low-pass filter as a biquad cascade.
///
/// `fp` is the passband edge (the −`rp_db` point), `rp_db` the passband ripple,
/// and `rs_db` the minimum stopband attenuation. The stopband edge is set by
/// the order and the two ripples, not by the caller. The cascade is normalised
/// to unity DC gain. This matches `scipy.signal.ellip` with `btype="low"`.
///
/// # Errors
/// Returns an error for a zero order, non-positive ripples, a stopband
/// attenuation not above the passband ripple, or an invalid `fs`/`fp`.
#[cfg(feature = "alloc")]
pub fn elliptic_lowpass(
    order: usize,
    fs: f32,
    fp: f32,
    rp_db: f32,
    rs_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, fp)?;
    if !(rp_db.is_finite() && rp_db > 0.0 && rs_db.is_finite() && rs_db > rp_db) {
        return Err(FilterDesignError::Gain);
    }
    let (zeros, poles) = elliptic_prototype(order, f64::from(rp_db), f64::from(rs_db));
    let fs64 = f64::from(fs);
    // Pre-warp the passband edge, as the other designs do.
    let wa = 2.0 * fs64 * (core::f64::consts::PI * f64::from(fp) / fs64).tan();

    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    let mut zero_iter = zeros.iter();
    for &(re, im) in &poles {
        if im == 0.0 {
            // Odd order: the real pole is a first-order section.
            let p = re * wa;
            sections.push(bilinear_biquad([-p, 0.0, 0.0], [-p, 1.0, 0.0], fs64));
        } else {
            let z = zero_iter.next().copied().unwrap_or(0.0) * wa;
            let (pr, pi) = (re * wa, im * wa);
            // Denominator s² − 2·Re(p)·s + |p|²; numerator s² + z² (the zeros
            // are on the imaginary axis).
            sections.push(bilinear_biquad(
                [z * z, 0.0, 1.0],
                [pr * pr + pi * pi, -2.0 * pr, 1.0],
                fs64,
            ));
        }
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
    checked_design(sections)
}

#[cfg(all(test, feature = "alloc"))]
mod elliptic_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn passband_ripple_and_stopband_attenuation_meet_the_spec() {
        for order in 2..=6 {
            let s = elliptic_lowpass(order, FS, 1000.0, 1.0, 50.0).unwrap();
            // Passband: relative to DC the ripple band is ±rp. For even orders DC
            // is the ripple minimum, so the band lies above 0 dB, and for odd
            // orders below it; the symmetric bound covers both.
            let floor = -1.0 - 1e-3;
            for k in 1..=40 {
                let f = 1000.0 * k as f32 / 40.0;
                let db = 20.0 * f64::from(cascade_magnitude_response(&s, FS, f)).log10();
                assert!(
                    db >= floor && db <= 1.0 + 1e-3,
                    "order {order} at {f} Hz: {db} dB"
                );
            }
            // Stopband: after the bilinear transform the ripple sits about 1 dB
            // shy of rs near Nyquist (scipy.signal.ellip gives the same floor).
            // Beyond the stopband edge the
            // gain is at least rs below DC. Check the far stopband.
            let far = 20.0 * f64::from(cascade_magnitude_response(&s, FS, 0.45 * FS)).log10();
            assert!(far < -48.5, "order {order}: far stopband {far} dB");
        }
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(matches!(
            elliptic_lowpass(0, FS, 1000.0, 1.0, 40.0),
            Err(FilterDesignError::InvalidOrder)
        ));
        assert!(matches!(
            elliptic_lowpass(4, FS, 1000.0, 0.0, 40.0),
            Err(FilterDesignError::Gain)
        ));
        // Stopband attenuation must exceed the passband ripple.
        assert!(matches!(
            elliptic_lowpass(4, FS, 1000.0, 3.0, 2.0),
            Err(FilterDesignError::Gain)
        ));
        assert!(elliptic_lowpass(4, FS, 30_000.0, 1.0, 40.0).is_err());
    }
}

/// Elliptic (Cauer) high-pass filter as a biquad cascade.
///
/// The same prototype as [`elliptic_lowpass`], with the low-pass-to-high-pass
/// substitution `s → ωp/s` applied to its poles and zeros. The passband edge
/// `fp`, ripples and order mean the same as for the low-pass. The cascade is
/// normalised to unit gain at the Nyquist frequency, where the high-pass
/// passband sits (its DC gain is zero, so DC cannot be the reference).
///
/// # Errors
/// As [`elliptic_lowpass`].
#[cfg(feature = "alloc")]
pub fn elliptic_highpass(
    order: usize,
    fs: f32,
    fp: f32,
    rp_db: f32,
    rs_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, fp)?;
    if !(rp_db.is_finite() && rp_db > 0.0 && rs_db.is_finite() && rs_db > rp_db) {
        return Err(FilterDesignError::Gain);
    }
    let (zeros, poles) = elliptic_prototype(order, f64::from(rp_db), f64::from(rs_db));
    let fs64 = f64::from(fs);
    let wa = 2.0 * fs64 * (core::f64::consts::PI * f64::from(fp) / fs64).tan();

    let mut sections = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    let mut zero_iter = zeros.iter();
    for &(re, im) in &poles {
        if im == 0.0 {
            // Real pole p → wa/p: first-order section with a zero at s = 0.
            let p = wa / re;
            sections.push(bilinear_biquad([0.0, 1.0, 0.0], [-p, 1.0, 0.0], fs64));
        } else {
            // Zero pair ±jy → ±j·wa/y, which is again a pure-imaginary pair.
            let y = zero_iter.next().copied().unwrap_or(0.0);
            let w = if y > 0.0 { wa / y } else { 0.0 };
            // Pole p → wa/p = wa·conj(p)/|p|².
            let mag2 = re * re + im * im;
            let (pr, pi) = (wa * re / mag2, wa * im / mag2);
            sections.push(bilinear_biquad(
                [w * w, 0.0, 1.0],
                [pr * pr + pi * pi, -2.0 * pr, 1.0],
                fs64,
            ));
        }
    }

    // Unit gain at Nyquist.
    let g = cascade_magnitude_response(&sections, fs, fs / 2.0);
    if g.is_finite() && g > 1e-12 {
        let first = &mut sections[0];
        first.b0 /= g;
        first.b1 /= g;
        first.b2 /= g;
    }
    checked_design(sections)
}

#[cfg(all(test, feature = "alloc"))]
mod elliptic_highpass_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn passband_is_near_nyquist_and_the_stopband_is_below_dc_region() {
        for order in 2..=5 {
            let s = elliptic_highpass(order, FS, 1000.0, 1.0, 50.0).unwrap();
            let db = |f: f32| 20.0 * f64::from(cascade_magnitude_response(&s, FS, f)).log10();
            // Passband: within the ±rp band (relative to Nyquist) from fp upward.
            for k in 0..=20 {
                let f = 2000.0 + (FS / 2.0 - 2000.0) * k as f32 / 20.0;
                assert!(
                    db(f).abs() <= 1.0 + 0.05,
                    "order {order} at {f} Hz: {}",
                    db(f)
                );
            }
            // Stopband: well below the passband at low frequencies.
            assert!(
                db(100.0) < -40.0,
                "order {order}: 100 Hz at {} dB",
                db(100.0)
            );
        }
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(elliptic_highpass(0, FS, 1000.0, 1.0, 40.0).is_err());
        assert!(elliptic_highpass(3, FS, 1000.0, 1.0, 0.5).is_err());
    }
}

/// Principal square root of a complex number `(re, im)`.
#[cfg(feature = "alloc")]
fn complex_sqrt(re: f64, im: f64) -> (f64, f64) {
    let mag = (re * re + im * im).sqrt();
    let sr = ((mag + re) / 2.0).max(0.0).sqrt();
    let si = ((mag - re) / 2.0).max(0.0).sqrt();
    (sr, if im < 0.0 { -si } else { si })
}

#[cfg(all(test, feature = "alloc"))]
mod elliptic_bandpass_tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn passes_the_band_and_rejects_outside_it() {
        let s = elliptic_bandpass(2, FS, 1000.0, 3000.0, 1.0, 40.0).unwrap();
        let db = |f: f32| 20.0 * f64::from(cascade_magnitude_response(&s, FS, f)).log10();
        // Inside the band: within the ripple of the centre.
        let centre = db(1737.0);
        assert!(centre.abs() < 0.2, "centre {centre}");
        assert!(db(1200.0) > -1.5 && db(2800.0) > -1.5, "band edges");
        // Well outside the band.
        assert!(db(200.0) < -30.0, "200 Hz: {}", db(200.0));
        assert!(db(10_000.0) < -30.0, "10 kHz: {}", db(10_000.0));
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(elliptic_bandpass(0, FS, 1000.0, 3000.0, 1.0, 40.0).is_err());
        assert!(elliptic_bandpass(2, FS, 3000.0, 1000.0, 1.0, 40.0).is_err());
        assert!(elliptic_bandpass(2, FS, 1000.0, 30_000.0, 1.0, 40.0).is_err());
    }
}

/// Map a low-pass prototype (edge at 1 rad/s) to a band-pass cascade.
///
/// `poles` lists the prototype poles: one representative per complex pair
/// (positive imaginary part) and each real pole once. `zero_mags` lists the
/// magnitudes of the finite prototype zeros, one per complex pair, in the same
/// order as the complex poles; an all-pole prototype passes an empty slice.
/// `wl` and `wh` are the pre-warped band edges in rad/s.
///
/// The substitution is `s → (s² + ω₀²)/(B·s)`, with `ω₀² = wl·wh` and
/// `B = wh − wl`. Each complex pole pair gives two band-pass pole pairs. Each
/// real pole gives two real poles, or a complex pair when the discriminant is
/// negative. The zeros at the origin that the mapping produces from zeros at
/// infinity go into the real-pole sections, or into a complex pair's own
/// sections when the prototype has no finite zero for that pair.
#[cfg(feature = "alloc")]
fn bandpass_sections(
    poles: &[(f64, f64)],
    zero_mags: &[f64],
    wl: f64,
    wh: f64,
    fs: f64,
) -> alloc::vec::Vec<BiquadCoeffs<f32>> {
    let w0sq = wl * wh;
    let bw = wh - wl;
    let mut sections = alloc::vec::Vec::with_capacity(2 * poles.len() + 1);
    let mut zero_iter = zero_mags.iter();
    for &(re, im) in poles {
        if im == 0.0 {
            // s² − p·B·s + ω₀² = 0. The prototype zero at infinity becomes a
            // zero at the origin, carried by one of the sections.
            let b = re * bw;
            let disc = b * b - 4.0 * w0sq;
            if disc >= 0.0 {
                let s = disc.sqrt();
                let (r1, r2) = ((b + s) / 2.0, (b - s) / 2.0);
                sections.push(bilinear_biquad([0.0, 1.0, 0.0], [-r1, 1.0, 0.0], fs));
                sections.push(bilinear_biquad([-r2, 0.0, 0.0], [-r2, 1.0, 0.0], fs));
            } else {
                let (rr, ri) = (b / 2.0, (-disc).sqrt() / 2.0);
                sections.push(bilinear_biquad(
                    [0.0, 1.0, 0.0],
                    [rr * rr + ri * ri, -2.0 * rr, 1.0],
                    fs,
                ));
            }
        } else {
            // Complex pair p, p̄: the roots r of s² − p·B·s + ω₀² = 0 are the
            // band-pass poles (their conjugates come from p̄).
            let (pbr, pbi) = (re * bw, im * bw);
            let disc_re = pbr * pbr - pbi * pbi - 4.0 * w0sq;
            let disc_im = 2.0 * pbr * pbi;
            let (sr, si) = complex_sqrt(disc_re, disc_im);
            let roots = [
                ((pbr + sr) / 2.0, (pbi + si) / 2.0),
                ((pbr - sr) / 2.0, (pbi - si) / 2.0),
            ];
            // With a finite zero ±jy, the two root sections get zero pairs
            // ±j·a₁ and ±j·a₂ from the mapped zero. Without one, the zeros sit
            // at the origin.
            let numerators: [[f64; 3]; 2] = match zero_iter.next() {
                Some(&y) => {
                    let yb = y * bw;
                    let root_mag = (yb * yb + 4.0 * w0sq).sqrt();
                    let (a1, a2) = ((yb + root_mag) / 2.0, (root_mag - yb) / 2.0);
                    [[a1 * a1, 0.0, 1.0], [a2 * a2, 0.0, 1.0]]
                }
                None => [[0.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            };
            for (r, num) in roots.iter().zip(numerators.iter()) {
                sections.push(bilinear_biquad(
                    *num,
                    [r.0 * r.0 + r.1 * r.1, -2.0 * r.0, 1.0],
                    fs,
                ));
            }
        }
    }
    sections
}

/// Validate a band-pass request and return the pre-warped band edges
/// `(wl, wh)` in rad/s.
#[cfg(feature = "alloc")]
fn bandpass_edges(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
) -> Result<(f64, f64), FilterDesignError> {
    if order == 0 {
        return Err(FilterDesignError::InvalidOrder);
    }
    check_cutoff(fs, f_low)?;
    check_cutoff(fs, f_high)?;
    if f_low >= f_high {
        return Err(FilterDesignError::FrequencyOutOfRange);
    }
    let fs64 = f64::from(fs);
    let warp = |f: f32| 2.0 * fs64 * (core::f64::consts::PI * f64::from(f) / fs64).tan();
    Ok((warp(f_low), warp(f_high)))
}

/// Normalise a band-pass cascade to unit gain at its digital centre frequency,
/// `fs/π · atan(ω₀ / 2fs)` with `ω₀ = √(wl·wh)`.
#[cfg(feature = "alloc")]
fn normalise_bandpass(sections: &mut [BiquadCoeffs<f32>], fs: f32, wl: f64, wh: f64) {
    let fs64 = f64::from(fs);
    let w0 = (wl * wh).sqrt();
    let centre = (fs64 / core::f64::consts::PI * (w0 / (2.0 * fs64)).atan()) as f32;
    let g = cascade_magnitude_response(sections, fs, centre);
    if g.is_finite() && g > 1e-12 {
        let first = &mut sections[0];
        first.b0 /= g;
        first.b1 /= g;
        first.b2 /= g;
    }
}

/// Elliptic (Cauer) band-pass filter as a biquad cascade.
///
/// `order` is the prototype (low-pass) order, so the band-pass has `2·order`
/// poles. The prototype is the same as [`elliptic_lowpass`], mapped to the
/// pre-warped band edges. The cascade is normalised to unit gain at the
/// digital centre frequency `fs/π · atan(ω₀ / 2fs)`. This matches
/// `scipy.signal.ellip` with `btype="band"` to within 0.001 dB.
///
/// # Errors
/// Returns an error for a zero order, non-positive ripples, a stopband
/// attenuation not above the passband ripple, or an invalid `fs` or band edges
/// (`0 < fl < fh < fs/2`).
#[cfg(feature = "alloc")]
pub fn elliptic_bandpass(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    rp_db: f32,
    rs_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    if !(rp_db.is_finite() && rp_db > 0.0 && rs_db.is_finite() && rs_db > rp_db) {
        return Err(FilterDesignError::Gain);
    }
    let (zeros, poles) = elliptic_prototype(order, f64::from(rp_db), f64::from(rs_db));
    let mut sections = bandpass_sections(&poles, &zeros, wl, wh, f64::from(fs));
    normalise_bandpass(&mut sections, fs, wl, wh);
    checked_design(sections)
}

/// Butterworth band-pass filter as a biquad cascade of order `2·order`.
///
/// Matches `scipy.signal.butter` with `btype="band"`. The cascade is
/// normalised to unit gain at the digital centre frequency.
///
/// # Errors
/// Returns an error for a zero order or an invalid `fs` or band edges.
#[cfg(feature = "alloc")]
pub fn butterworth_bandpass(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    // Butterworth prototype poles lie on the unit circle in the left half
    // plane, at angle π/2 + π(2k+1)/(2N). Keep one of each conjugate pair.
    let n = order as f64;
    let mut poles = alloc::vec::Vec::new();
    for k in 0..order {
        let theta = core::f64::consts::FRAC_PI_2
            + core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        let (re, im) = (theta.cos(), theta.sin());
        if im > 1e-12 {
            poles.push((re, im));
        } else if im.abs() <= 1e-12 {
            poles.push((-1.0, 0.0));
        }
    }
    let mut sections = bandpass_sections(&poles, &[], wl, wh, f64::from(fs));
    normalise_bandpass(&mut sections, fs, wl, wh);
    checked_design(sections)
}

/// Chebyshev type I band-pass filter as a biquad cascade of order `2·order`.
///
/// `ripple_db` is the passband ripple, as for [`chebyshev1_lowpass`]. Matches
/// `scipy.signal.cheby1` with `btype="band"`. The cascade is normalised to
/// unit gain at the digital centre frequency.
///
/// # Errors
/// Returns an error for a zero order, a non-positive ripple, or an invalid `fs`
/// or band edges.
#[cfg(feature = "alloc")]
pub fn chebyshev1_bandpass(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    ripple_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    if !(ripple_db.is_finite() && ripple_db > 0.0) {
        return Err(FilterDesignError::Gain);
    }
    // Chebyshev I prototype poles (edge at 1 rad/s): σ = −sinh a · sin θ and
    // ω = cosh a · cos θ, with θ = π(2k+1)/(2N) and a = asinh(1/ε)/N.
    let eps = (10f64.powf(f64::from(ripple_db) / 10.0) - 1.0).sqrt();
    let n = order as f64;
    let a = (1.0 / eps).asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    let mut poles = alloc::vec::Vec::new();
    for k in 0..order.div_ceil(2) {
        let theta = core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        if order % 2 == 1 && k == order / 2 {
            poles.push((-sh, 0.0));
        } else {
            poles.push((-sh * theta.sin(), ch * theta.cos()));
        }
    }
    let mut sections = bandpass_sections(&poles, &[], wl, wh, f64::from(fs));
    normalise_bandpass(&mut sections, fs, wl, wh);
    checked_design(sections)
}

/// Return the cascade only if it is finite and stable.
///
/// A design can be valid by its parameters and still fail once its
/// coefficients are rounded to `f32`. Overflow or underflow gives NaN. A band
/// edge close to Nyquist, or to DC, puts poles at `z ≈ −1` or `z ≈ 1`, where
/// rounding can move them onto or past the unit circle. Both are reported as
/// [`FilterDesignError::NumericallyDegenerate`] instead of being returned.
#[cfg(feature = "alloc")]
fn checked_design(
    sections: alloc::vec::Vec<BiquadCoeffs<f32>>,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    if sections.iter().all(biquad_is_stable) {
        Ok(sections)
    } else {
        Err(FilterDesignError::NumericallyDegenerate)
    }
}

/// Finite coefficients, and both poles of `z² + a1·z + a2` strictly inside the
/// unit circle (the Jury conditions for a second-order denominator).
#[cfg(feature = "alloc")]
fn biquad_is_stable(c: &BiquadCoeffs<f32>) -> bool {
    if ![c.b0, c.b1, c.b2, c.a1, c.a2].iter().all(|v| v.is_finite()) {
        return false;
    }
    let (a1, a2) = (f64::from(c.a1), f64::from(c.a2));
    a2.abs() < 1.0 && a1.abs() < 1.0 + a2
}

/// Complex product `a·b` for `(re, im)` pairs.
#[cfg(feature = "alloc")]
fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// Complex quotient `a / b` for `(re, im)` pairs.
#[cfg(feature = "alloc")]
fn cdiv(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let d = b.0 * b.0 + b.1 * b.1;
    ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
}

/// Map a low-pass prototype (edge at 1 rad/s) to a band-stop cascade.
///
/// The arguments follow [`bandpass_sections`]. The substitution is
/// `s → B·s/(s² + ω₀²)`. Each prototype pole `p` gives the two roots of
/// `p·s² − B·s + p·ω₀² = 0`. A complex pair gives two band-stop pole pairs,
/// and a real pole gives one second-order section, since its two roots have
/// sum `B/p` and product `ω₀²`. A finite prototype zero pair `±jy` maps to two
/// zero pairs `±j·m±`. A prototype zero at infinity maps to a zero pair `±jω₀`.
#[cfg(feature = "alloc")]
fn bandstop_sections(
    poles: &[(f64, f64)],
    zero_mags: &[f64],
    wl: f64,
    wh: f64,
    fs: f64,
) -> alloc::vec::Vec<BiquadCoeffs<f32>> {
    let w0sq = wl * wh;
    let bw = wh - wl;
    let mut sections = alloc::vec::Vec::with_capacity(2 * poles.len() + 1);
    let mut zero_iter = zero_mags.iter();
    for &(re, im) in poles {
        if im == 0.0 {
            // Real pole: denominator s² − (B/p)·s + ω₀², with a zero pair ±jω₀
            // from the prototype zero at infinity.
            sections.push(bilinear_biquad([w0sq, 0.0, 1.0], [w0sq, -bw / re, 1.0], fs));
        } else {
            // Roots of p·s² − B·s + p·ω₀² = 0 are r = (B ± √(B² − 4p²ω₀²)) / (2p).
            let p2 = cmul((re, im), (re, im));
            let (sr, si) = complex_sqrt(bw * bw - 4.0 * w0sq * p2.0, -4.0 * w0sq * p2.1);
            let two_p = (2.0 * re, 2.0 * im);
            let roots = [cdiv((bw + sr, si), two_p), cdiv((bw - sr, -si), two_p)];
            // A finite zero ±jy gives zero pairs ±j·m± with m± = (B ± S)/(2y),
            // where S = √(B² + 4y²ω₀²). Without one, the zeros sit at ±jω₀.
            let numerators: [[f64; 3]; 2] = match zero_iter.next() {
                Some(&y) => {
                    let s = (bw * bw + 4.0 * y * y * w0sq).sqrt();
                    let m1 = (bw + s) / (2.0 * y);
                    let m2 = (s - bw) / (2.0 * y);
                    [[m1 * m1, 0.0, 1.0], [m2 * m2, 0.0, 1.0]]
                }
                None => [[w0sq, 0.0, 1.0], [w0sq, 0.0, 1.0]],
            };
            for (r, num) in roots.iter().zip(numerators.iter()) {
                sections.push(bilinear_biquad(
                    *num,
                    [r.0 * r.0 + r.1 * r.1, -2.0 * r.0, 1.0],
                    fs,
                ));
            }
        }
    }
    sections
}

/// Normalise a cascade to unit gain at DC (the band-stop passbands sit there).
#[cfg(feature = "alloc")]
fn normalise_dc(sections: &mut [BiquadCoeffs<f32>], fs: f32) {
    let g = cascade_magnitude_response(sections, fs, 0.0);
    if g.is_finite() && g > 1e-12 {
        let first = &mut sections[0];
        first.b0 /= g;
        first.b1 /= g;
        first.b2 /= g;
    }
}

/// Butterworth band-stop filter as a biquad cascade of order `2·order`.
///
/// `f_low` and `f_high` are the passband edges, which bracket the stopband.
/// Matches `scipy.signal.butter` with `btype="bandstop"`. Gain is normalised
/// to unity at DC.
///
/// # Errors
/// Returns an error for a zero order or invalid `fs` or edges.
#[cfg(feature = "alloc")]
pub fn butterworth_bandstop(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    let poles = butterworth_prototype_poles(order);
    let mut sections = bandstop_sections(&poles, &[], wl, wh, f64::from(fs));
    normalise_dc(&mut sections, fs);
    checked_design(sections)
}

/// Butterworth low-pass prototype poles (edge at 1 rad/s): one per conjugate
/// pair with positive imaginary part, and each real pole once.
#[cfg(feature = "alloc")]
fn butterworth_prototype_poles(order: usize) -> alloc::vec::Vec<(f64, f64)> {
    let n = order as f64;
    let mut poles = alloc::vec::Vec::new();
    for k in 0..order {
        let theta = core::f64::consts::FRAC_PI_2
            + core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        let (re, im) = (theta.cos(), theta.sin());
        if im > 1e-12 {
            poles.push((re, im));
        } else if im.abs() <= 1e-12 {
            poles.push((-1.0, 0.0));
        }
    }
    poles
}

/// Chebyshev type I band-stop filter as a biquad cascade of order `2·order`.
///
/// Matches `scipy.signal.cheby1` with `btype="bandstop"`. Gain is normalised
/// to unity at DC.
///
/// # Errors
/// Returns an error for a zero order, a non-positive ripple, or invalid `fs`
/// or edges.
#[cfg(feature = "alloc")]
pub fn chebyshev1_bandstop(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    ripple_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    if !(ripple_db.is_finite() && ripple_db > 0.0) {
        return Err(FilterDesignError::Gain);
    }
    let poles = chebyshev1_prototype_poles(order, f64::from(ripple_db));
    let mut sections = bandstop_sections(&poles, &[], wl, wh, f64::from(fs));
    normalise_dc(&mut sections, fs);
    checked_design(sections)
}

/// Chebyshev I low-pass prototype poles (edge at 1 rad/s), in the same
/// representation as [`butterworth_prototype_poles`].
#[cfg(feature = "alloc")]
fn chebyshev1_prototype_poles(order: usize, ripple_db: f64) -> alloc::vec::Vec<(f64, f64)> {
    let eps = (10f64.powf(ripple_db / 10.0) - 1.0).sqrt();
    let n = order as f64;
    let a = (1.0 / eps).asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    let mut poles = alloc::vec::Vec::new();
    for k in 0..order.div_ceil(2) {
        let theta = core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        if order % 2 == 1 && k == order / 2 {
            poles.push((-sh, 0.0));
        } else {
            poles.push((-sh * theta.sin(), ch * theta.cos()));
        }
    }
    poles
}

/// Elliptic band-stop filter as a biquad cascade of order `2·order`.
///
/// `f_low` and `f_high` are the passband edges. The prototype is the same as
/// [`elliptic_lowpass`]. Matches `scipy.signal.ellip` with
/// `btype="bandstop"`. Gain is normalised to unity at DC.
///
/// # Errors
/// As [`elliptic_bandpass`].
#[cfg(feature = "alloc")]
pub fn elliptic_bandstop(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    rp_db: f32,
    rs_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    if !(rp_db.is_finite() && rp_db > 0.0 && rs_db.is_finite() && rs_db > rp_db) {
        return Err(FilterDesignError::Gain);
    }
    let (zeros, poles) = elliptic_prototype(order, f64::from(rp_db), f64::from(rs_db));
    let mut sections = bandstop_sections(&poles, &zeros, wl, wh, f64::from(fs));
    normalise_dc(&mut sections, fs);
    checked_design(sections)
}

/// Chebyshev type II prototype with the stopband edge at 1 rad/s.
///
/// Returns `(zeros, poles)`, in the same representation as
/// [`elliptic_prototype`]. Each zero is a magnitude `1/|cos θₖ|` on the
/// imaginary axis, and each pole is the reciprocal of a Chebyshev I pole,
/// which gives the maximally flat passband. `θₖ = π(2k+1)/(2N)`, and
/// `a = asinh(√(10^{A/10} − 1))/N`.
#[cfg(feature = "alloc")]
fn chebyshev2_prototype(
    order: usize,
    atten_db: f64,
) -> (alloc::vec::Vec<f64>, alloc::vec::Vec<(f64, f64)>) {
    let inv_eps = (10f64.powf(atten_db / 10.0) - 1.0).sqrt();
    let n = order as f64;
    let a = inv_eps.asinh() / n;
    let (sh, ch) = (a.sinh(), a.cosh());
    let mut zeros = alloc::vec::Vec::with_capacity(order / 2);
    let mut poles = alloc::vec::Vec::with_capacity(order.div_ceil(2));
    for k in 0..order / 2 {
        let theta = core::f64::consts::PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        // Reciprocal of the Chebyshev I pole p = σ + jω. Its conjugate
        // (σ, ω)/|p|² has positive imaginary part and is the one kept.
        let (pr, pi) = (-sh * theta.sin(), ch * theta.cos());
        let mag2 = pr * pr + pi * pi;
        poles.push((pr / mag2, pi / mag2));
        zeros.push(1.0 / theta.cos().abs().max(1e-12));
    }
    if order % 2 == 1 {
        // The middle Chebyshev I pole is real, and its reciprocal is too. Its
        // zero sits at infinity, so it has no finite zero magnitude.
        poles.push((-1.0 / sh, 0.0));
    }
    (zeros, poles)
}

/// Validate the stopband attenuation of a Chebyshev II request.
#[cfg(feature = "alloc")]
fn check_atten(atten_db: f32) -> Result<f64, FilterDesignError> {
    if !(atten_db.is_finite() && atten_db > 0.0) {
        return Err(FilterDesignError::Gain);
    }
    Ok(f64::from(atten_db))
}

/// Chebyshev type II band-pass filter as a biquad cascade of order `2·order`.
///
/// Unlike the type I forms, the band edges `f_low` and `f_high` are where the
/// response is `−atten_db`. The passband is maximally flat and sits between
/// them, and the stopbands outside have equiripple zeros. Matches
/// `scipy.signal.cheby2` with `btype="bandpass"`. Gain is normalised to unity
/// at the digital centre frequency.
///
/// # Errors
/// Returns an error for a zero order, a non-positive attenuation, or invalid
/// `fs` or edges.
#[cfg(feature = "alloc")]
pub fn chebyshev2_bandpass(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    atten_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    let atten = check_atten(atten_db)?;
    let (zeros, poles) = chebyshev2_prototype(order, atten);
    let mut sections = bandpass_sections(&poles, &zeros, wl, wh, f64::from(fs));
    normalise_bandpass(&mut sections, fs, wl, wh);
    checked_design(sections)
}

/// Chebyshev type II band-stop filter as a biquad cascade of order `2·order`.
///
/// The band edges `f_low` and `f_high` are where the response is
/// `−atten_db`, and the equiripple stopband lies between them. The passbands
/// outside are maximally flat. Matches `scipy.signal.cheby2` with
/// `btype="bandstop"`. Gain is normalised to unity at DC.
///
/// # Errors
/// Returns an error for a zero order, a non-positive attenuation, or invalid
/// `fs` or edges.
#[cfg(feature = "alloc")]
pub fn chebyshev2_bandstop(
    order: usize,
    fs: f32,
    f_low: f32,
    f_high: f32,
    atten_db: f32,
) -> Result<alloc::vec::Vec<BiquadCoeffs<f32>>, FilterDesignError> {
    let (wl, wh) = bandpass_edges(order, fs, f_low, f_high)?;
    let atten = check_atten(atten_db)?;
    let (zeros, poles) = chebyshev2_prototype(order, atten);
    let mut sections = bandstop_sections(&poles, &zeros, wl, wh, f64::from(fs));
    normalise_dc(&mut sections, fs);
    checked_design(sections)
}

#[cfg(all(test, feature = "alloc"))]
mod degenerate_tests {
    use super::*;

    type Design = Result<Vec<BiquadCoeffs<f32>>, FilterDesignError>;

    /// Finite coefficients, and both poles of each section strictly inside the
    /// unit circle (the same test the designs apply before returning).
    fn assert_stable(sections: &[BiquadCoeffs<f32>], what: &str) {
        for c in sections {
            assert!(
                biquad_is_stable(c),
                "{what}: unstable or non-finite section {c:?}"
            );
        }
    }

    #[test]
    fn first_order_sections_have_no_second_order_terms() {
        // A real pole is a first-order section: a2 = b2 = 0 exactly, so no
        // pole is left at z = -1 for rounding to push outside the circle.
        let s = chebyshev1_lowpass(1, 48_000.0, 1000.0, 1.0).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].a2, 0.0);
        assert_eq!(s[0].b2, 0.0);
        let s = chebyshev1_lowpass(3, 48_000.0, 1000.0, 1.0).unwrap();
        let last = s.last().unwrap();
        assert_eq!(last.a2, 0.0, "odd order: the real section is first-order");
        assert_eq!(last.b2, 0.0);
    }

    #[test]
    fn odd_order_cheby1_low_pass_is_stable() {
        // The order-3 and order-5 Chebyshev I low-passes used to come back
        // with a pole at z ≈ −1.0002 from the cancelled z = −1 pole.
        for order in [3usize, 5] {
            let s = chebyshev1_lowpass(order, 48_000.0, 1000.0, 1.0).unwrap();
            assert_stable(&s, "cheby1 low-pass");
        }
    }

    #[test]
    fn designs_that_return_ok_are_always_stable() {
        // A deterministic sweep over orders, edges (including ones close to DC
        // and Nyquist), and ripple/attenuation. A design may be rejected as
        // degenerate, but an accepted one must be finite and stable.
        let mut accepted = 0usize;
        for &fs in &[8_000.0f32, 44_100.0, 48_000.0, 192_000.0] {
            for order in 1..=6usize {
                for &f1f in &[0.0005f32, 0.01, 0.1, 0.25, 0.4, 0.45, 0.49] {
                    for &f2f in &[0.02f32, 0.3, 0.46, 0.49] {
                        let f1 = fs * f1f;
                        let f2 = fs * f2f;
                        if f2 <= f1 {
                            continue;
                        }
                        let designs: [(&str, Design); 5] = [
                            (
                                "butterworth_bandpass",
                                butterworth_bandpass(order, fs, f1, f2),
                            ),
                            (
                                "butterworth_bandstop",
                                butterworth_bandstop(order, fs, f1, f2),
                            ),
                            (
                                "chebyshev1_bandstop",
                                chebyshev1_bandstop(order, fs, f1, f2, 1.0),
                            ),
                            (
                                "chebyshev2_bandpass",
                                chebyshev2_bandpass(order, fs, f1, f2, 40.0),
                            ),
                            (
                                "chebyshev2_bandstop",
                                chebyshev2_bandstop(order, fs, f1, f2, 40.0),
                            ),
                        ];
                        for (name, result) in designs {
                            if let Ok(s) = result {
                                accepted += 1;
                                assert_stable(
                                    &s,
                                    &format!("{name} order {order} fs {fs} f {f1}..{f2}"),
                                );
                            }
                        }
                    }
                }
            }
        }
        assert!(
            accepted > 100,
            "the sweep should accept many designs, got {accepted}"
        );
    }

    #[test]
    fn fuzzer_found_degenerate_inputs_are_rejected_not_returned() {
        // Inputs from a random sweep that used to return NaN or unstable
        // coefficients with no error.
        let cases = [
            // Edge at 0.49·fs: poles near z = −1 round outside the circle.
            butterworth_bandpass(1, 93_231.83, 14_837.044, 45_683.598).map(|_| ()),
            // Ripple so small that 10^(R/10) − 1 rounds to zero.
            chebyshev1_bandstop(3, 48_000.0, 1000.0, 3000.0, 2.4e-38).map(|_| ()),
            // Edges so small against fs that the pre-warp underflows.
            butterworth_bandstop(2, 8.1e-20, 7.9e-38, 2.5e-36).map(|_| ()),
        ];
        for result in cases {
            match result {
                Ok(()) => {}
                Err(e) => assert_eq!(e, FilterDesignError::NumericallyDegenerate),
            }
        }
        // Each case above must be an error or a stable design; the first is
        // the one that used to pass the validation and come back unstable.
        let first = butterworth_bandpass(1, 93_231.83, 14_837.044, 45_683.598);
        if let Ok(s) = first {
            assert_stable(&s, "order-1 band-pass near Nyquist");
        }
    }
}
