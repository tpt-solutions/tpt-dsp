//! `tpt-dsp-audio` tour: an FM synthesis voice shaped by a 3-band biquad EQ.
//!
//! ```text
//! cargo run -p tpt-dsp-audio --example synth_eq
//! ```

use tpt_dsp_audio::{Eq, FmSynth};

fn main() {
    let fs = 48_000.0f32;

    let mut fm = FmSynth::new(fs, 220.0, 440.0, 2.0);
    // (centre frequency Hz, gain dB, Q) — a low boost, a mid dip, a high boost.
    let mut eq = Eq::new(
        fs,
        &[
            (120.0, 6.0, 0.707),
            (1_000.0, -3.0, 1.0),
            (6_000.0, 4.0, 0.707),
        ],
    );

    let mut block = [0.0f32; 128];
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f32;
    let mut samples = 0usize;
    for _ in 0..1_000 {
        for s in block.iter_mut() {
            *s = fm.tick();
        }
        eq.process(&mut block);
        for &s in &block {
            peak = peak.max(s.abs());
            sum_sq += s * s;
        }
        samples += block.len();
    }
    let rms = (sum_sq / samples as f32).sqrt();
    println!("FM synth through EQ: peak {peak:.3}, rms {rms:.3}");
}
