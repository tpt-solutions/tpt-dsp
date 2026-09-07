# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `tpt-dsp-core::FastMdctPlan` and `tpt-dsp-core::FastDctIvPlan`: FFT-backed
  (`FftPlan`/RustFFT) fast MDCT/IMDCT and DCT-IV, `std`+`f32`-only,
  numerically verified against the existing direct O(N²) `mdct`/`imdct`/
  `dct_iv` references (`fast_matches_reference_*` tests). At Pulse/Aura's
  block half-sizes (512-4096), the fast path is ~2000-15000× faster than
  the direct sum (see `BENCHMARKS.md`).
- `tpt-dsp-core::{autocorrelation_pitch, amdf_pitch}`: pitch (F0)
  estimation with voiced/unvoiced classification and a confidence metric,
  plus `hz_range_to_period_samples` for converting an F0 search range to a
  sample-lag range. Both estimators bias tie-breaking toward the shortest
  candidate lag to avoid the classic autocorrelation/AMDF "octave error"
  (reporting a harmonic multiple of the true period instead of the
  fundamental) — see `pitch.rs`'s doc comments for the reasoning. For
  `tpt-media`'s Vox codec.
- `tpt-dsp-core::{nearest_vector, nearest_vector_accelerated, vq_encode,
  vq_decode, DistanceMetric, VqCodebook}`: generic vector quantisation —
  linear nearest-codeword search (squared-Euclidean or Manhattan
  distance), an early-exit-accelerated variant that's numerically
  identical to the plain linear scan (see `accelerated_matches_linear_search`)
  but ~3-3.6× faster at Vox's/Whisper's target codebook sizes, and
  encode/decode against a caller-supplied flat codebook. For `tpt-media`'s
  Vox and Whisper codecs (the codec-specific codebook contents stay in
  each codec).
- `tpt-dsp-core::{lpc_to_lsp, lsp_to_lpc, lsp_is_stable, lsp_interpolate,
  lsp_quantize_uniform, lsp_rad_to_lsf_hz, lsf_hz_to_lsp_rad}`
  (`alloc`-gated): Line Spectral Pairs/Frequencies —
  an LPC representation robust to interpolation and quantization.
  `lpc_to_lsp` finds the interlacing roots of the classic `P(z)`/`Q(z)`
  polynomials via a direct evaluate-and-bisect grid search rather than the
  classical Chebyshev-recursion formulation (needs no extra scratch/
  coefficient transform, and is directly round-trip-testable against
  `lsp_to_lpc`) — verified at LPC orders 2 through 32 (Aura's max).
  `lsp_quantize_uniform` guards against a real failure mode of naive
  per-coefficient quantization: two close LSPs can quantize to the same
  level, silently producing an *unstable* reconstructed filter. For
  `tpt-media`'s Vox codec.
- `tpt-dsp-core::{hz_to_bark, bark_to_hz, erb_bandwidth, hz_to_erb_rate,
  erb_rate_to_hz, fft_bin_bark_bands, aggregate_bands}`: Bark and ERB
  perceptual frequency scales (Traunmüller/Glasberg-Moore), plus
  FFT-bin-to-critical-band mapping and per-band energy aggregation.
- `tpt-dsp-core::{threshold_in_quiet, is_tonal_peak,
  classify_tonal_bins, spreading_function_db, tonal_masking_offset_db,
  noise_masking_offset_db, simultaneous_masking, combined_threshold_db,
  perceptual_weight_db}`: a generic psychoacoustic model — Terhardt's
  (1979) threshold-in-quiet approximation, a local-peak tonality test,
  Schroeder's (1979) masking-spread function, and the standard tonal/
  noise masking offsets (matching MPEG psychoacoustic model 1's
  constants) combined into a per-band masking threshold and
  signal-to-mask-ratio. For `tpt-media`'s Aura lossy mode (§10 in its
  `todo.md`) — not a byte-for-byte reproduction of any one standard's
  exact model.
- `tpt-dsp-core::{noise_shape_step, noise_shaper_is_stable, NoiseShaper}`:
  a generic error-feedback noise-shaping filter (configurable order via
  the feedback-coefficient slice length). Subtracts the fed-back past
  error (`shaped[n] = x[n] - Σh[k]e[n-1-k]`) rather than adding it — the
  sign that gives the classic `NTF=1-z⁻¹` telescoping property
  (`Σ(y[n]-x[n])` stays bounded regardless of run length, unlike plain
  quantization of an unquantizable constant, whose cumulative error grows
  linearly). `noise_shaper_is_stable` validates an arbitrary
  caller-supplied `quantize` closure's error-boundedness under feedback —
  round-to-nearest is self-limiting for *any* finite feedback gain (its
  error is always a bounded rounding residual by construction), so the
  check has real teeth only for a quantizer that isn't. For `tpt-media`'s
  Aura lossy mode.

### Changed
- Documented `tpt-dsp-core`'s SIMD architecture support and why no runtime
  CPU-feature dispatch (`is_x86_feature_detected!`/`is_aarch64_feature_
  detected!`) is implemented — see `ARCHITECTURE.md` §10. Also corrected a
  stale `ARCHITECTURE.md` line that still described the `simd.rs` vectorised
  path as "not started".
- Defined explicit Real-time / Offline execution profiles and classified
  every public API across all five crates against them — see
  `ARCHITECTURE.md` §11.

### Fixed
- `dct_ii`/`dct_iii`/`dct_iv`/`mdct`/`imdct`: the direct sums evaluated
  `cos()` at the *raw*, unreduced `(π/N)·x·y` angle, which for realistic
  block sizes (N in the thousands, as used by `tpt-media`'s Pulse/Aura)
  reaches thousands of radians — poorly conditioned for `f32`/`f64` `cos()`,
  since representing a large angle consumes most of the mantissa before the
  periodic part that determines the result. Added `complex::cos_pi_over_n`,
  which reduces the angle in the *integer* domain (exact, since transform
  indices are each an integer or half-integer) before ever forming a float,
  and switched all five direct transforms to it. Caught by comparing the
  new `FastMdctPlan`/`FastDctIvPlan` against the existing direct
  implementations at N=1024+ and seeing real (not rounding-noise)
  disagreement.

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

[Unreleased]: https://github.com/TPT-Solutions/tpt-dsp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TPT-Solutions/tpt-dsp/releases/tag/v0.1.0
