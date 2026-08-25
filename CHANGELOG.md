# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `justfile` with `ci`, `test`, `examples` and cross-platform recipes
  (`no_std`, `wasm`) to de-duplicate the command list repeated in
  README / CONTRIBUTING.md / AGENTS.md.
- GitHub issue and pull-request templates.
- Runnable examples for `tpt-dsp-core`, `tpt-dsp-audio`,
  `tpt-dsp-analysis` and `tpt-dsp-control` (in addition to the existing
  `tpt-dsp-io` SDR pipeline example).
- `tpt-dsp-cli` — a command-line WAV/IQ DSP pipeline: `filter` (biquad / EQ /
  waveshaper / delay / convolution-reverb chains), `demod` (raw IQ → WAV via FM
  discriminator), `spectrum` (averaged magnitude spectrum + features, optional
  CSV) and `info`. Workspace member.
- `tpt-dsp-nihplug` — a CLAP/VST3 plugin wrapping `tpt-dsp-audio` (pedalboard:
  Waveshaper → Delay → ConvolutionReverb → 3-band EQ). Uses nice-plug (the
  maintained successor to nih-plug, which is no longer on crates.io) and is
  **excluded** from the main workspace (see root `Cargo.toml` `exclude`).
- `tpt-dsp-py` — pyo3 Python bindings exposing `rms`, `zero_crossing_rate`,
  `spectral_centroid`, `spectrum`, `fm_demod` and `analyze` as the `tpt_dsp`
  extension module. Also **excluded** from the main workspace.
- `docs/QUICKSTART.md` — a single clone → build → run path through the
  examples, library usage and the local web pedalboard.
- README comparison table (`no_std` / real-time guarantee / RF-SDR / plugin
  export) against `cpal`, `dasp`, `fundsp` and JUCE, alongside the prose
  comparison notes.
- `templates/dsp-effect-crate/` — a cargo-generate skeleton for new effect
  crates, pre-wired to the zero-allocation scratch pattern and
  `#![warn(missing_docs)]` (excluded from the workspace).
- `tpt-dsp-viz/examples/custom_waterfall.rs` — minimal custom-waterfall usage
  driving `VizApp` directly over a bounded channel.

### Changed
- `tpt-dsp-analysis`: `peak_bin` now uses a total order over `f32`, so a
  NaN/Inf value in the magnitude spectrum (malformed IQ-derived data) can no
  longer panic the real-time analysis path.
- `tpt-dsp-viz`: the `cpal` audio-input callbacks recover from a poisoned
  mutex (`lock().unwrap_or_else(|e| e.into_inner())`) instead of unwrapping,
  so a panic on another thread no longer crashes every subsequent callback.
- `tpt-dsp-io`: documented that `IqStream::feed` grows the internal buffer
  without bound if the caller never calls `drain` — the bounded
  `IqReassembler` is the recommended streaming path.
- CI: added a top-level `permissions: contents: read` block to
  `.github/workflows/ci.yml` for least-privilege defense-in-depth.

### Fixed
- Regression test covering `peak_bin` behaviour with NaN-containing input.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
