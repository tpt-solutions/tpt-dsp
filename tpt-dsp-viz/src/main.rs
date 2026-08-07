// SPDX-License-Identifier: MIT OR Apache-2.0

//! Entry point for the `tpt-dsp-viz` desktop visualizer.
//!
//! Pass `--audio` to capture from the default system audio input device
//! (requires the `audio` feature: `cargo run -p tpt-dsp-viz --features audio
//! -- --audio`). Without it, a deterministic synthetic signal is shown.

fn main() -> eframe::Result<()> {
    let use_audio = std::env::args().any(|a| a == "--audio");
    let source = if use_audio {
        #[cfg(feature = "audio")]
        {
            tpt_dsp_viz::Source::Audio
        }
        #[cfg(not(feature = "audio"))]
        {
            eprintln!("audio feature not enabled; run with --features audio");
            tpt_dsp_viz::Source::Synthetic
        }
    } else {
        tpt_dsp_viz::Source::Synthetic
    };
    tpt_dsp_viz::run(source)
}
