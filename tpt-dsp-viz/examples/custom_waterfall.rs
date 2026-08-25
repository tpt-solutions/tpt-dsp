// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal custom-waterfall example for `tpt-dsp-viz`.
//!
//! Shows the smallest direct usage of [`tpt_dsp_viz::VizApp`]: wire any
//! producer thread that emits `SpectrumFrame`s over a bounded
//! `crossbeam-channel` into the waterfall UI. Here we drive the built-in
//! deterministic synthetic generator (`run_synthetic`), but you can replace
//! it with frames computed from your own signal source using
//! [`tpt_dsp_viz::analyze_block`] on a `RealtimeSpectrumAnalyzer`.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crossbeam_channel::bounded;
use tpt_dsp_analysis::SpectrumConfig;
use tpt_dsp_viz::{run_synthetic, VizApp};

fn main() -> eframe::Result<()> {
    let config = SpectrumConfig::default();

    // Bounded channel: a slow UI simply drops frames instead of lagging behind.
    let (tx, rx) = bounded::<tpt_dsp_viz::SpectrumFrame>(8);
    let stop = Arc::new(AtomicBool::new(false));

    // Producer thread: swap `run_synthetic` for your own loop that feeds
    // blocks of `config.fft_size` samples through `analyze_block`.
    let producer_stop = stop.clone();
    std::thread::spawn(move || run_synthetic(config, tx, producer_stop));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("custom waterfall example")
            .with_inner_size([800.0, 560.0]),
        ..Default::default()
    };

    let app = VizApp::new(
        rx,
        &config,
        "custom synthetic source".to_string(),
        stop.clone(),
    );
    eframe::run_native(
        "custom-waterfall",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )?;
    // Signal the producer to exit if it has not already observed closure.
    stop.store(true, Ordering::Relaxed);
    Ok(())
}
