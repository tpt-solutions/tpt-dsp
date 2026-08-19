//! `tpt-dsp-analysis` tour: analyse a noisy sine with the real-time spectrum
//! analyzer and smooth its peak level with an EMA. Outlier detection flags any
//! block whose peak deviates from recent history.
//!
//! ```text
//! cargo run -p tpt-dsp-analysis --example analyze_signal
//! ```

use tpt_dsp_analysis::{Ema, OutlierDetector, RealtimeSpectrumAnalyzer, SpectrumConfig};

fn main() {
    let n = 1024;
    let sr = 48_000.0f32;
    let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size: n,
        sample_rate: sr,
        ..SpectrumConfig::default()
    });

    let mut ema = Ema::new(0.1);
    let mut outlier = OutlierDetector::new(64, 3.0);
    let mut lcg = 0x1234_5678_9abc_def1u64;
    let mut flagged = 0usize;

    for _ in 0..200 {
        let block: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / sr;
                let sig = (2.0 * std::f32::consts::PI * 1_000.0 * t).sin();
                lcg = lcg
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let noise = ((lcg >> 33) as f32 / (1u32 << 31) as f32) - 1.0;
                sig + 0.1 * noise
            })
            .collect();
        analyzer.process(&block);

        let peak = analyzer.peak().expect("peak");
        ema.push(peak.magnitude_db);
        if outlier.push(peak.magnitude_db) {
            flagged += 1;
        }
    }

    let peak = analyzer.peak().expect("peak");
    println!(
        "dominant tone: {:.1} Hz at {:.2} dB (smoothed {:.2} dB); {} outlier blocks",
        peak.frequency,
        peak.magnitude_db,
        ema.value(),
        flagged
    );
}
