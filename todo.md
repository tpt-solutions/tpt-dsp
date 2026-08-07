# tpt-dsp — Project TODO

A pure-Rust, real-time-safe DSP framework. Dual-licensed MIT / Apache-2.0. © TPT Solutions.

_Last synced: 2026-08-07. Reconciled with the actual code in `tpt-dsp-*/src`. The codebase has advanced well beyond the previous "Last synced" snapshot — SIMD, RTL-SDR, FM demod, resampling, the `viz`/`wasm` crates, benchmark suites, the benchmark report and an architecture guide were all committed. This revision marks those as done and records the new web pedalboard UI + Pages deploy workflow. See "Known Gaps" at the end for genuinely open items and external blockers._

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
- [ ] Push initial repo to GitHub — _local commits only; blocked on remote/credentials. Run once the GitHub remote exists._

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
- [x] Real-time waterfall spectrum rendering (desktop UI) — `tpt-dsp-viz` (egui)
- [x] Frame-drop-free continuous streaming verification — `streaming` integration test (`synthetic_stream_runs_frame_drop_free_at_full_rate`)
- [ ] **Milestone: Phase 2 MVP released** — _code complete; pending push to GitHub_

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
- [ ] v1.0.0 release on crates.io — _blocked on publish credentials; code is release-ready_
- [ ] **Milestone: Phase 3 complete — v1.0.0 published**

---

## Ongoing / Cross-Cutting

- [x] Run `cargo-deny` on every new dependency addition — _CI `deny` job runs default + `--all-features` on push/PR and a weekly schedule_
- [x] Keep `no_std` compatibility verified for `tpt-dsp-core` as features are added — _CI `embedded` job; `simd` feature fixed to not break stable builds_

---

## Known Gaps (last updated 2026-08-07)

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
- Push initial repo to GitHub — needs a remote + credentials.
- Deploy web pedalboard to GitHub Pages — workflow exists; needs the repo's Pages setting = "GitHub Actions".
- `v1.0.0` publish to crates.io — needs publish token; code is release-ready.
- Cortex-M `embedded-hal` validation — needs physical hardware; not runnable here.
- Benchmark report vs JUCE / libsamplerate — intentionally deferred (both are C libraries); `BENCHMARKS.md` documents the pure-Rust `rubato` comparison instead.
- `tpt-dsp-viz` desktop waterfall is a standalone example app; not wired into a release binary/crate publish.

(End of file)
