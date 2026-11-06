//! Filter designer back end for the browser (`www/designer.html`).
//!
//! The design itself is plain Rust in [`FilterSpec::design`], so it is tested
//! natively. The `#[wasm_bindgen]` methods only convert arguments and errors.
//!
//! Frequencies are in Hz. For a low-pass or high-pass, `f1` is the cutoff (for
//! the Chebyshev II low-pass, the stopband edge). Band shapes use `f1` as the
//! lower edge and `f2` as the upper edge. `ripple_db` is the passband ripple
//! (Chebyshev I, elliptic). `atten_db` is the stopband attenuation (Chebyshev
//! II, elliptic).

use tpt_dsp_core::{
    butterworth, butterworth_bandpass, butterworth_bandstop, cascade_magnitude_response,
    chebyshev1_bandpass, chebyshev1_bandstop, chebyshev1_lowpass, chebyshev2_bandpass,
    chebyshev2_bandstop, chebyshev2_lowpass, elliptic_bandpass, elliptic_bandstop,
    elliptic_highpass, elliptic_lowpass, BiquadCoeffs, Response,
};
use wasm_bindgen::prelude::*;

/// A filter request from the designer page.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq)]
pub struct FilterSpec {
    family: String,
    shape: String,
    order: u32,
    fs: f32,
    f1: f32,
    f2: f32,
    ripple_db: f32,
    atten_db: f32,
}

/// Magnitude in dB reported for a response that is exactly zero.
const DB_FLOOR: f32 = -300.0;

#[wasm_bindgen]
impl FilterSpec {
    /// A spec for `family` (`butterworth`, `chebyshev1`, `chebyshev2`,
    /// `elliptic`) and `shape` (`lowpass`, `highpass`, `bandpass`, `bandstop`)
    /// at sample rate `fs`. The edges default to 1 kHz and 3 kHz, 1 dB ripple
    /// and 40 dB attenuation.
    #[wasm_bindgen(constructor)]
    pub fn new(family: &str, shape: &str, order: u32, fs: f32) -> FilterSpec {
        FilterSpec {
            family: family.to_owned(),
            shape: shape.to_owned(),
            order,
            fs,
            f1: 1_000.0,
            f2: 3_000.0,
            ripple_db: 1.0,
            atten_db: 40.0,
        }
    }

    /// Set the edges. `f2` is ignored for low-pass and high-pass shapes.
    pub fn set_edges(&mut self, f1: f32, f2: f32) {
        self.f1 = f1;
        self.f2 = f2;
    }

    /// Set the passband ripple (Chebyshev I, elliptic) and the stopband
    /// attenuation (Chebyshev II, elliptic), in dB.
    pub fn set_ripple(&mut self, ripple_db: f32, atten_db: f32) {
        self.ripple_db = ripple_db;
        self.atten_db = atten_db;
    }

    /// Magnitude response in dB at each frequency in `freqs_hz`, relative to
    /// the design's own gain. Errors name the unavailable combination or the
    /// invalid parameter.
    pub fn response_db(&self, freqs_hz: &[f32]) -> Result<Vec<f32>, JsValue> {
        let sections = self.design().map_err(|e| JsValue::from_str(&e))?;
        Ok(response_db(&sections, self.fs, freqs_hz))
    }

    /// The biquad sections as a flat array, five values per section:
    /// `b0, b1, b2, a1, a2`, with `a0` normalised to 1 and the recurrence
    /// `y = b0·x + b1·x₋₁ + b2·x₋₂ − a1·y₋₁ − a2·y₋₂`.
    pub fn coefficients(&self) -> Result<Vec<f32>, JsValue> {
        let sections = self.design().map_err(|e| JsValue::from_str(&e))?;
        Ok(flatten(&sections))
    }

    /// The number of biquad sections the design produces.
    pub fn section_count(&self) -> Result<usize, JsValue> {
        self.design()
            .map(|s| s.len())
            .map_err(|e| JsValue::from_str(&e))
    }
}

impl FilterSpec {
    /// The biquad cascade for this request, or the reason it cannot be built.
    fn design(&self) -> Result<Vec<BiquadCoeffs<f32>>, String> {
        let n = self.order as usize;
        let (fs, f1, f2) = (self.fs, self.f1, self.f2);
        let (rp, rs) = (self.ripple_db, self.atten_db);
        let result = match (self.family.as_str(), self.shape.as_str()) {
            ("butterworth", "lowpass") => butterworth(Response::LowPass, n, fs, f1),
            ("butterworth", "highpass") => butterworth(Response::HighPass, n, fs, f1),
            ("butterworth", "bandpass") => butterworth_bandpass(n, fs, f1, f2),
            ("butterworth", "bandstop") => butterworth_bandstop(n, fs, f1, f2),
            ("chebyshev1", "lowpass") => chebyshev1_lowpass(n, fs, f1, rp),
            ("chebyshev1", "bandpass") => chebyshev1_bandpass(n, fs, f1, f2, rp),
            ("chebyshev1", "bandstop") => chebyshev1_bandstop(n, fs, f1, f2, rp),
            ("chebyshev2", "lowpass") => chebyshev2_lowpass(n, fs, f1, rs),
            ("chebyshev2", "bandpass") => chebyshev2_bandpass(n, fs, f1, f2, rs),
            ("chebyshev2", "bandstop") => chebyshev2_bandstop(n, fs, f1, f2, rs),
            ("elliptic", "lowpass") => elliptic_lowpass(n, fs, f1, rp, rs),
            ("elliptic", "highpass") => elliptic_highpass(n, fs, f1, rp, rs),
            ("elliptic", "bandpass") => elliptic_bandpass(n, fs, f1, f2, rp, rs),
            ("elliptic", "bandstop") => elliptic_bandstop(n, fs, f1, f2, rp, rs),
            _ => return Err(format!("{} {} is not available", self.family, self.shape)),
        };
        result.map_err(|e| e.to_string())
    }
}

/// Magnitude in dB of a cascade at each frequency, with an exact zero reported
/// as [`DB_FLOOR`] instead of −∞.
fn response_db(sections: &[BiquadCoeffs<f32>], fs: f32, freqs_hz: &[f32]) -> Vec<f32> {
    freqs_hz
        .iter()
        .map(|&f| {
            let m = cascade_magnitude_response(sections, fs, f);
            if m > 0.0 {
                (20.0 * m.log10()).max(DB_FLOOR)
            } else {
                DB_FLOOR
            }
        })
        .collect()
}

fn flatten(sections: &[BiquadCoeffs<f32>]) -> Vec<f32> {
    sections
        .iter()
        .flat_map(|c| [c.b0, c.b1, c.b2, c.a1, c.a2])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(family: &str, shape: &str, order: u32) -> FilterSpec {
        FilterSpec::new(family, shape, order, 48_000.0)
    }

    #[test]
    fn butterworth_lowpass_is_minus_three_db_at_cutoff() {
        let mut s = spec("butterworth", "lowpass", 2);
        s.set_edges(1_000.0, 0.0);
        let db = s.response_db(&[1_000.0]).unwrap();
        assert!((db[0] + 3.01).abs() < 0.05, "{}", db[0]);
    }

    #[test]
    fn highpass_passes_nyquist_and_blocks_dc() {
        let mut s = spec("butterworth", "highpass", 4);
        s.set_edges(1_000.0, 0.0);
        let db = s.response_db(&[10.0, 23_000.0]).unwrap();
        assert!(db[0] < -60.0, "DC {}", db[0]);
        assert!(db[1].abs() < 0.1, "Nyquist-ish {}", db[1]);
    }

    #[test]
    fn unavailable_combinations_are_named() {
        let s = spec("chebyshev1", "highpass", 3);
        let err = s.design().unwrap_err();
        assert_eq!(err, "chebyshev1 highpass is not available");
    }

    #[test]
    fn invalid_parameters_are_reported_not_panicked() {
        // A cutoff above Nyquist is rejected by the design.
        let mut s = spec("butterworth", "lowpass", 2);
        s.set_edges(30_000.0, 0.0);
        assert!(s.design().is_err());
        // Order zero is rejected.
        assert!(spec("butterworth", "lowpass", 0).design().is_err());
    }

    #[test]
    fn coefficients_are_five_per_section() {
        let mut s = spec("elliptic", "bandpass", 3);
        s.set_edges(1_000.0, 3_000.0);
        let sections = s.design().unwrap();
        assert_eq!(flatten(&sections).len(), 5 * sections.len());
        assert_eq!(s.section_count().unwrap(), sections.len());
    }

    #[test]
    fn bandstop_has_a_notch_and_unity_dc() {
        let mut s = spec("chebyshev2", "bandstop", 3);
        s.set_edges(1_000.0, 3_000.0);
        let db = s.response_db(&[0.0001, 2_000.0]).unwrap();
        assert!(db[0].abs() < 0.1, "DC {}", db[0]);
        assert!(db[1] < -35.0, "stopband centre {}", db[1]);
    }
}
