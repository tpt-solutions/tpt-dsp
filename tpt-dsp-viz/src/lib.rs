//! Real-time waterfall spectrum and live spectrum-line desktop UI.
//!
//! `tpt-dsp-viz` renders live signal displays with [`egui`]/[`eframe`] on top
//! of [`tpt_dsp_analysis`]. A capture/generator thread streams analysed
//! [`SpectrumFrame`]s to the UI over a `crossbeam-channel`; [`run`] wires that
//! thread to a [`VizApp`] and shows the waterfall plus the live spectrum line.
//!
//! [`tpt_dsp_analysis`]: tpt_dsp_analysis

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod app;
mod colormap;
mod pipeline;

pub use app::VizApp;
pub use colormap::colormap;
#[cfg(feature = "audio")]
pub use pipeline::run_audio_input;
pub use pipeline::{analyze_block, run_synthetic, SpectrumFrame, SyntheticGenerator};

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crossbeam_channel::bounded;
use tpt_dsp_analysis::SpectrumConfig;

/// Where the visualized signal comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Deterministic multi-tone + noise demo signal (no hardware required).
    Synthetic,
    /// Live capture from the default system audio input device.
    #[cfg(feature = "audio")]
    Audio,
}

/// Run the visualizer, blocking until the window is closed.
///
/// Spawns the producer thread for `source`, opens an `eframe` window, and
/// returns once the user closes it (the producer is signalled to stop on exit).
pub fn run(source: Source) -> eframe::Result<()> {
    let config = SpectrumConfig::default();
    let (tx, rx) = bounded(8);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let source_label = match source {
        Source::Synthetic => "synthetic (multi-tone + noise)",
        #[cfg(feature = "audio")]
        Source::Audio => "audio input (cpal)",
    }
    .to_string();

    std::thread::spawn(move || match source {
        Source::Synthetic => run_synthetic(config, tx, stop_thread),
        #[cfg(feature = "audio")]
        Source::Audio => run_audio_input(config, tx, stop_thread),
    });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("tpt-dsp-viz")
            .with_inner_size([1024.0, 720.0]),
        ..Default::default()
    };

    let app = VizApp::new(rx, &config, source_label, stop);
    eframe::run_native(
        "tpt-dsp-viz",
        options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
}
