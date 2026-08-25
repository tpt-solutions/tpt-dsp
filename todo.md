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
- [x] Discrete Cosine Transform (DCT) — DCT-II/III/IV, direct O(N²)
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
  - [ ] Manually run the app (`cargo run -p tpt-dsp-viz` and `--features audio`) and confirm the waterfall/spectrum line actually render — _blocked: this headless environment has no display, so live rendering can't be verified here; code compiles, builds and unit-tests pass. Run on a machine with a GUI to confirm._
  - [x] Update `ARCHITECTURE.md` §4.2, which previously listed "the desktop waterfall UI" under **Not yet implemented** / **Unstarted roadmap** (stale)
- [x] Frame-drop-free continuous streaming verification — `streaming` integration test (`synthetic_stream_runs_frame_drop_free_at_full_rate`)
- [ ] **Milestone: Phase 2 MVP released** — _waterfall UI implemented; still blocked on a live render check (needs a display) plus pending push to GitHub_

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
- [ ] README — move the GitHub Pages demo link/badge to the top of the file
  (once Pages is enabled), not only under "Web demo"
  _— still blocked on the repo's Pages setting = "GitHub Actions"._

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
- Deploy web pedalboard to GitHub Pages — workflow exists and is pushed; verified 2026-08-26 that `https://tpt-solutions.github.io/tpt-dsp/` still returns 404, so the repo's Pages setting must be switched to "GitHub Actions" in the web UI (not doable via git). Once live, finish the last README task: move the demo link/badge to the top of the file.
- `v0.1.0` publish to crates.io — needs publish token; all 7 crates are now implemented and release-ready (the `tpt-dsp-viz` desktop UI builds, tests and clips clean — a live render check on a GUI machine is the only remaining verification).
- Cortex-M `embedded-hal` validation — needs physical hardware; not runnable here.
- Benchmark report vs JUCE / libsamplerate — intentionally deferred (both are C libraries); `BENCHMARKS.md` documents the pure-Rust `rubato` comparison instead.
- `tpt-dsp-viz` is **implemented** (2026-08-07): `VizApp` renders a waterfall + live spectrum line from a producer thread, with a deterministic synthetic source and an optional `cpal` audio source (`audio` feature). A live (displayed) render check is still pending — this headless environment has no display.
- Three new crates added on 2026-08-20: `tpt-dsp-cli` (workspace member; `filter`/`demod`/`spectrum`/`info` over WAV/IQ), `tpt-dsp-nihplug` and `tpt-dsp-py` (both **excluded** from the main workspace in root `Cargo.toml` `exclude`). `tpt-dsp-nihplug` uses `nice-plug` (the maintained fork of `nih-plug`, which is no longer on crates.io) to export CLAP + VST3; `tpt-dsp-py` builds a `tpt_dsp` pyo3 extension module (`extension-module` + `abi3-py38`). Verified separately: `cargo build`/`clippy` inside each dir, and the py module imports and runs under Python 3.13.

(End of file)
