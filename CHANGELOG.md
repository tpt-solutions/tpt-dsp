# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Native audio backends for all three desktop platforms in `tpt-dsp-io/src/audio/`,
  with no external audio-crate dependency:
  - macOS CoreAudio AudioUnits (`backend_mac.rs`) — default-output playback and
    HALOutput capture via hand-declared `extern "C"` bindings to the system
    `AudioToolbox`/`CoreAudio` frameworks; device enumeration via
    `kAudioHardwarePropertyDevices`.
  - Linux raw ALSA UAPI (`backend_linux.rs`) — ioctls directly on
    `/dev/snd/pcmC*D*p|c`, blocking `RW_INTERLEAVED` transfers, FLOAT/S32/S16 format
    negotiation, XRUN recovery and `/proc/asound`-based device enumeration.
- Cross-platform device selection API: `run_output_on_device` / `run_input_on_device`
  plus `list_output_devices` / `list_input_devices` (friendly names from the MMDevice
  property store on Windows, `/proc/asound` on Linux, CoreAudio property queries on
  macOS).
- `tpt-dsp-io/src/wav.rs` — built-in RIFF/WAVE reader/writer replacing the `hound`
  crate: PCM 8/16/24/32-bit and IEEE float 32/64 input, WAVE_FORMAT_EXTENSIBLE
  support, 32-bit float output, normalised to `f32`; CLI migrated to it.
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
- **`cpal` fully removed from the tree.** The `audio` feature of `tpt-dsp-io` now has
  zero external dependencies, resolving the Apache-2.0-only licensing constraint for
  MIT-only redistribution.
- `tpt-dsp-analysis`: `peak_bin` now uses a total order over `f32`, so a
  NaN/Inf value in the magnitude spectrum (malformed IQ-derived data) can no
  longer panic the real-time analysis path.
- `tpt-dsp-viz`: the audio-input callbacks recover from a poisoned
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
