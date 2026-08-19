//! `tpt-dsp-core` quick tour: design a low-pass biquad, filter a mixed signal,
//! then locate the surviving tone with an FFT.
//!
//! ```text
//! cargo run -p tpt-dsp-core --example biquad_lowpass
//! ```

use tpt_dsp_core::{next_power_of_two, Biquad, BiquadType, Complex32, FftPlan};

fn main() {
    let fs = 48_000.0f32;

    // Keep everything below 2 kHz: a 1 kHz tone plus a 6 kHz interferer.
    let mut lp = Biquad::<f32>::design(BiquadType::LowPass, fs, 2_000.0, 0.707, 0.0);

    let n = 2048;
    let mut mixed = vec![0.0f32; n];
    for (i, s) in mixed.iter_mut().enumerate() {
        let t = i as f32 / fs;
        *s = (2.0 * std::f32::consts::PI * 1_000.0 * t).sin()
            + 0.5 * (2.0 * std::f32::consts::PI * 6_000.0 * t).sin();
    }

    let mut filtered = vec![0.0f32; n];
    lp.process(&mixed, &mut filtered);

    // Transform and locate the dominant one-sided bin.
    let len = next_power_of_two(n);
    let mut buf: Vec<Complex32> = filtered.iter().map(|&x| Complex32::new(x, 0.0)).collect();
    buf.resize(len, Complex32::default());
    let mut spectrum = vec![Complex32::default(); len];
    let mut plan = FftPlan::new_forward(len);
    plan.process(&buf, &mut spectrum);

    let mut peak = 0usize;
    let mut peak_mag = 0.0f32;
    for (k, c) in spectrum.iter().enumerate().take(len / 2 + 1) {
        let m = c.norm();
        if m > peak_mag {
            peak_mag = m;
            peak = k;
        }
    }
    let freq = peak as f32 * fs / len as f32;
    println!("dominant frequency after low-pass: {freq:.1} Hz (bin {peak})");
}
