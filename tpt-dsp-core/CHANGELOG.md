# Changelog

All notable changes to `tpt-dsp-core` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).

## [Unreleased]

## [0.1.0] - 2026-08-26

### Added
- Initial release of the `no_std`, `#![forbid(unsafe_code)]` math engine that every
  other `tpt-dsp-*` crate builds on.
- `biquad_lowpass` example: design a low-pass `Biquad`, filter a mixed 1 kHz + 6 kHz
  tone, then locate the surviving tone with a reusable `FftPlan`.
- Complex math helpers: `exp_i`, `magnitude`, `magnitude_squared`, `phase`,
  `rotate`, and SIMD `complex_add_simd` / `complex_mul_simd` / `magnitude_simd`.
- Transforms: `fft` / `ifft` / `fft_inplace` / `fft_inplace_f32` / `ifft_inplace`,
  the reusable `FftPlan`, `twiddles`, `next_power_of_two` / `is_power_of_two`,
  DCT-II/III/IV, and the Hilbert transform (free `hilbert` + owning
  `HilbertTransformer`).
- Demodulation: `FmDemodulator`, `phase_delta`, `phase_to_audio`.
- Windowing: `windowed` with `WindowType` (Hann, Hamming, Blackman).
- Filters: the allocation-free single-stage `Biquad` (`design` / `process`), plus
  owning `Fir`, `IirCoeffs` / `IirStage` / `IirFilter` when the `alloc` feature is on.
- Convolution: `convolve`, `FftConvolver`, `ConvolvePlan`.
- Resampling: `FIRDecimator` (feature `alloc`).
- Buffers: lock-free `RingBuffer` (`RingRead` / `RingWrite`) and the crossbeam-backed
  `SpscQueue` (feature `std`).
- SIMD-accelerated paths behind the nightly-only `simd` feature, with an identical
  stable scalar fallback so the public API never changes.

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
