# tpt-dsp — Project TODO

A pure-Rust, real-time-safe DSP framework. Dual-licensed MIT / Apache-2.0. © TPT Solutions.

_Last synced: 2026-08-07. Status reflects current code in `tpt-dsp-*/src`. See "Known Gaps" at the end of this file for important caveats (build failures, zero-alloc violations, unstarted MVPs)._

---

## Phase 0: Project & Repo Setup

- [x] `git init` and add a Rust `.gitignore`
- [x] Scaffold Cargo workspace with 5 member crates: `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-control`, `tpt-dsp-io`
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (copyright TPT Solutions)
- [x] Add SPDX `dual MIT/Apache-2.0` license headers/identifiers to crate manifests
- [x] Write `README.md` (overview, architecture diagram, build instructions)
- [x] Write `CONTRIBUTING.md`
- [x] Add `deny.toml` (cargo-deny config blocking GPL/LGPL copyleft dependencies) — _present but uses legacy `copyleft`/`unlicensed` keys; may error on modern cargo-deny_
- [x] Set up GitHub Actions CI: build, test, clippy, fmt, cargo-deny across native, `wasm32-unknown-unknown`, and `thumbv7em-none-eabihf` targets — _jobs exist but no_std, clippy, and test runs currently fail_
- [x] Fill in Cargo.toml metadata (authors, license, repository) for each crate
- [ ] Push initial repo to GitHub — _2 local commits only; working tree dirty, `tpt-dsp-io` source untracked_

---

## Phase 1: Core Math & Audio MVP (Months 1-3)

### tpt-dsp-core
- [x] Complex number math (num-complex integration, IQ data support)
- [x] FFT (rustfft integration) — `plan.rs` + hand-rolled radix-2 `fft.rs` (power-of-2 only)
- [x] Discrete Cosine Transform (DCT) — DCT-II/III/IV, direct O(N²)
- [x] Hilbert transform
- [x] Windowing functions (Hann, Hamming, Blackman) — _f32-only; no rect/Kaiser/Tukey/flat-top_
- [x] Convolution — direct + FFT `ConvolvePlan` + overlap-add `FftConvolver` (_no partitioned convolution; long IRs truncated_)
- [x] Biquad filters (Low-pass, High-pass, Band-pass, Notch, All-pass) + Shelf/Peaking
- [x] FIR filter implementation (windowed-sinc design)
- [x] IIR filter implementation — _cascade; `process` allocates a `Vec` per stage per block (not zero-alloc)_
- [x] Lock-free, pre-allocated ring buffers — _single-owner `&mut self` API; not actually shared cross-thread_
- [x] SPSC queues (crossbeam-based) — _fixed: `split` now consumes `self`, so dropping the producer disconnects the consumer and `recv` returns `Err` instead of hanging_
- [x] Unit tests + zero-allocation verification for core math/filters/buffers — _test suite now passes (deadlock fixed); `IirFilter`/`Eq`/`ConvolutionReverb`/`OutlierDetector` reuse pre-allocated scratch buffers so steady-state processing is allocation-free_

### tpt-dsp-audio
- [x] Audio graph node system (sources → effects → sinks) — _linear mono chain, no DAG/fan-in/fan-out/multi-channel_
- [x] Oscillators (basic waveforms) — _naive (aliasing) saw/square, no PolyBLEP_
- [x] Wavetable synthesis engine — _no band-limited mipmaps/morphing_
- [x] FM synthesis engine — _2-operator only, no feedback/algorithms_
- [x] Subtractive synthesis engine
- [x] Waveshaping / distortion effect — _no oversampling_
- [x] Delay effect — _integer delay only_
- [x] Convolution reverb (pre-allocated impulse response buffers) — _`process` allocates per block_
- [x] EQ (biquad-based) — _`process` allocates per call_
- [x] Real-time callback engine with strict deadline guarantees (128/256-sample blocks) — _buffer mgmt present; no timing/xrun instrumentation, not wired to I/O_
- [x] Unit tests for audio graph & effects

### MVP 1: Web-Native Guitar Effects Pedal
- [ ] WASM build target setup (wasm-bindgen)
- [ ] Web Audio API integration
- [ ] Pedalboard UI (distortion, delay, reverb, EQ chain)
- [ ] Zero-glitch verification (no allocation inside 128-sample callback)
- [ ] Deploy to GitHub Pages
- [ ] **Milestone: Phase 1 MVP released**

---

## Phase 2: Analysis & Streaming MVP (Months 4-6)

### tpt-dsp-analysis
- [ ] Real-time FFT averaging — `SpectrumAnalyzer` averages magnitude frames but never performs an FFT itself; no windowing/dB/interpolated peaks
- [x] Peak detection
- [x] Spectrogram / waterfall generation — _no dB/colormap scaling_
- [x] Moving averages
- [x] Exponential smoothing
- [x] Outlier detection for noisy sensor data — _`push` allocates + sorts per sample (not O(1))_
- [x] Zero-crossing rate
- [x] RMS energy calculation
- [x] Spectral centroid calculation
- [ ] tokio / async-std adapters for streaming pipelines — _tokio only; async-std missing; module not feature-gated (breaks no_std build)_
- [x] Unit tests for analysis features

### tpt-dsp-io
- [x] Audio I/O via cpal — _output only; hardcoded mono f32; no capture/duplex/device selection; no tests_
- [x] Serial port handling (microcontroller / SDR dongle telemetry) — _open/read only; no write/enumeration/timeout/framing_
- [x] Raw USB/TCP streaming integration (rtlsdr bindings or raw TCP) — _`tcp.rs` single-connection `serve_iq`; no cross-read buffering (mid-sample splits drop data); tests fail (no `enable_io`)_

### Core Optimization
- [ ] `portable-simd` optimization for FFT
- [ ] `portable-simd` optimization for complex number math

### MVP 2: SDR Spectrum Analyzer & FM Demodulator
- [ ] RTL-SDR IQ data ingestion (2.4M complex samples/sec)
- [ ] FIR decimation filters for channel selection
- [ ] FM demodulation (phase delta calculation) — _no FM demod anywhere_
- [ ] Real-time waterfall spectrum rendering (desktop UI)
- [ ] Frame-drop-free continuous streaming verification
- [ ] **Milestone: Phase 2 MVP released**

---

## Phase 3: Control, Embedded & Ecosystem Maturation (Months 7-12)

### tpt-dsp-control
- [x] PID controller with anti-windup — _derivative-on-error, fixed dt, f32_
- [x] Input shaping (mechanical resonance cancellation) — _ZVD only; no ZV/ZVDD/EI/multi-mode_
- [x] Kinematics: real-time trajectory planning — `TrapezoidalProfile` complete
- [x] Kinematics: jerk-limiting for stepper/servo motors — `JerkLimiter` is first-order velocity loop, not true S-curve
- [x] Unit tests for control loops

### no_std / Embedded
- [x] Verify `tpt-dsp-core` is fully `no_std` compliant — _fixed: `ConvolvePlan`/`IirStage` re-exports gated behind `alloc`; `tpt-dsp-analysis`' `async_adapters` gated behind the `async` feature_
- [ ] Test on ARM Cortex-M microcontroller via `embedded-hal`
- [x] CI target: `thumbv7em-none-eabihf` build verification — _job exists; `cargo build -p tpt-dsp-core --no-default-features` now succeeds_

### Documentation & Release
- [x] Comprehensive API documentation (rustdoc, docs.rs-ready) — _docs present but intra-links point at private modules_
- [ ] Architecture/design guide
- [ ] Benchmark suite vs JUCE — _no `benches/` in any crate_
- [ ] Benchmark suite vs libsamplerate
- [ ] Publish benchmark comparison report
- [ ] Final license/dependency audit (full `cargo-deny` pass)
- [ ] v1.0.0 release on crates.io
- [ ] **Milestone: Phase 3 complete — v1.0.0 published**

---

## Ongoing / Cross-Cutting

- [ ] Run `cargo-deny` on every new dependency addition — block GPL/LGPL — _not enforced in workflow; `deny.toml` uses legacy schema_
- [ ] Keep `no_std` compatibility verified for `tpt-dsp-core` as features are added — _currently broken_

---

## Known Gaps (last updated 2026-08-07; resolved items struck through)

**Build/test health**
- ~~`cargo test` **hangs forever** (deadlock in `spsc.rs`)~~ — **RESOLVED**: `SpscQueue::split` now consumes `self`, so dropping the producer disconnects the consumer. Full suite passes (109 tests, 0 failures).
- ~~`cargo clippy --workspace --all-features -D warnings` **fails**~~ — **RESOLVED**: replaced hand-rolled π/τ literals with `core::f64::consts`, fixed `needless_range_loop`, `unnecessary_operation`, `assign_op`, `explicit_counter_loop`, dead-code and `unused_mut` lints. `clippy --all-targets --all-features -D warnings` is clean.
- ~~`no_std` builds of `tpt-dsp-core` **fail**~~ — **RESOLVED**: `ConvolvePlan`/`IirStage` re-exports gated behind `alloc`; `tpt-dsp-analysis`' `async_adapters` gated behind the `async` feature. `cargo build -p tpt-dsp-core --no-default-features` succeeds.
- ~~`tpt-dsp-io` tests: 2 pass, 3 fail~~ — **RESOLVED**: U8 IQ scaling clarified to the standard `(byte-128)/128` mapping; `tcp` test runtime now enables IO + time. All 5 io tests pass.
- `cargo-deny` config used a legacy schema — **RESOLVED**: migrated `deny.toml` to the modern format and added explicit path-dep versions; `cargo deny check` passes (advisories/bans/licenses/sources ok).

**Zero-allocation contract violations** (despite docs claiming no-alloc hot paths)
- ~~`IirFilter::process` — `Vec` per stage per block~~ — **RESOLVED**: single reusable scratch buffer (re-allocates only when block length grows).
- ~~`Eq::process` — `Vec` per call~~ — **RESOLVED**: reusable scratch buffer.
- ~~`ConvolutionReverb::process` — `vec![0.0; bs]` per block~~ — **RESOLVED**: pre-allocated `block_in`/`block_out` scratch buffers (also fixes partial-final-block handling).
- ~~`OutlierDetector::push` — 2 `Vec`s + sort per sample~~ — **RESOLVED**: single reusable scratch buffer, in-place sort. (Note: still O(n log n) per sample, not O(1).)

**Still open / unstarted roadmap items:** SIMD optimizations (FFT + complex), MVP 1 (WASM guitar pedal UI + wasm-bindgen + GitHub Pages), MVP 2 (RTL-SDR ingestion, FIR decimation, FM demod, desktop waterfall, frame-drop-free streaming), benchmark report vs JUCE / libsamplerate (criterion benches added in `tpt-dsp-core/benches` and `tpt-dsp-audio/benches` but no comparative report yet), `embedded-hal` Cortex-M validation, crates.io publish (v1.0.0), `serde`/`rayon` usage.
