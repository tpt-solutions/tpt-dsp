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

#[test]
fn remez_taps_match_scipy() {
    use tpt_dsp_core::{remez_fir, RemezBand};

    fn band(start: f32, end: f32, desired: f32, weight: f32) -> RemezBand {
        RemezBand {
            start,
            end,
            desired,
            weight,
        }
    }

    // Must match the specifications in tests/golden/generate_remez.py.
    let specs: [(&str, usize, Vec<RemezBand>); 3] = [
        (
            "lp31",
            31,
            vec![band(0.0, 0.2, 1.0, 1.0), band(0.3, 0.5, 0.0, 1.0)],
        ),
        (
            "lp41_weighted",
            41,
            vec![band(0.0, 0.2, 1.0, 1.0), band(0.3, 0.5, 0.0, 10.0)],
        ),
        (
            "bp35",
            35,
            vec![
                band(0.0, 0.1, 0.0, 5.0),
                band(0.2, 0.3, 1.0, 1.0),
                band(0.35, 0.5, 0.0, 5.0),
            ],
        ),
    ];

    let mut golden_taps: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for line in golden("remez.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let i: usize = f[1].parse().unwrap();
        let v: f64 = f[2].parse().unwrap();
        let row = golden_taps.entry(f[0].to_string()).or_default();
        if row.len() <= i {
            row.resize(i + 1, 0.0);
        }
        row[i] = v;
    }

    for (name, taps, bands) in &specs {
        let ours = remez_fir(*taps, bands).unwrap();
        let want = golden_taps
            .get(*name)
            .unwrap_or_else(|| panic!("no golden taps for {name}"));
        assert_eq!(ours.len(), want.len(), "{name}: tap count");
        let worst = ours
            .iter()
            .zip(want.iter())
            .map(|(a, b)| (f64::from(*a) - b).abs())
            .fold(0.0f64, f64::max);
        println!("{name}: worst tap error {worst:e}");
        // Low-pass designs agree to ~5e-6. The band-pass optimum is flat, so
        // the taps can differ by ~2e-4 while the weighted ripple agrees with
        // scipy's to within 0.3% (both ~0.0303 in every band, measured). The
        // tolerance is set to that flatness, not to the low-pass agreement.
        assert!(worst < 1e-3, "{name}: worst tap error {worst:e}");
    }
}

#[test]
fn elliptic_magnitude_matches_scipy() {
    use tpt_dsp_core::{cascade_magnitude_response, elliptic_lowpass};

    // Reference: tests/golden/generate_ellip.py (scipy.signal.ellip, low-pass,
    // fs 48 kHz, passband edge 1 kHz, rp 1 dB, rs 40 dB).
    let mut worst = 0.0f64;
    let mut rows = 0usize;
    for line in golden("ellip.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let order: usize = f[0].parse().unwrap();
        let fs: f32 = f[1].parse().unwrap();
        let fc: f32 = f[2].parse().unwrap();
        let rp: f32 = f[3].parse().unwrap();
        let rs: f32 = f[4].parse().unwrap();
        let freq: f32 = f[5].parse().unwrap();
        let want: f64 = f[6].parse().unwrap();

        let sections = elliptic_lowpass(order, fs, fc, rp, rs).unwrap();
        let got = 20.0 * f64::from(cascade_magnitude_response(&sections, fs, freq)).log10();
        let err = (got - want).abs();
        assert!(
            err < 0.01,
            "order {order} at {freq} Hz: got {got:.4} dB, scipy {want:.4} dB"
        );
        worst = worst.max(err);
        rows += 1;
    }
    println!("elliptic: {rows} points, worst |diff| {worst:.6} dB");
    assert!(rows == 45, "expected 45 reference points, read {rows}");
}

#[test]
fn elliptic_highpass_magnitude_matches_scipy() {
    use tpt_dsp_core::{cascade_magnitude_response, elliptic_highpass};

    // Reference: tests/golden/generate_ellip.py (scipy.signal.ellip, high-pass),
    // relative to the gain at Nyquist − 1 Hz, as the generator computes it.
    let mut worst = 0.0f64;
    let mut rows = 0usize;
    for line in golden("ellip_high.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let order: usize = f[0].parse().unwrap();
        let fs: f32 = f[1].parse().unwrap();
        let fc: f32 = f[2].parse().unwrap();
        let rp: f32 = f[3].parse().unwrap();
        let rs: f32 = f[4].parse().unwrap();
        let freq: f32 = f[5].parse().unwrap();
        let want: f64 = f[6].parse().unwrap();

        let sections = elliptic_highpass(order, fs, fc, rp, rs).unwrap();
        let db = |x: f32| 20.0 * f64::from(cascade_magnitude_response(&sections, fs, x)).log10();
        let got = db(freq) - db(fs / 2.0 - 1.0);
        let err = (got - want).abs();
        assert!(
            err < 0.01,
            "high-pass order {order} at {freq} Hz: got {got:.4} dB, scipy {want:.4} dB"
        );
        worst = worst.max(err);
        rows += 1;
    }
    println!("elliptic high-pass: {rows} points, worst |diff| {worst:.6} dB");
    assert!(rows == 36, "expected 36 reference points, read {rows}");
}

#[test]
fn elliptic_bandpass_magnitude_matches_scipy() {
    use tpt_dsp_core::{cascade_magnitude_response, elliptic_bandpass};

    // Reference: tests/golden/generate_ellip_band.py (scipy.signal.ellip,
    // band-pass 1–3 kHz, 48 kHz), relative to the digital centre frequency.
    let mut worst = 0.0f64;
    let mut rows = 0usize;
    let centre = |fs: f32, fl: f32, fh: f32| -> f32 {
        let fs = f64::from(fs);
        let wl = 2.0 * fs * (std::f64::consts::PI * f64::from(fl) / fs).tan();
        let wh = 2.0 * fs * (std::f64::consts::PI * f64::from(fh) / fs).tan();
        let w0 = (wl * wh).sqrt();
        (fs / std::f64::consts::PI * (w0 / (2.0 * fs)).atan()) as f32
    };
    for line in golden("ellip_band.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let order: usize = f[0].parse().unwrap();
        let fs: f32 = f[1].parse().unwrap();
        let fl: f32 = f[2].parse().unwrap();
        let fh: f32 = f[3].parse().unwrap();
        let rp: f32 = f[4].parse().unwrap();
        let rs: f32 = f[5].parse().unwrap();
        let freq: f32 = f[6].parse().unwrap();
        let want: f64 = f[7].parse().unwrap();

        let sections = elliptic_bandpass(order, fs, fl, fh, rp, rs).unwrap();
        let db = |x: f32| 20.0 * f64::from(cascade_magnitude_response(&sections, fs, x)).log10();
        let got = db(freq) - db(centre(fs, fl, fh));
        let err = (got - want).abs();
        assert!(
            err < 0.02,
            "band-pass order {order} at {freq} Hz: got {got:.4} dB, scipy {want:.4} dB"
        );
        worst = worst.max(err);
        rows += 1;
    }
    println!("elliptic band-pass: {rows} points, worst |diff| {worst:.6} dB");
    assert!(rows == 27, "expected 27 reference points, read {rows}");
}

#[test]
fn butterworth_and_chebyshev1_bandpass_match_scipy() {
    use tpt_dsp_core::{butterworth_bandpass, cascade_magnitude_response, chebyshev1_bandpass};

    // Reference: tests/golden/generate_bandpass.py (scipy.signal.butter and
    // cheby1, band-pass 1–3 kHz, 48 kHz), relative to the digital centre.
    let centre = |fs: f32, fl: f32, fh: f32| -> f32 {
        let fs = f64::from(fs);
        let wl = 2.0 * fs * (std::f64::consts::PI * f64::from(fl) / fs).tan();
        let wh = 2.0 * fs * (std::f64::consts::PI * f64::from(fh) / fs).tan();
        (fs / std::f64::consts::PI * ((wl * wh).sqrt() / (2.0 * fs)).atan()) as f32
    };
    let mut worst = 0.0f64;
    let mut rows = 0usize;
    for line in golden("bandpass.csv").lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        let kind = f[0];
        let order: usize = f[1].parse().unwrap();
        let fs: f32 = f[2].parse().unwrap();
        let fl: f32 = f[3].parse().unwrap();
        let fh: f32 = f[4].parse().unwrap();
        let ripple: f32 = f[5].parse().unwrap();
        let freq: f32 = f[6].parse().unwrap();
        let want: f64 = f[7].parse().unwrap();

        let sections = match kind {
            "butter" => butterworth_bandpass(order, fs, fl, fh).unwrap(),
            "cheby1" => chebyshev1_bandpass(order, fs, fl, fh, ripple).unwrap(),
            other => panic!("unknown kind `{other}`"),
        };
        let db = |x: f32| 20.0 * f64::from(cascade_magnitude_response(&sections, fs, x)).log10();
        let got = db(freq) - db(centre(fs, fl, fh));
        let err = (got - want).abs();
        assert!(
            err < 0.02,
            "{kind} band-pass order {order} at {freq} Hz: got {got:.4} dB, scipy {want:.4} dB"
        );
        worst = worst.max(err);
        rows += 1;
    }
    println!("butter/cheby1 band-pass: {rows} points, worst |diff| {worst:.6} dB");
    assert!(rows == 54, "expected 54 reference points, read {rows}");
}
