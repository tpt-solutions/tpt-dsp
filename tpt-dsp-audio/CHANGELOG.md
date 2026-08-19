# Changelog

All notable changes to `tpt-dsp-audio` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

### Added
- `synth_eq` example: an FM synthesis voice (`FmSynth`) shaped by a 3-band `Eq`.

## [0.1.0]

### Added
- Initial release of the synthesis, effects and real-time audio-graph crate built
  on `tpt-dsp-core`.
- Oscillators & synthesis: `Oscillator` (selectable `Waveform`), band-limited
  `Wavetable`, 2-operator `FmSynth`, and `SubtractiveVoice` (oscillator → filter → amp).
- Effects: `Waveshaper` (configurable `Curve`), `Delay` (feedback + wet/dry mix),
  `ConvolutionReverb` (with the `generate_decay_ir` impulse-response helper), and
  `Eq` (multi-band peaking/tone equaliser).
- Graph & engine: `AudioGraph` (sources → nodes → sinks) with ready-made
  `ClosureSource` / `ClosureNode` / `ClosureSink` and `Passthrough`, the
  `Source` / `Sink` / `AudioNode` traits, and the fixed-block, allocation-free
  `RealtimeEngine` (exposing the `BLOCK_128` / `BLOCK_256` block-size constants).

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
