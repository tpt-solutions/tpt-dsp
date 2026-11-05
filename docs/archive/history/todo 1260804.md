# tpt-dsp — Project TODO

A pure-Rust, real-time-safe DSP framework. Dual-licensed MIT / Apache-2.0. © TPT Solutions.

---

## Phase 0: Project & Repo Setup

- [ ] `git init` and add a Rust `.gitignore`
- [ ] Scaffold Cargo workspace with 5 member crates: `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`, `tpt-dsp-control`, `tpt-dsp-io`
- [ ] Add `LICENSE-MIT` and `LICENSE-APACHE` (copyright TPT Solutions)
- [ ] Add SPDX `dual MIT/Apache-2.0` license headers/identifiers to crate manifests
- [ ] Write `README.md` (overview, architecture diagram, build instructions)
- [ ] Write `CONTRIBUTING.md`
- [ ] Add `deny.toml` (cargo-deny config blocking GPL/LGPL copyleft dependencies)
- [ ] Set up GitHub Actions CI: build, test, clippy, fmt, cargo-deny across native, `wasm32-unknown-unknown`, and `thumbv7em-none-eabihf` targets
- [ ] Fill in Cargo.toml metadata (authors, license, repository) for each crate
- [ ] Push initial repo to GitHub

---

## Phase 1: Core Math & Audio MVP (Months 1-3)

### tpt-dsp-core
- [ ] Complex number math (num-complex integration, IQ data support)
- [ ] FFT (rustfft integration)
- [ ] Discrete Cosine Transform (DCT)
- [ ] Hilbert transform
- [ ] Windowing functions (Hann, Hamming, Blackman)
- [ ] Convolution
- [ ] Biquad filters (Low-pass, High-pass, Band-pass, Notch, All-pass)
- [ ] FIR filter implementation
- [ ] IIR filter implementation
- [ ] Lock-free, pre-allocated ring buffers
- [ ] SPSC queues (crossbeam-based)
- [ ] Unit tests + zero-allocation verification for core math/filters/buffers

### tpt-dsp-audio
- [ ] Audio graph node system (sources → effects → sinks)
- [ ] Oscillators (basic waveforms)
- [ ] Wavetable synthesis engine
- [ ] FM synthesis engine
- [ ] Subtractive synthesis engine
- [ ] Waveshaping / distortion effect
- [ ] Delay effect
- [ ] Convolution reverb (pre-allocated impulse response buffers)
- [ ] EQ (biquad-based)
- [ ] Real-time callback engine with strict deadline guarantees (128/256-sample blocks)
- [ ] Unit tests for audio graph & effects

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
- [ ] Real-time FFT averaging
- [ ] Peak detection
- [ ] Spectrogram / waterfall generation
- [ ] Moving averages
- [ ] Exponential smoothing
- [ ] Outlier detection for noisy sensor data
- [ ] Zero-crossing rate
- [ ] RMS energy calculation
- [ ] Spectral centroid calculation
- [ ] tokio / async-std adapters for streaming pipelines
- [ ] Unit tests for analysis features

### tpt-dsp-io
- [ ] Audio I/O via cpal
- [ ] Serial port handling (microcontroller / SDR dongle telemetry)
- [ ] Raw USB/TCP streaming integration (rtlsdr bindings or raw TCP)

### Core Optimization
- [ ] `portable-simd` optimization for FFT
- [ ] `portable-simd` optimization for complex number math

### MVP 2: SDR Spectrum Analyzer & FM Demodulator
- [ ] RTL-SDR IQ data ingestion (2.4M complex samples/sec)
- [ ] FIR decimation filters for channel selection
- [ ] FM demodulation (phase delta calculation)
- [ ] Real-time waterfall spectrum rendering (desktop UI)
- [ ] Frame-drop-free continuous streaming verification
- [ ] **Milestone: Phase 2 MVP released**

---

## Phase 3: Control, Embedded & Ecosystem Maturation (Months 7-12)

### tpt-dsp-control
- [ ] PID controller with anti-windup
- [ ] Input shaping (mechanical resonance cancellation)
- [ ] Kinematics: real-time trajectory planning
- [ ] Kinematics: jerk-limiting for stepper/servo motors
- [ ] Unit tests for control loops

### no_std / Embedded
- [ ] Verify `tpt-dsp-core` is fully `no_std` compliant
- [ ] Test on ARM Cortex-M microcontroller via `embedded-hal`
- [ ] CI target: `thumbv7em-none-eabihf` build verification

### Documentation & Release
- [ ] Comprehensive API documentation (rustdoc, docs.rs-ready)
- [ ] Architecture/design guide
- [ ] Benchmark suite vs JUCE
- [ ] Benchmark suite vs libsamplerate
- [ ] Publish benchmark comparison report
- [ ] Final license/dependency audit (full `cargo-deny` pass)
- [ ] v1.0.0 release on crates.io
- [ ] **Milestone: Phase 3 complete — v1.0.0 published**

---

## Ongoing / Cross-Cutting

- [ ] Run `cargo-deny` on every new dependency addition — block GPL/LGPL
- [ ] Keep `no_std` compatibility verified for `tpt-dsp-core` as features are added
