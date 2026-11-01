# Changelog

All notable changes to `tpt-dsp-core` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/CHANGELOG.md).

## [Unreleased]

## [0.2.0] - 2026-11-01

### Added
- FFT-backed fast transforms for a fixed size (`std` feature): `FastDct2Plan` and
  `FastDct2dPlan` (DCT-II/III, 1D and separable 2D), `FastDctIvPlan` (DCT-IV), and
  `FastMdctPlan`. Each matches the direct reference function's conventions, so
  `forward` followed by `inverse` recovers the input up to the documented scale.
- Separable 2D DCT: `dct_2d` and `idct_2d`, plus the direct MDCT/IMDCT pair `mdct`
  / `imdct` with `MdctPlan` and `mdct_sine_window`.
- Perceptual frequency scales: `hz_to_bark`, `bark_to_hz`, `hz_to_erb_rate`,
  `erb_rate_to_hz`, `erb_bandwidth`, `fft_bin_bark_bands` and `aggregate_bands`.
- Linear prediction: `autocorrelation`, `levinson_durbin`, `predict`, `residual`,
  `synthesize`, and the owning `LpcAnalyzer` (feature `alloc`).
- Line spectral pairs/frequencies (feature `alloc`): `lpc_to_lsp`, `lsp_to_lpc`,
  `lsp_is_stable`, `lsp_interpolate`, `lsp_quantize_uniform`, `lsp_rad_to_lsf_hz`,
  `lsf_hz_to_lsp_rad`, and `DEFAULT_GRID_POINTS`. The LPC-to-LSP conversion is
  checked by round-tripping through `lsp_to_lpc` at orders 2 through 32.
- Pitch estimation: `autocorrelation_pitch` and `amdf_pitch`, returning a
  `PitchEstimate` with a voiced/unvoiced decision and a confidence value, plus
  `hz_range_to_period_samples` and the speech F0 range constants. Both estimators
  prefer the shortest candidate lag, which avoids reporting a harmonic multiple of
  the true period.
- Psychoacoustic model helpers: `threshold_in_quiet`, `is_tonal_peak`,
  `classify_tonal_bins`, `spreading_function_db`, `tonal_masking_offset_db`,
  `noise_masking_offset_db`, `simultaneous_masking`, `combined_threshold_db`,
  `perceptual_weight_db`, `power_to_db` and `db_to_power`. The masking offsets use
  the constants from MPEG psychoacoustic model 1; this is not a bit-exact
  reproduction of any one standard.
- Noise shaping: `noise_shape_step`, `noise_shaper_is_stable`, and the owning
  `NoiseShaper` (feature `alloc`). The error-feedback sign gives the `NTF = 1 - z⁻¹`
  telescoping property, so the accumulated error stays bounded.
- Vector quantisation: `nearest_vector`, `nearest_vector_accelerated` (numerically
  identical to the linear scan, with early exit), `vq_encode`, `vq_decode`,
  `DistanceMetric` (squared Euclidean or Manhattan), and the owning `VqCodebook`
  (feature `alloc`).

### Changed
- The `simd` module documents its architecture support and why there is no runtime
  CPU-feature dispatch; wider instruction sets are a compile-time choice (for example
  `-C target-cpu=native`). See `ARCHITECTURE.md` §10.
- Every public API is classified against explicit real-time and offline execution
  profiles; see `ARCHITECTURE.md` §11.

### Fixed
- `FftConvolver` truncated its kernel to the FFT length and kept only one block of
  overlap-add tail, so impulse responses longer than the block size were silently cut
  short. It now splits the kernel into block-sized partitions and is exact for an
  impulse response of any length, still without allocating in `process`.
- `dct_ii`, `dct_iii`, `dct_iv`, `mdct` and `imdct` evaluated `cos()` at the raw,
  unreduced angle. For large transform sizes that angle reaches thousands of radians,
  which `f32`/`f64` `cos()` handles poorly. The angle is now reduced in the integer
  domain (`complex::cos_pi_over_n`) before any float is formed. Results change slightly
  at large N.
- Spurious unused-import warnings in `no_std` builds. The float imports are kept for
  targets whose `core` lacks float intrinsics. No behaviour change.

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

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.2.0
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
