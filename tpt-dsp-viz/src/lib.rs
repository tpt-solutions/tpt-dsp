//! Real-time waterfall spectrum and live spectrum-line desktop UI.
//!
//! `tpt-dsp-viz` renders live signal displays on top of [`tpt_dsp_analysis`]
//! with its own software renderer: a `0x00RRGGBB` pixel canvas, a built-in
//! bitmap font and a small layout/hit-test layer. The native window and input
//! come from `minifb`, the only third-party GUI-adjacent dependency. A
//! capture/generator thread streams analysed [`SpectrumFrame`]s to the UI over
//! a `crossbeam-channel`; [`run`] wires that thread to a [`VizApp`] and shows
//! the waterfall plus the live spectrum line.
//!
//! [`tpt_dsp_analysis`]: tpt_dsp_analysis

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

mod app;
mod canvas;
mod colormap;
mod font;
mod pipeline;
mod window;

pub use app::VizApp;
pub use canvas::Rgb;
pub use colormap::colormap;
#[cfg(feature = "audio")]
pub use pipeline::run_audio_input;
pub use pipeline::{analyze_block, run_synthetic, SpectrumFrame, SyntheticGenerator};
pub use window::{run_app, Error, Result};

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

/// Default window size used by [`run`], in pixels.
pub const DEFAULT_WINDOW_SIZE: (usize, usize) = (1024, 720);

/// Run the visualizer, blocking until the window is closed.
///
/// Spawns the producer thread for `source`, opens a window, and returns once
/// the user closes it (the producer is signalled to stop on exit).
///
/// # Errors
///
/// Returns [`Error::Window`] if the window cannot be created or presented to.
pub fn run(source: Source) -> Result {
    let config = SpectrumConfig::default();
    let (tx, rx) = bounded(8);
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let source_label = match source {
        Source::Synthetic => "synthetic (multi-tone + noise)",
        #[cfg(feature = "audio")]
        Source::Audio => "audio input (built-in WASAPI/native backend)",
    }
    .to_string();

    std::thread::spawn(move || match source {
        Source::Synthetic => run_synthetic(config, tx, stop_thread),
        #[cfg(feature = "audio")]
        Source::Audio => run_audio_input(config, tx, stop_thread),
    });

    let app = VizApp::new(rx, &config, source_label, stop);
    run_app(app, "tpt-dsp-viz", DEFAULT_WINDOW_SIZE)
}
