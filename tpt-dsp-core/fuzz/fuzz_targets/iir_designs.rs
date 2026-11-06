#![no_main]

// Drives the IIR design functions with arbitrary orders, sample rates, edges
// and ripple/attenuation values. Every design must either return an error or
// produce finite, stable coefficients. None may panic.

use libfuzzer_sys::fuzz_target;
use tpt_dsp_core::{
    butterworth_bandpass, butterworth_bandstop, chebyshev1_bandstop, chebyshev2_bandpass,
    chebyshev2_bandstop, chebyshev2_lowpass, elliptic_bandpass, elliptic_bandstop, BiquadCoeffs,
};

fn check(result: Result<Vec<BiquadCoeffs<f32>>, tpt_dsp_core::FilterDesignError>) {
    if let Ok(sections) = result {
        for c in &sections {
            // Finite, and both poles strictly inside the unit circle (Jury).
            assert!(
                [c.b0, c.b1, c.b2, c.a1, c.a2].iter().all(|v| v.is_finite()),
                "non-finite coefficients from a successful design: {c:?}"
            );
            assert!(
                f64::from(c.a2).abs() < 1.0 && f64::from(c.a1).abs() < 1.0 + f64::from(c.a2),
                "unstable section from a successful design: {c:?}"
            );
        }
    }
}

fuzz_target!(|data: &[u8]| {
    // 1 byte order, 1 byte design kind, then four little-endian f32 values
    // (fs, first edge, second edge, ripple or attenuation).
    if data.len() < 18 {
        return;
    }
    let order = usize::from(data[0] % 9);
    let f = |i: usize| f32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]);
    let (fs, f1, f2, par) = (f(2), f(6), f(10), f(14));
    match data[1] % 6 {
        0 => check(butterworth_bandpass(order, fs, f1, f2)),
        1 => check(butterworth_bandstop(order, fs, f1, f2)),
        2 => check(chebyshev1_bandstop(order, fs, f1, f2, par)),
        3 => check(chebyshev2_bandpass(order, fs, f1, f2, par)),
        4 => check(chebyshev2_bandstop(order, fs, f1, f2, par)),
        _ => {
            check(elliptic_bandpass(order, fs, f1, f2, par, f2));
            check(elliptic_bandstop(order, fs, f1, f2, par, f2));
            check(chebyshev2_lowpass(order, fs, f1, par));
        }
    }
});
