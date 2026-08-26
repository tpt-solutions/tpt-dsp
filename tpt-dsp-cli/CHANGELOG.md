# Changelog

All notable changes to `tpt-dsp-cli` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

## [0.1.0] - 2026-08-26

### Added
- Initial release: a command-line WAV/IQ DSP pipeline built on `tpt-dsp-core`,
  `tpt-dsp-audio`, `tpt-dsp-analysis` and `tpt-dsp-io`.
- `filter`: apply a chain of effects (biquad, waveshaper, delay, convolution
  reverb, parametric EQ) to a WAV file, one chain per channel.
- `demod`: FM-demodulate a raw IQ file into a mono WAV file, with optional
  output decimation.
- `spectrum`: averaged magnitude spectrum plus peak/RMS/zero-crossing/
  centroid features for a WAV or IQ file, with optional CSV export of the
  spectrum.
- `info`: print WAV header or IQ size/format metadata.
- Library API (`read_wav`/`write_wav`, `Effect`/`EffectChain`, `read_iq`/
  `demod_iq`, `analyze_real`/`analyze_complex`, `write_spectrum_csv`) usable
  independently of the CLI binary.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
