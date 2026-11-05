//! Golden-vector conformance tests.
//!
//! The reference values in `tests/golden/*.csv` were produced by scipy (see
//! `tests/golden/generate.py`), not by this crate. These tests check that the
//! windows and filter designs match an independent implementation.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tpt_dsp_core::{
    butterworth, cascade_magnitude_response, chebyshev1_lowpass, chebyshev2_lowpass,
    windowed_symmetric_f64, Response, WindowType,
};

fn golden(name: &str) -> String {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "golden", name]
        .iter()
        .collect();
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn window_for(name: &str) -> WindowType {
    match name {
        "hann" => WindowType::Hann,
        "hamming" => WindowType::Hamming,
        "blackman" => WindowType::Blackman,
        "flattop" => WindowType::FlatTop,
        "kaiser8" => WindowType::Kaiser(8.0),
        "tukey0.5" => WindowType::Tukey(0.5),
        "gaussian0.3" => WindowType::Gaussian(0.3),
        other => panic!("unknown window `{other}` in golden file"),
    }
}

#[test]
fn symmetric_windows_match_scipy() {
    // (window, n) -> values by index.
    let mut table: BTreeMap<(String, usize), Vec<f64>> = BTreeMap::new();
    for line in golden("windows.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let n: usize = f[1].parse().unwrap();
        let i: usize = f[2].parse().unwrap();
        let v: f64 = f[3].parse().unwrap();
        let row = table
            .entry((f[0].to_string(), n))
            .or_insert_with(|| vec![0.0; n]);
        row[i] = v;
    }
    assert!(!table.is_empty(), "no window rows read");

    let mut worst = 0.0f64;
    for ((name, n), want) in &table {
        let mut got = vec![0.0f64; *n];
        windowed_symmetric_f64(window_for(name), *n, &mut got);
        for (i, (g, w)) in got.iter().zip(want.iter()).enumerate() {
            let err = (g - w).abs();
            worst = worst.max(err);
            // Window parameters (sigma, beta, alpha) are stored as f32, so the
            // Gaussian's 0.3 is 0.3 + ~1e-8. That gives about 2e-9 in the
            // samples; 1e-7 is still far tighter than any design tolerance.
            assert!(err < 1e-7, "{name} n={n} i={i}: got {g}, scipy {w}");
        }
    }
    println!("windows: {} cases, worst abs error {worst:e}", table.len());
}

#[derive(Default)]
struct Worst {
    err_db: f64,
    at: String,
}

#[test]
fn iir_designs_match_scipy_magnitude() {
    let fs = 48_000.0f32;
    let mut worst: BTreeMap<String, Worst> = BTreeMap::new();
    let mut rows = 0usize;

    for line in golden("filters.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let kind = f[0];
        let order: usize = f[1].parse().unwrap();
        let fc: f32 = f[3].parse().unwrap();
        let param: f32 = f[4].parse().unwrap();
        let freq: f32 = f[5].parse().unwrap();
        let want_db: f64 = f[6].parse().unwrap();

        let sections = match kind {
            "butter" => butterworth(Response::LowPass, order, fs, fc).unwrap(),
            "cheby1" => chebyshev1_lowpass(order, fs, fc, param).unwrap(),
            "cheby2" => chebyshev2_lowpass(order, fs, fc, param).unwrap(),
            other => panic!("unknown filter kind `{other}`"),
        };
        // Our designs are normalised to unity DC, and the reference is
        // relative to its own DC, so compare the dB values directly.
        let got_db = 20.0 * f64::from(cascade_magnitude_response(&sections, fs, freq)).log10();
        let err = (got_db - want_db).abs();
        let entry = worst.entry(format!("{kind} order {order}")).or_default();
        if err > entry.err_db {
            entry.err_db = err;
            entry.at = format!("{freq} Hz: got {got_db:.4} dB, scipy {want_db:.4} dB");
        }
        rows += 1;
    }

    for (name, w) in &worst {
        println!("{name}: worst {:.4} dB at {}", w.err_db, w.at);
        // Measured worst case is about 0.0002 dB; 0.001 dB leaves room for
        // f32 coefficient rounding and still catches a real design error.
        assert!(
            w.err_db < 0.001,
            "{name}: magnitude off by {:.4} dB ({})",
            w.err_db,
            w.at
        );
    }
    assert!(rows > 0, "no filter rows read");
}
