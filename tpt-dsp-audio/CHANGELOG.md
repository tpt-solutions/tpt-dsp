# Changelog

All notable changes to `tpt-dsp-audio` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

## [0.3.0] - 2026-11-07

### Added
- `dynamics`: `Compressor` (with makeup gain and gain-reduction metering), `Limiter`
  (ceiling and release) and `Gate`. Each has a per-sample `tick` and a buffer
  `process`.
- `modulation`: `Lfo` (selectable waveform, unipolar mode), and the effects built on
  it: `Tremolo`, `Chorus`, `Flanger` (with feedback) and `Phaser`.
- `synthesis`: `PolyBlepOscillator` with `BlepShape`, a band-limited oscillator using
  the `poly_blep` correction; `NoiseSource`, `PinkNoise`, and `TpdfDither` for
  quantising to a given bit depth.
- `oversampling::Oversampler`: runs a nonlinear closure at a higher rate and decimates
  the result, with a reported `latency`. Use it around waveshapers and other
  distortion that would otherwise alias.
- `freeverb`: `Freeverb`, the Schroeder–Moorer algorithmic reverb, and
  `StereoFreeverb`, its stereo form with a 23-sample right-channel spread at 44.1 kHz
  (`process_interleaved` for interleaved buffers).
- `Delay::set_delay_fractional`, for fractional delay times.
- `Eq::try_new`, `Eq::with_max_block` and `Eq::try_with_shelves`, for fallible
  construction and for reserving scratch for a chosen block size.
- `set_sample_rate` on `Adsr`, `SubtractiveVoice` and `Eq`, so they can follow a
  host sample-rate change without being rebuilt.

### Fixed
- Oscillator, wavetable, FM and envelope phase wrap no longer hangs on huge or
  non-finite frequencies; parameters are validated.
- `ConvolutionReverb` no longer shifts the stream after a short final block. Output
  now has a fixed one-block latency, and the dry signal is delayed to match.
- `SubtractiveVoice` redesigns its filter every 32 samples instead of every sample.
- `Eq` reserves its scratch at construction, so `process` no longer allocates on the
  first block.
- `Delay` supports fractional, glided delay times and flushes denormal and NaN
  feedback.
- `synth_eq` example: fixed the band order.

## [0.2.0] - 2026-11-01

### Fixed
- `ConvolutionReverb` no longer cuts impulse responses longer than the block size
  short. It now runs on the exact partitioned `FftConvolver` from `tpt-dsp-core`.

## [0.1.0] - 2026-08-26

### Added
- Initial release of the synthesis, effects and real-time audio-graph crate built
  on `tpt-dsp-core`.
- `synth_eq` example: an FM synthesis voice (`FmSynth`) shaped by a 3-band `Eq`.
- Oscillators & synthesis: `Oscillator` (selectable `Waveform`), band-limited
  `Wavetable`, 2-operator `FmSynth`, and `SubtractiveVoice` (oscillator → filter → amp).
- Effects: `Waveshaper` (configurable `Curve`), `Delay` (feedback + wet/dry mix),
  `ConvolutionReverb` (with the `generate_decay_ir` impulse-response helper), and
  `Eq` (multi-band peaking/tone equaliser).
- Graph & engine: `AudioGraph` (sources → nodes → sinks) with ready-made
  `ClosureSource` / `ClosureNode` / `ClosureSink` and `Passthrough`, the
  `Source` / `Sink` / `AudioNode` traits, and the fixed-block, allocation-free
  `RealtimeEngine` (exposing the `BLOCK_128` / `BLOCK_256` block-size constants).

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.3.0
[0.2.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.2.0
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
