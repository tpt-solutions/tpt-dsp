# Changelog

All notable changes to `tpt-dsp-viz` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

### Changed
- **Breaking:** the `eframe`/`egui` GUI is replaced by an in-tree software renderer
  (`canvas`, `font`, `app`) over a `minifb` window. `egui`, `eframe` and their
  Apache-2.0-only transitive crates (`winit`, `glutin`, `gethostname`, `dpi`, ...) are
  gone from the build.
- **Breaking:** `run` and the examples return `tpt_dsp_viz::Result` (wrapping `tpt_dsp_viz::Error`)
  instead of `eframe::Result`. `colormap` returns `Rgb` instead of `egui::Color32`.
- `VizApp` is no longer an `eframe::App`. Drive it with `run_app`, or with
  `update`, `on_click` and `draw_into` from your own loop.
- The crate uses the workspace MSRV (1.85). The `rust-version = "1.95"` override and the
  CI exclusion are removed.

### Added
- `run_app` opens a window for a `VizApp`. Left-click on the button, `P` or Space toggle pause. Escape closes.
- `examples/snapshot.rs` renders one frame to a PNG without a display.
- A built-in 5x7 bitmap font covering the UI labels.

### Changed
- `eframe` and `egui` are upgraded from 0.36 to 0.37.
- The crate sets its own `rust-version = "1.95"`, not the workspace's 1.85, because
  `egui` 0.37 requires Rust 1.95. The workspace MSRV check excludes this crate.

## [0.2.0] - 2026-11-01

### Changed
- `egui` is now reached through `eframe::egui` rather than as a separate direct
  dependency, so the crate can't pull in a second `egui` that drifts from `eframe`'s.

### Fixed
- The build no longer fails against duplicate `egui` versions, and the crate
  documentation builds without warnings.

## [0.1.0] - 2026-08-26

### Added
- Initial release of the desktop visualization front end, rendered with `egui` /
  `eframe` on top of `tpt-dsp-core` and `tpt-dsp-analysis`.
- `VizApp` / `run`: a `crossbeam-channel`-backed producer → UI split, so a
  capture/generator thread can stream analysed `SpectrumFrame`s to the render loop
  without blocking it.
- A scrolling waterfall spectrogram (colour-mapped black → blue → cyan → yellow →
  red) produced from `tpt-dsp-analysis`'s `Spectrogram`.
- A live dB spectrum-line plot with configurable floor and a peak-frequency / dB
  readout, driven by `SpectrumAnalyzer` / `RealtimeSpectrumAnalyzer`.
- A `Source` selector: live capture from the default system audio device (feature
  `audio`, via `tpt-dsp-io`'s in-tree WASAPI/ALSA/CoreAudio backends) or a
  deterministic synthetic multi-tone + noise signal.

### Changed
- The audio-input callbacks recover from a poisoned mutex
  (`lock().unwrap_or_else(|e| e.into_inner())`) instead of unwrapping, so a panic
  on another thread no longer crashes every subsequent callback.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.2.0
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
