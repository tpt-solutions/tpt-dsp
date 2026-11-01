# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-01

### Added
- `tpt-dsp-core::FastDct2Plan` and `tpt-dsp-core::FastDct2dPlan`: FFT-backed
  DCT-II and DCT-III of a fixed size, and the separable in-place 2D counterpart,
  carrying the direct pair's exact conventions (`forward` then `inverse` recovers
  `n/2·x` in 1D and `n_rows·n_cols/4·x` in 2D; verified against the direct
  references at 4–128 points and 8×8–32×32). Both directions share one length-`2n`
  inverse FFT, so the `+½` sample offsets need no reordering. Measured 4.4–15×
  faster than the direct pair at video tile sizes (see `BENCHMARKS.md`).
- `tpt-dsp-core::FastMdctPlan` and `tpt-dsp-core::FastDctIvPlan`: FFT-backed fast
  MDCT/IMDCT and DCT-IV, numerically verified against the direct O(N²) `mdct`,
  `imdct` and `dct_iv` references. At block half-sizes of 512–4096 the fast path is
  roughly 2000–15000× faster than the direct sum (see `BENCHMARKS.md`).
- `tpt-dsp-core::{autocorrelation_pitch, amdf_pitch}`: pitch (F0) estimation with
  voiced/unvoiced classification and a confidence metric, plus
  `hz_range_to_period_samples`. Both estimators prefer the shortest candidate lag,
  which avoids the classic octave error of reporting a harmonic multiple of the
  true period.
- `tpt-dsp-core::{nearest_vector, nearest_vector_accelerated, vq_encode, vq_decode,
  DistanceMetric, VqCodebook}`: generic vector quantisation. The accelerated search
  is numerically identical to the linear scan, with early exit, and about 3–3.6×
  faster at typical codebook sizes.
- `tpt-dsp-core::{lpc_to_lsp, lsp_to_lpc, lsp_is_stable, lsp_interpolate,
  lsp_quantize_uniform, lsp_rad_to_lsf_hz, lsf_hz_to_lsp_rad}` (`alloc`-gated): line
  spectral pairs and frequencies. `lpc_to_lsp` finds the interlacing roots of the
  `P(z)`/`Q(z)` polynomials by evaluate-and-bisect grid search and is round-trip
  tested against `lsp_to_lpc` at LPC orders 2 through 32. `lsp_quantize_uniform`
  guards against two close LSPs quantising to the same level, which would produce an
  unstable reconstructed filter.
- `tpt-dsp-core::{hz_to_bark, bark_to_hz, erb_bandwidth, hz_to_erb_rate,
  erb_rate_to_hz, fft_bin_bark_bands, aggregate_bands}`: Bark and ERB perceptual
  frequency scales, plus FFT-bin-to-critical-band mapping and per-band energy
  aggregation.
- `tpt-dsp-core::{threshold_in_quiet, is_tonal_peak, classify_tonal_bins,
  spreading_function_db, tonal_masking_offset_db, noise_masking_offset_db,
  simultaneous_masking, combined_threshold_db, perceptual_weight_db}`: a generic
  psychoacoustic model built from Terhardt's threshold-in-quiet approximation, a
  local-peak tonality test, Schroeder's masking-spread function, and the tonal and
  noise masking offsets of MPEG psychoacoustic model 1. It is not a bit-exact
  reproduction of any one standard.
- `tpt-dsp-core::{noise_shape_step, noise_shaper_is_stable, NoiseShaper}`: a generic
  error-feedback noise-shaping filter with configurable order. It subtracts the
  fed-back past error, which gives the `NTF = 1 - z⁻¹` telescoping property, so the
  accumulated error stays bounded. `noise_shaper_is_stable` checks a caller-supplied
  quantiser's error bound under feedback.

### Changed
- Documented `tpt-dsp-core`'s SIMD architecture support and why there is no runtime
  CPU-feature dispatch. See `ARCHITECTURE.md` §10.
- Defined explicit real-time and offline execution profiles and classified every
  public API against them. See `ARCHITECTURE.md` §11.
- Fixed broken intra-doc links in `tpt-dsp-analysis` and `tpt-dsp-viz`, and removed
  the duplicate `egui` dependency from `tpt-dsp-viz`. Per-crate changelogs have the
  details.
- `tpt-dsp-io`: the `audio_check` example requires the `audio` feature.

### Fixed
- `dct_ii`, `dct_iii`, `dct_iv`, `mdct` and `imdct` evaluated `cos()` at the raw,
  unreduced `(π/N)·x·y` angle. For large N that angle reaches thousands of radians,
  which `f32`/`f64` `cos()` handles poorly. A new `complex::cos_pi_over_n` reduces the
  angle in the integer domain before any float is formed, and all five direct
  transforms use it. Caught by comparing the new fast plans against the direct
  implementations at N = 1024 and up.
- `FftConvolver` truncated impulse responses to a single block. It now uses
  partitioned convolution and is exact for an impulse response of any length, which
  also fixes `tpt-dsp-audio`'s `ConvolutionReverb`.
- `tpt-dsp-core`: spurious unused-import warnings in `no_std` builds.

## [0.1.0] - 2026-08-26

Initial public release of `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`,
`tpt-dsp-control`, `tpt-dsp-io`, `tpt-dsp-viz`, `tpt-dsp-wasm` and
`tpt-dsp-cli` to crates.io. `tpt-dsp-nihplug` and `tpt-dsp-py` remain
unpublished (see [PUBLISHING.md](PUBLISHING.md)).

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

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.2.0
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
