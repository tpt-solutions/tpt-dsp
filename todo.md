# tpt-dsp — Project TODO

A pure-Rust, real-time-safe DSP framework. Dual-licensed MIT / Apache-2.0. © TPT Solutions.

_Last synced: 2026-08-26. Reconciled with the actual code in `tpt-dsp-*/src`. Three previously-deferred "future pass" crates are now implemented: `tpt-dsp-cli` (WAV/IQ pipe tool, a workspace member), and `tpt-dsp-nihplug` (CLAP/VST3 wrapper, uses nice-plug) and `tpt-dsp-py` (pyo3 Python bindings). The latter two are intentionally excluded from the main workspace (see root `Cargo.toml` `exclude`) and verified as standalone workspaces. All original workspace gates (`fmt`/`build`/`test`/`clippy`/`deny`/`doc`) remain green._

---

## Phase 0: Project & Repo Setup

- [x] `git init` and add a Rust `.gitignore`
- [x] Scaffold Cargo workspace with 5 member crates: `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-control`, `tpt-dsp-io` _(+ later `tpt-dsp-viz`, `tpt-dsp-wasm`)_
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (copyright TPT Solutions)
- [x] Add SPDX `dual MIT/Apache-2.0` license headers/identifiers to crate manifests
- [x] Write `README.md` (overview, architecture diagram, build instructions)
- [x] Write `CONTRIBUTING.md`
- [x] Add `deny.toml` (cargo-deny config blocking GPL/LGPL copyleft dependencies)
- [x] Set up GitHub Actions CI: build, test, clippy, fmt, cargo-deny across native, `wasm32-unknown-unknown`, and `thumbv7em-none-eabihf` targets
- [x] Fill in Cargo.toml metadata (authors, license, repository) for each crate
- [x] Push initial repo to GitHub — _done 2026-08-26: `origin` = `github.com/tpt-solutions/tpt-dsp`; all commits pushed through `63710a7` (Phase 5 adoption/DX tooling). Working tree clean._

---

## Phase 1: Core Math & Audio MVP (Months 1-3)

### tpt-dsp-core
- [x] Complex number math (num-complex integration, IQ data support)
- [x] FFT (rustfft integration) — `plan.rs` + hand-rolled radix-2 `fft.rs` (power-of-2 only)
- [x] Discrete Cosine Transform (DCT) — DCT-II/III/IV, direct O(N²) — _2026-09-07: added `FastDctIvPlan` (FFT-backed, `std`/`f32`-only), verified against the direct reference; also fixed a precision bug in the direct `cos()` evaluation, see below_
- [x] Modified DCT (MDCT/IMDCT) — `mdct.rs`, direct O(N²) reference (Princen-Bradley convention, `MdctPlan`) plus `FastMdctPlan` (FFT-backed, `std`/`f32`-only) — _added 2026-09-07 for `tpt-media`'s Pulse/Aura codecs (see their `todo.md` §5.1); not originally tracked here_
- [x] Linear Predictive Coding (LPC) — `lpc.rs`: `autocorrelation`, `levinson_durbin`, `predict`, `residual`, `synthesize`, `LpcAnalyzer` — _added prior to 2026-09-07 for `tpt-media`'s Aura/Vox codecs (see their `todo.md` §6); not originally tracked here either_
- [x] Pitch estimation — `pitch.rs`: `autocorrelation_pitch`, `amdf_pitch`, `hz_range_to_period_samples`, `PitchEstimate` (voiced/unvoiced + confidence) — _added 2026-09-07 for `tpt-media`'s Vox codec (see its `todo.md` §7); not originally tracked here_
- [x] Vector quantisation (VQ) — `vq.rs`: `nearest_vector` (+ early-exit-accelerated variant), `vq_encode`/`vq_decode`, `DistanceMetric` (squared-Euclidean/Manhattan), `VqCodebook` — _added 2026-09-07 for `tpt-media`'s Vox/Whisper codecs (see its `todo.md` §8); not originally tracked here. Tree/product-quantisation for larger codebooks intentionally not implemented — no concrete consumer yet to validate the requirements against_
- [x] Line Spectral Pairs/Frequencies (LSP/LSF, `alloc`-gated) — `lsp.rs`: `lpc_to_lsp`/`lsp_to_lpc` (direct evaluate-and-bisect root search, not the classical Chebyshev recursion), `lsp_is_stable`, `lsp_interpolate`, `lsp_quantize_uniform` (guards against quantization collisions producing an unstable filter) — _added 2026-09-07 for `tpt-media`'s Vox codec (see its `todo.md` §9); verified round-trip at LPC orders 2-32_
- [x] Bark/ERB perceptual frequency scales — `bark.rs`: `hz_to_bark`/`bark_to_hz` (Traunmüller), `erb_bandwidth`/`hz_to_erb_rate`/`erb_rate_to_hz` (Glasberg-Moore), `fft_bin_bark_bands`, `aggregate_bands` — _added 2026-09-07 for `tpt-media`'s Aura psychoacoustic model (see its `todo.md` §10.1)_
- [x] Psychoacoustic model — `psychoacoustic.rs`: `threshold_in_quiet` (Terhardt), `is_tonal_peak`/`classify_tonal_bins`, `spreading_function_db` (Schroeder), `simultaneous_masking`, `combined_threshold_db`, `perceptual_weight_db` — _added 2026-09-07 for `tpt-media`'s Aura psychoacoustic model (see its `todo.md` §10.2); a generic model using well-known closed-form pieces, not a byte-for-byte reproduction of any one standard_
- [x] Noise shaping — `noise_shaping.rs`: `noise_shape_step` (configurable-order error-feedback filter), `noise_shaper_is_stable` (validates an arbitrary `quantize` closure's error-boundedness under feedback), `NoiseShaper` (`alloc`-gated owning wrapper) — _added 2026-09-07 for `tpt-media`'s Aura lossy mode (see its `todo.md` §11). Caught a real sign error mid-implementation: the fed-back error must be subtracted, not added, to get the classic `NTF=1-z⁻¹` telescoping/bounded-cumulative-error property — the first (additive) version passed compilation but produced degenerate fixed-point/limit-cycle behavior for constant inputs, only caught by a DC-tracking test_
- [x] Hilbert transform
- [x] Windowing functions (Hann, Hamming, Blackman) — _f32-only; no rect/Kaiser/Tukey/flat-top_
- [x] Convolution — direct + FFT `ConvolvePlan` + overlap-add `FftConvolver`
- [x] Biquad filters (Low-pass, High-pass, Band-pass, Notch, All-pass) + Shelf/Peaking
- [x] FIR filter implementation (windowed-sinc design)
- [x] IIR filter implementation
- [x] Lock-free, pre-allocated ring buffers
- [x] SPSC queues (crossbeam-based) — _`split` consumes `self`, dropping the producer disconnects the consumer_
- [x] Unit tests + zero-allocation verification for core math/filters/buffers — _109 tests / 0 failures_

### tpt-dsp-audio
- [x] Audio graph node system (sources → effects → sinks)
- [x] Oscillators (basic waveforms)
- [x] Wavetable synthesis engine
- [x] FM synthesis engine — _2-operator only_
- [x] Subtractive synthesis engine
- [x] Waveshaping / distortion effect
- [x] Delay effect
- [x] Convolution reverb (pre-allocated impulse response buffers)
- [x] EQ (biquad-based)
- [x] Real-time callback engine with strict deadline guarantees (128/256-sample blocks)
- [x] Unit tests for audio graph & effects

### MVP 1: Web-Native Guitar Effects Pedal
- [x] WASM build target setup (wasm-bindgen) — `tpt-dsp-wasm` crate builds for `wasm32-unknown-unknown`
- [x] Web Audio API integration — `web.rs` (`register_worklet`, `create_pedal_node`, `connect_stream`, `open_microphone`)
- [x] Pedalboard UI (distortion, delay, reverb, EQ chain) — `www/index.html` + `www/main.js` + `www/pedal-processor.js` (added 2026-08-07)
- [x] Zero-glitch verification (no allocation inside 128-sample callback) — `pedalboard::tests::process_block_128_does_not_allocate` (counting allocator probe)
- [x] Deploy to GitHub Pages — `.github/workflows/pages.yml` builds the wasm pkg and publishes `www/` (needs repo + Pages "GitHub Actions" source enabled)
- [ ] **Milestone: Phase 1 MVP released** — _code complete; pending push to GitHub + Pages enablement_

---

## Phase 2: Analysis & Streaming MVP (Months 4-6)

### tpt-dsp-analysis
- [x] Real-time FFT averaging — `RealtimeSpectrumAnalyzer`: window → FFT → one-sided magnitude → dB → exponential/linear/time-constant averaging + parabolic peak interpolation
- [x] Peak detection
- [x] Spectrogram / waterfall generation
- [x] Moving averages
- [x] Exponential smoothing
- [x] Outlier detection for noisy sensor data
- [x] Zero-crossing rate
- [x] RMS energy calculation
- [x] Spectral centroid calculation
- [x] tokio / async-std adapters for streaming pipelines — runtime-agnostic `futures` glue + `async-tokio` / `async-std` feature-gated channel adapters
- [x] Unit tests for analysis features

### tpt-dsp-io
- [x] Audio I/O via cpal
- [x] Serial port handling (microcontroller / SDR dongle telemetry)
- [x] Raw USB/TCP streaming integration (rtlsdr bindings or raw TCP) — `tcp.rs` + `rtlsdr.rs` (stubbed backend) + `iq.rs` reassembler + `source.rs` trait
- [x] RTL-SDR IQ source trait + synthetic source (`iq.rs`, `source.rs`)

### Core Optimization
- [x] `portable-simd` optimization for FFT — `simd.rs` vectorised butterfly (nightly `core::simd`)
- [x] `portable-simd` optimization for complex number math — `simd.rs` `complex_mul/add`/`magnitude` (`core::simd`)
  - _Stable builds: the `simd` feature now degrades to the scalar fallback (`simd_scalar.rs`) via a build-script `tpt_portable_simd` cfg, so `cargo build --all-features` compiles on stable (fixed 2026-08-07)._

### MVP 2: SDR Spectrum Analyzer & FM Demodulator
- [x] RTL-SDR IQ data ingestion (2.4M complex samples/sec) — `tpt-dsp-io` `iq`/`source`/`rtlsdr` + `tpt-dsp-core` `demod`
- [x] FIR decimation filters for channel selection — `FIRDecimator` in `resample.rs`
- [x] FM demodulation (phase delta calculation) — `FmDemodulator` in `demod.rs`
- [x] Real-time waterfall spectrum rendering (desktop UI) — _`tpt-dsp-viz` is now implemented (2026-08-07): `VizApp` (`eframe::App`) renders a scrolling `Spectrogram` waterfall (colour-mapped by `colormap`: black → blue → cyan → yellow → red) plus a live dB spectrum line, with peak frequency/dB readout and a pause toggle. A producer thread streams `SpectrumFrame`s over a `crossbeam-channel`: `run_synthetic` (deterministic multi-tone + noise) runs with no hardware, `run_audio_input` (under the `audio` feature) captures the default input device via `cpal` (F32/I16/U16, mono downmix). Sub-tasks:_
  - [x] `pipeline.rs`: `SpectrumFrame` + `analyze_block` (windows/transforms/averages one block via `RealtimeSpectrumAnalyzer`, allocates only the per-frame `Vec<f32>` sent over the channel — same known tradeoff as the `tpt-dsp-analysis` async adapters, not a hot-path violation)
  - [x] `pipeline.rs`: `SyntheticGenerator` + `run_synthetic` — deterministic multi-tone + noise demo signal on a paced background thread, so the app runs with no hardware attached
  - [x] `pipeline.rs` (`audio` feature): `run_audio_input` — `cpal` default input device capture, mono downmix, F32/I16/U16 format handling
  - [x] `colormap.rs`: dB → heat-map `Color32` gradient (black → blue → cyan → yellow → red) + unit tests
  - [x] `app.rs`: `VizApp` (`eframe::App` impl) — waterfall texture from `Spectrogram`, live spectrum line via `egui` painter, peak frequency/dB readout, pause toggle, source label
  - [x] `lib.rs`: wire modules + `pub fn run()` (spawn pipeline thread, `eframe::run_native`); `main.rs` calls it
  - [x] Unit tests for `analyze_block` (known sine block → expected peak bin/dB) and `SyntheticGenerator` determinism/bounds
  - [x] Manually run the app (`cargo run -p tpt-dsp-viz` and `--features audio`) and confirm the waterfall/spectrum line actually render — _done 2026-08-26 on a desktop with display + webcam mic: synthetic waterfall renders correctly, and `--audio` mode shows a live spectrum reacting to microphone input_
  - [x] Update `ARCHITECTURE.md` §4.2, which previously listed "the desktop waterfall UI" under **Not yet implemented** / **Unstarted roadmap** (stale)
- [x] Frame-drop-free continuous streaming verification — `streaming` integration test (`synthetic_stream_runs_frame_drop_free_at_full_rate`)
- [x] **Milestone: Phase 2 MVP released** — _waterfall UI implemented; live render check passed 2026-08-26 (synthetic + live audio input)_

---

## Phase 3: Control, Embedded & Ecosystem Maturation (Months 7-12)

### tpt-dsp-control
- [x] PID controller with anti-windup
- [x] Input shaping (mechanical resonance cancellation) — ZVD
- [x] Kinematics: real-time trajectory planning — `TrapezoidalProfile`
- [x] Kinematics: jerk-limiting for stepper/servo motors — `JerkLimiter`
- [x] Unit tests for control loops

### no_std / Embedded
- [x] Verify `tpt-dsp-core` is fully `no_std` compliant — `cargo build -p tpt-dsp-core --no-default-features` (and `+alloc`) succeed
- [ ] Test on ARM Cortex-M microcontroller via `embedded-hal` — _requires physical hardware; not runnable in CI/this environment_
- [x] CI target: `thumbv7em-none-eabihf` build verification — job in `ci.yml` builds + clippy

### Documentation & Release
- [x] Comprehensive API documentation (rustdoc, docs.rs-ready)
- [x] Architecture/design guide — `ARCHITECTURE.md`
- [x] Benchmark suite vs JUCE — _JUCE is C++; documented as deferred in `BENCHMARKS.md`. Comparable pure-Rust `rubato` resampler benchmark added._
- [x] Benchmark suite vs libsamplerate — _libsamplerate is C; deferred (documented). `rubato` used as the pure-Rust analog._
- [x] Publish benchmark comparison report — `BENCHMARKS.md` (criterion suites in `tpt-dsp-core/benches`, `tpt-dsp-audio/benches`)
- [x] Final license/dependency audit (full `cargo-deny` pass) — _`cargo deny --all-features check` green (OFL-1.1 / Ubuntu-font-1.0 added for egui's bundled fonts, 2026-08-07)_
- [ ] v0.1.0 release on crates.io — _blocked on publish credentials; code is release-ready_
- [ ] **Milestone: Phase 3 complete — v0.1.0 published**

---

## Ongoing / Cross-Cutting

- [x] Run `cargo-deny` on every new dependency addition — _CI `deny` job runs default + `--all-features` on push/PR and a weekly schedule_
- [x] Keep `no_std` compatibility verified for `tpt-dsp-core` as features are added — _CI `embedded` job; `simd` feature fixed to not break stable builds_

---

## Phase 4: Robustness, Security Hardening & Adoption Tooling (2026-08-19)

_From a full-project review: security audit, stub inventory, and adoption/DX assessment. See the review's plan for detail; tracked here for follow-through._

### Robustness fixes
- [x] `tpt-dsp-analysis/src/spectrum.rs` `peak_bin()` — replaced `partial_cmp().unwrap()` (panics on NaN) with `total_cmp` so malformed IQ-derived spectra can't crash a real-time analysis thread
- [x] Add a regression test for `peak_bin` with a NaN-containing input
- [x] `tpt-dsp-viz/src/pipeline.rs:249,263,277` — `state.lock().unwrap()` in `cpal` audio-input callbacks; switch to `.lock().unwrap_or_else(|e| e.into_inner())` so a mutex poisoned by an unrelated panic doesn't crash every subsequent audio callback
- [x] `tpt-dsp-io/src/iq.rs` `IqStream::feed`/`IqStream` — document that the buffer grows unboundedly if the caller never calls `drain` (not exercised by `tcp.rs`, which uses the bounded `IqReassembler` path instead)

### CI security hardening
- [x] `.github/workflows/ci.yml` — add a top-level `permissions: contents: read` block (defense-in-depth; matches the least-privilege pattern already used in `pages.yml`)

### Adoption / DX tooling
- [x] Add a root `justfile` (`just ci`, `just test`, `just examples`) to de-duplicate the command list currently repeated across README/CONTRIBUTING.md/AGENTS.md
- [x] Add `CHANGELOG.md` (Keep-a-Changelog format, `[Unreleased]` section) ahead of the eventual v0.1.0 crates.io publish
- [x] Add `.github/ISSUE_TEMPLATE/bug_report.md` + `feature_request.md` and `.github/PULL_REQUEST_TEMPLATE.md`
- [x] Add an `examples/` directory + one runnable example each for `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-control` (only `tpt-dsp-io` has one today)
- [x] Root README — link the `www/` pedalboard demo (once Pages is live) and add a short positioning note vs. `fundsp`/`dasp`/`cpal`

### Noted for a future, separate pass (now implemented as separate crates)
- `tpt-dsp-cli` — terminal tool to pipe WAV/IQ files through filters/FFT/analysis. **Done** (workspace member: `filter`, `demod`, `spectrum`, `info` subcommands).
- `tpt-dsp-nihplug` — CLAP/VST3 wrapper example around `tpt-dsp-audio` (pedalboard: Waveshaper → Delay → ConvolutionReverb → 3-band EQ). **Done** (standalone workspace using nice-plug — the maintained successor to nih-plug).
- `tpt-dsp-py` — pyo3 Python bindings for analysis/core. **Done** (standalone workspace; `tpt_dsp` extension module: `rms`, `zero_crossing_rate`, `spectral_centroid`, `spectrum`, `fm_demod`, `analyze`).

---

## Phase 5: Adoption & DX (2026-08-26)

_From a platform review: the codebase and its own status docs are unusually
well-tracked already (see "Known Gaps" below); this phase captures adoption/DX
gaps that were not previously tracked._

- [x] `docs/QUICKSTART.md` (or a new README section) — a single runnable
  "clone → build → hear/see output" path, e.g. `just example tpt-dsp-audio
  synth_eq`, distinct from the README's isolated code snippets
  _(added 2026-08-26: `docs/QUICKSTART.md`, covering build, every crate's
  example, library usage and the local wasm pedalboard)_
- [x] Comparison table in the README (columns: `no_std`, real-time guarantee,
  RF/SDR support, plugin export) against `cpal`/`dasp`/`fundsp`/JUCE,
  alongside (not necessarily replacing) the existing prose comparison
  _(added 2026-08-26: table at the top of "How does tpt-dsp compare?", with
  the former bullet points kept as prose notes below it)_
- [x] `cargo-generate` template (or a `templates/` dir with
  `cargo-generate.toml`) for a new effect crate / CLI pipeline skeleton,
  pre-wired to the workspace's zero-alloc scratch pattern and
  `#![warn(missing_docs)]` convention
  _(added 2026-08-26: `templates/dsp-effect-crate/` — use with
  `cargo generate --path templates/dsp-effect-crate -n my-effect`; excluded
  from the workspace in root `Cargo.toml` because of its `{{crate_name}}`
  placeholders)_
- [x] Minimal `tpt-dsp-viz` example (smallest custom-waterfall usage) —
  `core`/`audio`/`analysis`/`control`/`io` each have one `examples/` entry;
  `viz` currently has none of its own
  _(added 2026-08-26: `tpt-dsp-viz/examples/custom_waterfall.rs`, driving
  `VizApp` directly over a bounded channel with `run_synthetic`)_
- [x] README — move the GitHub Pages demo link/badge to the top of the file
  (once Pages is enabled), not only under "Web demo"
  _— done 2026-08-26: demo confirmed live; link added at the top of `README.md`._

### Automation & dependency hygiene
- [x] `AGENTS.md` — fixed the stale "`tpt-dsp-viz` is an empty stub" paragraph
  (viz has been implemented since 2026-08-07; the doc hadn't been updated)
- [x] Ran `cargo machete` against the workspace — found and removed unused
  `tpt-dsp-core` dependencies from `tpt-dsp-control`, `tpt-dsp-viz`, and
  `tpt-dsp-wasm` (`control`'s was already noted in `ARCHITECTURE.md` §7; the
  other two were new findings). Also dropped the now-dangling `tpt_dsp_core`
  intra-doc link from `tpt-dsp-viz/src/lib.rs`. Verified `cargo machete`
  reports clean and `cargo build --workspace --all-features` still succeeds.
- [x] Add a `cargo-machete` step to `.github/workflows/ci.yml` (new `machete`
  job, mirroring the `deny` job's `taiki-e/install-action` pattern) now that
  the workspace passes it clean, so unused deps don't creep back in
- [x] Add `.github/dependabot.yml` — `cargo` ecosystem for the root workspace
  plus the excluded `tpt-dsp-nihplug`/`tpt-dsp-py` standalone workspaces
  (weekly), and a `github-actions` ecosystem entry for `ci.yml`/`pages.yml`
  action version bumps

---

## Phase 6: Platform Review Follow-ups (2026-11-06)

Source: static review of 0.2.0 (nothing built/run). Reproduce each bug with a failing test first. Plan: `~/.claude/plans/pasted-content-id-87af-review-platform-polymorphic-dream.md`.

_Status (2026-11-06, this pass): sections A and B and the housekeeping item are worked through with tests; C, D, F and most of E are not started. Items that need outside access (publishing, hardware, GitHub/PyPI) are left open. Regression tests were written for each A item. Only two were run against the unfixed code and seen to fail: the oscillator hang (the test never returned) and the envelope's NaN output. The rest were checked against the fixed code only._

### A. Bugs & robustness
- [x] Oscillator/Wavetable phase wrap hang (`tpt-dsp-audio/src/oscillator.rs:91`, `wavetable.rs:99`): `wrap_phase` (constant-time, `util.rs`) replaces the `while` loop; non-finite/negative frequency → 0, invalid sample rate ignored; same for `fm.rs`. `envelope.rs`: non-finite times/sustain sanitised, `set_sample_rate` added.
- [x] `OutlierDetector` NaN panic (`tpt-dsp-analysis/src/timeseries.rs`): `total_cmp`. `Ema::new` now rejects `alpha = 0`.
- [x] WAV parser hardening (`tpt-dsp-io/src/wav.rs`): chunk bodies read via `take` (no up-front allocation from a declared size), RIFF pad byte consumed for `fmt `/`data`, partial trailing frames dropped, writer uses checked arithmetic and rejects `len % channels != 0`.
- [~] `try_*` constructors: **done** for `BiquadCoeffs::try_design` (f0 > Nyquist, q <= 0, non-finite gain), `FirDesign::try_design` (even/zero taps; single tap no longer divides by zero), `Fir::try_new` (empty taps), `Pid::try_new`, `TrapezoidalProfile::try_new` (`q0 == q1` now valid, zero duration), `InputShaper::try_new_zvd` (too-low sample rate is an error, not a silent degenerate shaper), `Eq::try_new`. `Eq::try_with_shelves` added in a later pass.
- [x] Remove first-call allocation: `IirFilter::with_max_block` / `Eq::with_max_block`; `new` reserves a default 1024-sample block.
- [x] `Subtractive::tick` redesigns the biquad every 32 samples instead of every sample; the clamp can no longer panic at very low sample rates.
- [x] Denormal/NaN guards: biquad state scrubbed once per block (`process_biquad`), `Delay` feedback flushed, `ConvolutionReverb` input/output sanitised, `RealtimeSpectrumAnalyzer` treats non-finite bins as silence for that frame.
- [x] `Linear` averaging: blend floored at `1/LINEAR_EXACT_FRAMES` (2^16) so the average keeps tracking; exact cumulative mean below that.
- [x] `Delay`: fractional delay (linear interpolation) and a glided target so time changes don't click.
- [x] `ConvolutionReverb` short final block: now accumulates input to whole blocks, so the stream stays continuous across calls of any length. Output has a fixed one-block latency (dry signal delayed to match) — this is a behaviour change, documented in the type's docs.
- [x] PID: derivative on measurement (no kick), optional `set_derivative_filter`, `update_with_dt`; `InputShaper::new_zvd` degenerate case fixed (see `try_*` above).
- [~] Add `set_sample_rate` to `Adsr`, `Subtractive`, `Eq` — **done**. `Delay` **not added on purpose**: it stores time in samples and has no sample-rate state; `set_delay_seconds` already takes the rate per call.
- [x] Housekeeping: logs, `audio_check*.txt` and `gate.txt` untracked and gitignored; `tpt-dsp-core/tests/debug_biquad.rs` removed (its all-pass check is covered by `allpass_preserves_magnitude`); `history/` and `spec.txt` moved to `docs/archive/` (AGENTS.md updated).

### B. Doc & status drift
- [x] `README.md` says 0.2.0 now; badges added (crates.io, docs.rs, CI, license, MSRV 1.85)
- [x] `ARCHITECTURE.md`: wasm/Pages status corrected, fast DCT/MDCT noted, Linux/macOS backends noted, "eight crates" instead of "five"; zero-alloc note updated for reserved scratch
- [ ] Tidy this file: stale items at "Phase 1 MVP released"/"v0.1.0 on crates.io"/"Phase 3 complete"; merge "Known Gaps" duplicates; remove mid-file "(End of file)" — _partly done: the v0.1.0 publish is superseded by 0.2.0 (see `PUBLISHING.md`); the remaining clean-up of the Phase 1–3 bullets and the "Known Gaps" section is not done_
- [ ] Track: GitHub Release for v0.2.0 (manual, `gh` not installed), PyPI release for `tpt-dsp-py`, real RTL-SDR driver (or document `rtl_tcp` as the supported path)
- [x] `ci.yml`: cpal comments removed and `libasound2-dev` dropped (the audio backend has no system library; **not yet confirmed by a CI run**); `CHANGELOG [Unreleased]` records the eframe 0.37 bump; `nihplug`/`py` manifests now pin core 0.2.0

### C. Missing DSP features
- [~] Filter design — **mostly done** (`tpt-dsp-core/src/iir_design.rs`, `filters.rs`): `butterworth` (LP/HP, any order), `chebyshev1_lowpass`, `chebyshev2_lowpass`, `OnePole`, `DcBlocker`, `Svf` (TPT, LP/BP/HP), `FirDesign::try_design_windowed` (firwin with any window), `halfband_fir`, `magnitude_response` / `cascade_magnitude_response`. `remez_fir` (Parks–McClellan equiripple FIR, `remez.rs`), checked against scipy's `remez`: low-pass taps agree to 5e-6. `cascade_group_delay` (checked against scipy's `group_delay` to 0.01 samples) and `cascade_poles_zeros` (returns a `PolesZeros` struct). `elliptic_lowpass` (Cauer, `iir_design.rs`), built on Orfanidis's construction with AGM-based Jacobi functions and a bisection for the degree equation. Checked against scipy's `ellip` (orders 1–5): magnitudes agree to 0.0003 dB. `elliptic_highpass` is the same prototype under s → ωp/s; it matches scipy's `ellip(btype="high")` to 0.005 dB (normalised to unit gain at Nyquist). `elliptic_bandpass` (order doubling via `s → (s² + ω₀²)/(Bs)`) matches scipy's `ellip(btype="band")` to 0.00075 dB. The elliptic family is now complete (low-pass, high-pass, band-pass). `butterworth_bandpass` and `chebyshev1_bandpass` share the same LP→BP mapping (`bandpass_sections`) and match scipy's `butter`/`cheby1` band-pass to 0.00007 dB. _Outstanding in filter design: Chebyshev II band-pass, and band-stop forms of every family._
- [x] Windows — `WindowType` gained `Rectangular`, `FlatTop`, `Kaiser(beta)`, `Tukey(alpha)`, `Gaussian(sigma)`; f64 (`windowed_f64`) and symmetric (`windowed_symmetric`, `windowed_symmetric_f64`) variants. Kaiser benchmark added to `dsp_bench.rs`; the unmeasured `window/kaiser_f32` figure in `BENCHMARKS.md` was replaced with "not yet measured" (it had no backing benchmark).
- [~] Multirate/RF — **mostly done**: `Goertzel`, `Nco`, `CicDecimator` (`rf.rs`); `RationalResampler` (polyphase L/M, Kaiser-windowed) and `FarrowDelay` (cubic Lagrange, Farrow form) (`polyphase.rs`); `Pll` (complex or Costas-BPSK detector), `Agc`, `Squelch` (hysteresis), `AmDemodulator`, `SsbDemodulator` (`receiver.rs`). Half-band FIR is in `filters.rs`. **Outstanding:** the Costas and PLL are not yet tuned for FSK or QAM, and there is no carrier-recovery demo beyond the unit tests.
- [x] Dynamics & modulation (`tpt-dsp-audio/src/dynamics.rs`, `modulation.rs`, `freeverb.rs`): `Compressor`, `Limiter` (no lookahead, instant attack), `Gate` (expander), `Lfo`, `Tremolo`, `Chorus`, `Flanger`, `Phaser`, and `Freeverb` (mono, 8 combs + 4 all-passes). _Not done: Freeverb's stereo spread; an FDN reverb is not implemented (Freeverb was chosen instead)._
- [x] Synthesis — `PolyBlepOscillator` (saw, square), `poly_blep`, `PinkNoise`, `TpdfDither`, `NoiseSource` (`synthesis.rs`); `Oversampler` (zero-stuff, FIR, nonlinearity at N× rate, decimate; allocation-free per sample; `oversampling.rs`). PolyBLEP still leaves some aliasing; the oversampler is the tool for nonlinear stages.
- [~] Analysis — **mostly done**: `spectral_flatness`, `spectral_rolloff`, `spectral_flux` (`features.rs`); `yin_pitch` (`yin.rs`); `Mfcc` and `hz_to_mel`/`mel_to_hz` (`mfcc.rs`); `LoudnessMeter` (`loudness.rs`, BS.1770 K-weighting in `f64`, checked against pyloudnorm's `DeMan` class to 0.001 LU on steady tones and 0.011 LU on a mixed-level programme; a measurement tool, not a certified meter); `stft`, `istft`, `time_stretch` (phase vocoder) (`stft.rs`). **Outstanding:** a peak-picking onset detector (only the flux measure exists), and a STFT-based pitch shifter (time-stretch exists).

### D. Innovation
- [x] `Processor` trait (`process`, `process_block`, `reset`) + `Chain` combinator (`tpt-dsp-core/src/process.rs`). Implemented for `Biquad`, `OnePole`, `DcBlocker`. `Cascade` (dynamic, boxed stages) is also in `process.rs`. _Not done: the f64 policy (the trait is f32-only)._
- [~] Declarative pipeline spec — **CLI part done**: `tpt-dsp-cli pipeline --spec chain.json` reads an ordered list of effects in JSON (`tools.rs`). Each entry becomes the compact `--effect` syntax and goes through `Effect::parse`, so the two forms cannot diverge (a test checks that). _Not done: TOML (the `toml` crate is not in the local registry), the same format for wasm and Python, and a `serde` derive (JSON is read with `serde_json::Value`)._
- [x] `tpt-dsp-testkit`: `CountingAllocator` + `assert_no_alloc` / `allocations_in`, with tests. The wasm pedalboard's `alloc_probe` now uses it (the module was removed; AGENTS.md and the PR template point to the testkit).
- [~] Golden-vector conformance tests — **scipy part done** (`tpt-dsp-core/tests/golden.rs`, vectors from `tests/golden/generate.py`): all seven symmetric windows at three lengths match scipy to 1e-7, and Butterworth, Chebyshev I and Chebyshev II magnitudes match scipy to 0.0002 dB across orders 2–5. The comparison found a real bug: Chebyshev I was not pre-warped and was off by up to 0.03 dB; fixed. Remez taps too (see below). The LUFS meter has its own reference, pyloudnorm (`tpt-dsp-analysis/tests/golden_lufs.rs`); that check also found the meter was 2.7 LU low at 1 kHz because the K-weighting was an RBJ approximation, now replaced with the BS.1770 coefficients. _Not done: librosa-based MFCC vectors (librosa's MFCC uses different mel and normalisation conventions, so it would test the convention, not the algorithm)._
- [x] `Smoothed<F>` parameter smoother (`process.rs`), generic over `Float`.
- [ ] Browser filter designer in `www/` (response plot + export coefficients / Rust snippet) — _not started: needs a UI and the `magnitude_response` function exposed to wasm._

### E. Automation & CI
- [x] CI: concurrency group, `RUSTDOCFLAGS=-D warnings` on docs, MSRV (1.85) `cargo check` job, Windows test job, macOS build job, nightly `simd` job (continue-on-error), and a build job for `tpt-dsp-py` and `tpt-dsp-nihplug`. _None of the new jobs has run on a GitHub runner yet, so each may need a fix on first run._
- [ ] `cargo-semver-checks`, coverage (llvm-cov), `cargo bench --no-run` in CI, `cargo-hack` feature powerset — _not started._
- [~] `proptest` round trips — **done** for FFT, fast DCT, the ring buffer (vs a FIFO model) in `tpt-dsp-core/tests/proptest_roundtrips.rs`, and for the WAV writer/reader (exact round trip; the reader never panics on arbitrary bytes, including after a RIFF header) in `tpt-dsp-io/tests/proptest_wav.rs`. LPC → LSP → LPC round trips and sorted, in-band LSPs in `tpt-dsp-core/tests/proptest_lsp.rs`. _Not done: `cargo-fuzz` targets (they need nightly and `cargo-fuzz`)._
- [ ] Tag-triggered release workflow (or `release-plz`): publish in dependency order + GitHub Release — _not started; publishing is out of scope for this pass._
- [~] pin actions by SHA — **done for six actions** (`actions/checkout`, `Swatinem/rust-cache`, `actions/setup-python`, `jetli/wasm-pack-action`, `actions/upload-pages-artifact`, `actions/deploy-pages`), each with its tag as a comment. _Not pinned, on purpose: `dtolnay/rust-toolchain` and `taiki-e/install-action` read the toolchain or tool name from the ref, so a SHA would break them. Their refs are kept; pin them with an explicit `toolchain:` / `tool:` input if you want them pinned too._
- [x] Per-package `[package.metadata.docs.rs]` (all-features) on every member crate; `SECURITY.md` and `CODE_OF_CONDUCT.md` added.
- [x] `Cargo.lock`: kept gitignored for now. The library crates are what is published; revisit if the binaries (`tpt-dsp-cli`, `tpt-dsp-viz`) are distributed.
- [~] Benches for `analysis`, `control`, `io` — **added** (`analysis_bench`, `control_bench`, `io_bench`; criterion), and CI compiles all benches. _Not done: regression tracking (storing baselines and failing on a slowdown)._

### F. Adoption
- [x] `tpt-dsp` facade crate with prelude + feature flags (`audio`, `analysis`, `control`, `io`), default audio/analysis/control.
- [~] Walkthrough "WAV in -> filter -> analyse -> write" as a compiled doctest (`tpt-dsp/src/walkthrough.rs`, runs under `--all-features`). _Not done: a README examples table, and examples for wasm and embedded `thumbv7em`._
- [ ] nihplug "swap in your own effect" plugin template; extend `templates/dsp-effect-crate` — _not started._
- [x] CLI: `gen` (sine, saw, square, noise, chirp), `resample` (polyphase), `design` (Butterworth / Chebyshev I / II response CSV), `response` (effect-chain response from its impulse response, CSV), and `completions` (bash, zsh, fish, powershell, elvish). Library side in `tpt-dsp-cli/src/tools.rs`. Smoke-tested end to end.
- [~] Python (`tpt-dsp-py`): `pyproject.toml` (maturin), a `tpt_dsp` package over the compiled `tpt_dsp._native`, `_native.pyi` and `py.typed`, stateful `Biquad` and `Pid` classes, a stateful `Oscillator` (sine, saw, square, triangle), and 23 unittest tests. Two numpy interop tests run when numpy is installed (they skip otherwise). Built and tested locally on Windows (Python 3.13, abi3 wheel); the CI job that builds and tests it has not run yet. Fixed a pre-existing bug: pyo3 0.30 has no `abi3-py38` feature, so the crate now uses `abi3-py39`. _Not done: returning numpy arrays without copying (the bindings take and return lists), and PyPI publishing._
- [~] API ergonomics — **partly done**: `Biquad::builder(kind).sample_rate(..).frequency(..).q(..).gain_db(..).build()` (`BiquadBuilder`, `filters.rs`) gives named parameters with the same validation as the positional `design`. `FilterDesignError` is the shared error for the filter designs and the EQ. `tpt-dsp-control` no longer declares a `core` dependency (it was removed earlier; verified). _Not done: builders for the other designs (PID, profiles, shapers, delays), and a single error type across the crates, since each crate has its own error enum today._
- [x] Docs: `docs/CHOOSING_A_CRATE.md`, `docs/COOKBOOK.md` (ten recipes; code not compiled), `docs/RT_SAFETY.md`.

### Suggested order
1. Phase 1: A (oscillator, OutlierDetector, WAV), housekeeping, B, CI quick wins (Win/macOS/MSRV/semver, docs.rs)
2. Phase 2: rest of A, `Processor`/`Chain`, facade crate, `try_*` ctors, proptest + fuzz
3. Phase 3: C1-C4, CLI `design`, Python packaging, examples/templates
4. Phase 4: D, C5-C7

## Known Gaps (last updated 2026-08-20)

**Build/test health (all green)**
- `cargo test --workspace` — 159 native tests + 7 wasm-crate tests + doctests, 0 failures.
- `cargo clippy --workspace --all-targets --all-features -D warnings` — clean.
- `cargo build -p tpt-dsp-core --no-default-features` and `+alloc` — succeed.
- `cargo build -p tpt-dsp-wasm --all-features --target wasm32-unknown-unknown` — succeeds.
- `cargo deny --all-features check bans licenses advisories sources` — passes.
- `cargo build -p tpt-dsp-core --features simd` on stable — now uses scalar fallback (build-script `tpt_portable_simd` cfg); vectorised path still active on nightly.

**Zero-allocation contract**
- Verified for `IirFilter`, `Eq`, `ConvolutionReverb`, `OutlierDetector` (pre-allocated scratch), and the pedalboard hot path (`process_internal_block`, counting-allocator test). `OutlierDetector` is still O(n log n) per sample, not O(1).

**Still open / external blockers**
- Deploy web pedalboard to GitHub Pages — **done 2026-08-26: live at `https://tpt-solutions.github.io/tpt-dsp/`; demo link moved to the top of `README.md`.**
- `v0.1.0` publish to crates.io — needs publish token; all 7 crates are now implemented and release-ready (the `tpt-dsp-viz` desktop UI builds, tests and clips clean — a live render check on a GUI machine is the only remaining verification).
- Cortex-M `embedded-hal` validation — needs physical hardware; not runnable here.
- Benchmark report vs JUCE / libsamplerate — intentionally deferred (both are C libraries); `BENCHMARKS.md` documents the pure-Rust `rubato` comparison instead.
- `tpt-dsp-viz` is **implemented** (2026-08-07): `VizApp` renders a waterfall + live spectrum line from a producer thread, with a deterministic synthetic source and an optional `cpal` audio source (`audio` feature). A live (displayed) render check is still pending — this headless environment has no display.
- Three new crates added on 2026-08-20: `tpt-dsp-cli` (workspace member; `filter`/`demod`/`spectrum`/`info` over WAV/IQ), `tpt-dsp-nihplug` and `tpt-dsp-py` (both **excluded** from the main workspace in root `Cargo.toml` `exclude`). `tpt-dsp-nihplug` uses `nice-plug` (the maintained fork of `nih-plug`, which is no longer on crates.io) to export CLAP + VST3; `tpt-dsp-py` builds a `tpt_dsp` pyo3 extension module (`extension-module` + `abi3-py38`). Verified separately: `cargo build`/`clippy` inside each dir, and the py module imports and runs under Python 3.13.

(End of file)

## Dependency Licensing Policy (MIT/Apache-2.0 purity)

- [x] Replace `hound` (Apache-2.0-only) with the built-in RIFF/WAVE module in `tpt-dsp-io/src/wav.rs` - _done 2026-08-26: `read_wav_f32_path`/`write_wav_f32_path` (+ reader/writer variants) normalise to `f32`; PCM 8/16/24/32-bit and IEEE float 32/64 read, WAVE_FORMAT_EXTENSIBLE supported, 32-bit float written; CLI migrated; `hound` removed from `[workspace.dependencies]`._
## Long-term: clean-room audio I/O

- [ ] **Clean-room rewrite of `cpal` in pure Rust, built internally** - `cpal` is Apache-2.0-only, which blocks MIT-only redistribution when the `audio` feature is enabled. Status 2026-08-26: **`cpal` fully removed from the tree.** `tpt-dsp-io/src/audio/` now implements shared-mode WASAPI directly in-tree (`backend_windows.rs`: hand-declared COM vtables/GUIDs, event-driven render + capture, `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM`, Windows 10+) and a Linux backend (`backend_linux.rs`, 2026-08-26): raw ALSA UAPI ioctls on `/dev/snd/pcmC*D*p|c` — no libasound linkage, blocking `RW_INTERLEAVED` transfers, FLOAT_LE/S32_LE/S16_LE negotiation, XRUN recovery, device enumeration via a `/dev/snd` scan + `/proc/asound` card names (compile-checked on `x86_64-unknown-linux-gnu`; runtime validation still needs a Linux machine with sound hardware). Friendly names via the MMDevice property store and a device-selection API (`run_output_on_device` / `run_input_on_device`) landed 2026-08-26 for both platforms. **macOS backend landed 2026-08-26** (`backend_mac.rs`): CoreAudio AudioUnits — the default-output unit (`'auol'/'def '`) for playback with a mono `f32` client format, and a HALOutput unit (`'auol'/'hal '`) for capture (input I/O on element 1, render callback on output scope element 1 calling `AudioUnitRender` on bus 1) — all through hand-declared `extern "C"` bindings to the system `AudioToolbox`/`CoreAudio`/`CoreFoundation` frameworks; no `coreaudio-sys`, no wrapper crates. Device enumeration via `kAudioHardwarePropertyDevices` + `kAudioObjectPropertyName`, selection by exact name or case-insensitive substring. Compile-checked for `x86_64-apple-darwin`; all workspace gates remain green. Public API: `run_output`, `run_input`, `has_default_input`, `list_output_devices` / `list_input_devices`, plus the `_on_device` variants (all three platforms). **Windows runtime validation passed 2026-08-26** (desktop, Realtek + NVIDIA outputs, C922 webcam mic): device enumeration, default/specific output playback and default/specific input capture all verified — including a first-ever **release-profile** run, which exposed and fixed a release-only crash (see below). Remaining work: live runtime validation of the macOS and Linux backends on machines with real sound hardware.
- [x] Fix release-mode WASAPI capture crash found during live validation (2026-08-26) — `tpt-dsp-viz --audio` and `audio_check` under `--release` died with `0xC0000005` inside `ntdll.dll` (~heap-corruption signature), while debug builds were fine; every prior validation had only exercised debug builds. Root cause: COM vtable methods were invoked through a *variadic* transmuted function-pointer type (`extern "system" fn(*mut c_void, ...) -> HRESULT`), which is not ABI-safe under optimization. Fix: `vt_call!` now coerces each argument to a machine word (`ArgUsize` trait) and dispatches through exactly-typed **non-variadic** per-arity signatures (`call_vt`). Post-fix: repeated release runs of `audio_check` pass clean (exit 0, both captures ok) and `tpt-dsp-viz --audio` runs stable with a live mic-driven waterfall. All gates re-run green (`fmt`, `clippy -D warnings`, full `--all-features` test suite).
