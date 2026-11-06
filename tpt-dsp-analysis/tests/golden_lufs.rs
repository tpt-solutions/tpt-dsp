//! Integrated loudness against pyloudnorm (an implementation of ITU-R BS.1770).
//!
//! The reference values come from `tests/golden/generate_lufs.py`, which uses
//! pyloudnorm's BS.1770 `"DeMan"` class. The signals
//! here are built with the same formulas as that script.

use std::path::PathBuf;

use tpt_dsp_analysis::LoudnessMeter;

const FS: f32 = 48_000.0;

fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
    let n = (FS * secs) as usize;
    (0..n)
        .map(|i| amp * (std::f32::consts::TAU * freq * i as f32 / FS).sin())
        .collect()
}

fn signal(name: &str) -> Vec<f32> {
    match name {
        "tone1k_quiet" => sine(1000.0, 0.1, 10.0),
        "tone1k_loud" => sine(1000.0, 0.5, 10.0),
        "tone60_low" => sine(60.0, 0.2, 10.0),
        "gated_loud_then_quiet" => {
            let mut v = sine(250.0, 0.3, 5.0);
            v.extend(sine(4000.0, 0.03, 5.0));
            v
        }
        other => panic!("unknown signal `{other}`"),
    }
}

#[test]
fn integrated_loudness_matches_pyloudnorm() {
    let path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "tests", "golden", "lufs.csv"]
        .iter()
        .collect();
    let text = std::fs::read_to_string(&path).expect("read lufs.csv");

    let mut worst = 0.0f64;
    let mut rows = 0;
    for line in text.lines().skip(1) {
        let mut f = line.split(',');
        let name = f.next().unwrap();
        let want: f64 = f.next().unwrap().parse().unwrap();

        let mut meter = LoudnessMeter::new(FS);
        meter.push(&signal(name));
        let got = meter.integrated().expect("signal is above the gate");
        let err = (got - want).abs();
        println!("{name}: ours {got:.3} LUFS, pyloudnorm {want:.3} LUFS, |diff| {err:.4}");
        worst = worst.max(err);
        rows += 1;
    }
    assert!(rows == 4, "expected 4 reference signals, read {rows}");
    // Measured worst case is 0.011 LU, on the mixed-level programme where a
    // block sits on a gate edge. 0.05 LU leaves room for that and nothing more.
    assert!(worst < 0.05, "worst |diff| {worst:.4} LU");
}
