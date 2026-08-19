# Changelog

All notable changes to `tpt-dsp-analysis` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

### Added
- `analyze_signal` example: real-time spectrum analysis of a noisy sine, with EMA
  smoothing and outlier detection on the peak level.

### Changed
- `peak_bin` now uses a total order over `f32`, so a NaN/Inf value in the magnitude
  spectrum can no longer panic the real-time analysis path.

## [0.1.0]

### Added
- Initial release of the spectrum-analysis, time-series statistics and feature
  extraction crate built on `tpt-dsp-core`. Ships with no async runtime by default;
  opt in via `async` / `async-tokio` / `async-std`.
- Time series (`timeseries`): `MovingAverage`, `RunningMean`, `Ema`,
  `OutlierDetector`.
- Features (`features`): `rms`, `zero_crossing_rate`, `spectral_centroid`,
  `spectral_centroid_normalized`.
- Spectrum (`spectrum`): `SpectrumAnalyzer` / `SpectrumConfig` (windowed FFT, dB
  scaling, configurable floor), `RealtimeSpectrumAnalyzer` with `Averaging` modes,
  `find_peaks` (parabolic interpolation), `dominant_frequency`, and
  `linear_to_db` / `db_to_linear`.
- Spectrogram (`spectrogram`): `Spectrogram`, accumulating successive frames into a
  waterfall image.
- Async adapters (`async_adapters`): runtime-agnostic `process_stream_in_place` /
  `process_stream_into_sink`, plus tokio `process_channel*` / `process_stream` and
  async-std equivalents, behind the `async-tokio` / `async-std` features.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
