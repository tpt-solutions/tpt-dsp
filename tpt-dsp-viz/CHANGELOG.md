# Changelog

All notable changes to `tpt-dsp-viz` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

### Changed
- Audio input now uses `tpt-dsp-io`'s in-tree WASAPI backend instead of `cpal`,
  which has been fully removed from the tree.
- The audio-input callbacks recover from a poisoned mutex
  (`lock().unwrap_or_else(|e| e.into_inner())`) instead of unwrapping, so a panic
  on another thread no longer crashes every subsequent callback.

## [0.1.0]

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
  `audio`, via `cpal`) or a deterministic synthetic multi-tone + noise signal.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
