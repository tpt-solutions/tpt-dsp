# Changelog

All notable changes to `tpt-dsp-analysis` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

## [0.3.0] - 2026-11-07

### Added
- Spectral descriptors in `features`: `spectral_flatness`, `spectral_rolloff` and
  `spectral_flux`.
- `loudness::LoudnessMeter`: ITU-R BS.1770 K-weighted integrated loudness, with
  `push`, `integrated` and `reset`. Checked against `pyloudnorm`.
- `mfcc::Mfcc`: mel-frequency cepstral coefficients (`compute` into a caller buffer),
  with `hz_to_mel` and `mel_to_hz`.
- `stft` module: `stft` and `istft`, a phase vocoder `time_stretch`, and `pitch_shift`
  (phase vocoder followed by cubic resampling).
- `onset` module: `detect_onsets` (spectral-flux onset detection) and `pick_onsets`
  (peak picking with `OnsetConfig`).
- `yin::yin_pitch`: YIN fundamental-frequency estimation.

### Fixed
- `OutlierDetector` sorts with a total order (`f32::total_cmp`), so a NaN sample in
  the window no longer panics.
- `Linear` spectrum averaging keeps tracking on very long runs instead of freezing.
- `Ema` rejects `alpha = 0`, which would otherwise hold its output forever.

## [0.2.0] - 2026-11-01

### Fixed
- Broken intra-doc links in the crate documentation. No API change.

## [0.1.0] - 2026-08-26

### Added
- Initial release of the spectrum-analysis, time-series statistics and feature
  extraction crate built on `tpt-dsp-core`. Ships with no async runtime by default;
  opt in via `async` / `async-tokio` / `async-std`.
- `analyze_signal` example: real-time spectrum analysis of a noisy sine, with EMA
  smoothing and outlier detection on the peak level.
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

### Changed
- `peak_bin` now uses a total order over `f32`, so a NaN/Inf value in the magnitude
  spectrum can no longer panic the real-time analysis path.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.3.0
[0.2.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.2.0
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
