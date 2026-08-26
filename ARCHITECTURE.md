# tpt-dsp — Architecture & Design Guide

> Scope note: this document describes the framework **as it currently exists in
> the source tree** (snapshot: 2026-08-07). The project is mid-development — the
> two headline MVPs (a web guitar pedal and an SDR spectrum analyzer) are **not
> yet complete applications**, but the DSP building blocks they need are largely
> present in the crates. Anything not yet implemented is marked **(planned)**.
>
> The crate is dual-licensed MIT / Apache-2.0 © TPT Solutions.

---

## 1. Overview & design principles

`tpt-dsp` is a pure-Rust, real-time-safe DSP framework split into five crates.
The guiding contract is:

- **Hot paths never allocate.** Processing functions take caller-supplied or
  pre-allocated buffers and reuse them across calls. Where a struct owns buffers,
  those buffers are allocated **once at construction**; every `process` / `tick`
  call is allocation-free in steady state.
- **`no_std` where it counts.** `tpt-dsp-core` builds for bare metal with
  `--no-default-features` (verified on `thumbv7em-none-eabihf`). Higher crates
  currently target `std` (see §6).
- **Pure, no `unsafe` in core.** `tpt-dsp-core` is `#![forbid(unsafe_code)]`.
- **Composable primitives.** Small, single-purpose types (filters, oscillators,
  analyzers) combine into graphs and pipelines.

---

## 2. Workspace layout & dependency direction

```
tpt-dsp-core      ← foundation, no_std, depends on nothing in this workspace
   ▲      ▲      ▲
   │      │      │
tpt-dsp- │ tpt-dsp-  tpt-dsp-
audio    │ analysis   io
   ▲      │      ▲
   │      │      │
   └── tpt-dsp-control (depends on core only; see §7 caveat)

        tpt-dsp-control
```

- `tpt-dsp-core` is a **leaf**: it depends only on external crates
  (`num-complex`, `num-traits`, and — under `std` — `rustfft`, `crossbeam-channel`).
- `audio`, `analysis`, `io` each depend **only on `core`**. They do **not**
  depend on each other (no audio↔analysis coupling in code today).
- `tpt-dsp-control` declares a dependency on `tpt-dsp-core` but (as of this
  snapshot) does not use it in its implementation — its math (PID, input
  shaper, kinematics) is self-contained `f32` code. See §7.
- The dependency graph is acyclic. `core` is the base of every path.

### Crate responsibilities

| Crate | Purpose | `no_std`? |
| --- | --- | --- |
| `tpt-dsp-core` | Math engine: complex helpers, FFT/DCT/Hilbert, windows, biquad/FIR/IIR, convolution, ring buffers, SPSC queues, FM demod, FIR decimation. | **Yes** (`--no-default-features`) |
| `tpt-dsp-audio` | Oscillators & synthesis, effects, audio graph, real-time engine. | No (uses `std::vec`) |
| `tpt-dsp-analysis` | Spectrum analysis, time-series stats, features, spectrogram/waterfall, async stream adapters. | No (uses `std`; `async` feature pulls `tokio`/`futures`) |
| `tpt-dsp-control` | PID, input shaping (ZVD), trajectory kinematics. | No |
| `tpt-dsp-io` | IQ byte-stream parsing (always available), built-in audio I/O, serial reader, async TCP IQ server (feature-gated). | No |

---

## 3. The real-time-safe / zero-allocation contract

### 3.1 Buffer ownership patterns

Two idioms are used throughout:

1. **Free functions on slices** — e.g. `fft(input, out, scratch)`,
   `process_biquad(coeffs, &mut state, input, output)`, `convolve(...)`. The
   caller owns all buffers; the function mutates in place. Never allocates.
2. **Structs with pre-allocated scratch** — e.g. `FftConvolver`,
   `HilbertTransformer`, `IirFilter`, `Eq`, `OutlierDetector`,
   `SpectrumAnalyzer`, `RealtimeSpectrumAnalyzer`, `AudioGraph`, `RealtimeEngine`.
   Buffers are created in `new(...)` and reused on every `process`.

### 3.2 Current zero-alloc limitations (reality check)

The crate docs and `README.md` claim "never allocates". That is **true in steady
state for most paths**, but the following caveats are real:

- **`IirFilter` / `Eq` scratch buffers**: re-allocated only when the block
  length *grows* beyond the previously seen size (`if self.scratch.len() < n {
  self.scratch = vec![...] }`). Within a fixed-block-size real-time loop this
  never triggers, so it is allocation-free in practice.
- **`OutlierDetector::push`** still performs an in-place `sort` of the window per
  sample → **O(n log n) per sample**, not O(1). It is allocation-free (single
  reusable scratch) but not constant-time.
- **Async adapters** (`process_channel`, `process_stream`, and the `async`
  module) allocate one `Vec<f32>` output buffer **per frame**. The DSP closure
  itself stays allocation-free; the plumbing is not.
- **`AudioGraph`** owns two `Vec<f32>` scratch buffers allocated at construction
  — its `run`/`tick` are allocation-free.
- **Hand-rolled `fft`/`ifft`** (`tpt-dsp-core/src/fft.rs`) recompute the twiddle
  table on every call (inside `fft_inplace` via `twiddles`). Not an allocation,
  but a redundant computation — for repeated transforms prefer the pre-planned
  `FftPlan` (RustFFT-backed) which caches the plan and reuses one scratch.

### 3.3 Concurrency primitives

- **`RingBuffer`** (`core`): fixed-capacity FIFO over a caller-supplied slice,
  lock-free via atomics (`no_std` safe). Uses a single-slack-slot scheme so a
  full ring is unambiguous; usable capacity is `storage.len() - 1`.
  API is **single-owner** (`&mut self`): it is *not* currently shared across two
  threads — the atomics make it reusable/movable but the public methods require
  exclusive access, so it is intra-thread hand-off, not a cross-thread SPSC by
  itself.
- **`SpscQueue`** (`core`, `std`-only, crossbeam-backed): the recommended
  thread-safe producer→consumer channel for handing audio/control blocks between
  an audio thread and a UI/control thread. `try_send`/`try_recv` are the
  real-time-safe (non-blocking, non-allocating) calls. `split()` consumes `self`
  into `Producer`/`Consumer` halves so dropping one end disconnects the other.

### 3.4 Safety

`core` forbids `unsafe`. Higher crates do not declare the forbid; they use only
safe std APIs.

---

## 4. MVP pipeline data flows

### 4.1 MVP 1 — Web guitar pedal  **(planned application; blocks exist)**

Target: a browser/WASM audio engine driving a pedalboard chain.

```
Source (Oscillator / mic) ─▶ Distortion (Waveshaper)
        ─▶ Delay ─▶ Reverb (ConvolutionReverb) ─▶ EQ (Eq) ─▶ Sink (built-in audio / Web Audio)
```

Building blocks already available in `tpt-dsp-audio` (all mono, in-place
`&mut [f32]` effects):

- `Waveshaper` (curve: `Tanh`/`HardClip`/`Cubic`/`Polynomial`) — distortion.
  No oversampling.
- `Delay` — feedback delay line, **integer delay only** (sample granularity).
- `ConvolutionReverb` — wraps `core::FftConvolver`; long IRs are truncated to
  the FFT length (no partitioned convolution).
- `Eq` — cascaded RBJ biquads (peaking + optional low/high shelves).
- `Oscillator`, `Wavetable`, `FmSynth`, `SubtractiveVoice` — sources/voices.
- `AudioGraph` — bundles one `Source`, a `Vec` of `AudioNode`s, and one `Sink`,
  driven block-by-block over two scratch buffers. **Linear mono chain only**: no
  DAG, fan-in/fan-out, or multi-channel support today.
- `RealtimeEngine` — drives a `FnMut(&[f32], &mut [f32])` per fixed block
  (constants `BLOCK_128`, `BLOCK_256`).

**Not yet implemented** (from `todo.md`): the `wasm32`/`wasm-bindgen` build
setup, Web Audio API integration, the pedalboard UI, and the GitHub Pages deploy.
The `wasm32-unknown-unknown` target is exercised only as a `cargo check` in CI,
not a working audio app.

> Note: effects expose `process(&mut [f32])` (in-place) whereas `AudioGraph`
> nodes use `AudioNode::process(&[f32], &mut [f32])`. Effects are wired into a
> graph via `ClosureNode` adapters, not by directly implementing `AudioNode`.

### 4.2 MVP 2 — SDR spectrum analyzer & FM demodulator  **(blocks mostly landed; app planned)**

Target: ingest RTL-SDR IQ, decimate, FFT, optionally FM-demod, render a
waterfall.

```
IQ bytes ─▶ parse_iq / IqStream ─▶ FIRDecimator (channel select)
   ─▶ FftPlan / RealtimeSpectrumAnalyzer ─▶ Spectrogram (waterfall)
   ─▶ (optional) FmDemodulator ─▶ audio out / analysis
```

Building blocks now present:

- **Ingestion (I/O):** `parse_iq(format, bytes, &mut [Complex32])` and the
  buffered `IqStream` (handles mid-sample splits across reads) in `tpt-dsp-io`.
  Formats: `U8`, `I16Le`, `I16Be`, `F32Le`. `serve_iq` provides an async TCP
  server (`tcp` feature).
- **Decimation:** `FIRDecimator` (`core::resample`) — integer-factor anti-alias
  decimation; state carries across arbitrarily-sized blocks.
- **Transform:** `RealtimeSpectrumAnalyzer` (`analysis`) windows a block, runs
  `FftPlan` (RustFFT, `core::plan`), folds to a one-sided magnitude spectrum,
  time-averages, and converts to dB; `peak()` returns sub-bin frequency via
  parabolic interpolation. `Spectrogram` stores the most recent `rows` frames as
  a pre-allocated ring for waterfall rendering.
- **FM demod:** `FmDemodulator` / `phase_delta` / `phase_to_audio`
  (`core::demod`) — phase-delta discriminator, `no_std`, `Copy`, allocation-free.
- **Desktop waterfall UI (`tpt-dsp-viz`):** implemented. A producer thread
  (`pipeline::run_synthetic` / `pipeline::run_audio_input`) streams
  [`SpectrumFrame`]s (one-sided dB spectrum + metadata) to the egui UI over a
  `crossbeam-channel`. `analyze_block` windows/transforms/averages one block via
  `RealtimeSpectrumAnalyzer`. `VizApp` (`app.rs`, an `eframe::App`) renders the
  `Spectrogram` as a scrolling `egui` texture (colour-mapped by `colormap` —
  black → blue → cyan → yellow → red) and the latest frame as a live spectrum
  line, with peak-frequency/dB readout and a pause toggle. `run_audio_input`
  (under the `audio` feature) captures the default input device via the built-in WASAPI backend,
  downmixing F32/I16/U16 streams to mono. The only per-frame allocation is the
  `Vec<f32>` of dB values sent over the channel.

**Not yet implemented** (from `todo.md`): the actual RTL-SDR USB driver/binding
and a fully wired end-to-end SDR app (the `io` `rtl-sdr` feature is a stubbed
backend). Continuous frame-drop-free streaming verification and the desktop
waterfall UI are now implemented.

---

## 5. Module-by-module overview

### 5.1 `tpt-dsp-core` (`src/lib.rs`)

| Module | Key public types / functions |
| --- | --- |
| `complex` | `Complex32`/`Complex64`, aliases `C32`/`C64`; `magnitude`, `magnitude_squared`, `phase`, `exp_i`, `rotate` |
| `fft` | `fft`, `ifft`, `fft_inplace`, `ifft_inplace`, `twiddles`, `is_power_of_two`, `next_power_of_two` — hand-rolled radix-2, power-of-two only, `no_std` |
| `plan` (`std`) | `FftPlan` — RustFFT-backed, arbitrary length, buffers allocated once |
| `dct` | `dct_ii`, `dct_iii`, `dct_iv` — direct O(N²), allocation-free |
| `hilbert` | `hilbert` (free) + `HilbertTransformer` (`alloc`) — analytic-signal transform |
| `windows` | `windowed`, `WindowType` (`Hann`/`Hamming`/`Blackman`) — f32 only |
| `filters` | `Biquad`, `BiquadCoeffs`, `BiquadType`, `process_biquad`; `alloc`: `Fir`, `FirDesign`, `IirFilter`/`IirCoeffs`/`IirStage` |
| `convolution` | `convolve` (direct); `alloc`: `ConvolvePlan`, `FftConvolver` (overlap-add) |
| `ring` | `RingBuffer`, traits `RingRead`/`RingWrite` |
| `spsc` (`std`) | `SpscQueue`, `Producer`, `Consumer` (crossbeam) |
| `demod` | `FmDemodulator`, `phase_delta`, `phase_to_audio` |
| `resample` (`alloc`) | `FIRDecimator` — integer decimation with windowed-sinc anti-alias |

Feature flags: `std` (default) = `alloc` + `rustfft` + `crossbeam-channel`;
`alloc` enables owning structs.

### 5.2 `tpt-dsp-audio` (`src/lib.rs`)

| Module | Key types |
| --- | --- |
| `oscillator` | `Oscillator`, `Waveform` (Sine/Sawtooth/Square/Triangle) — phase accumulator, **naive (aliasing) waveforms, no PolyBLEP** |
| `wavetable` | `Wavetable` — single-period lookup, **no band-limited mipmaps/morphing** |
| `fm` | `FmSynth` — **2-operator only, no feedback/algorithms** |
| `subtractive` | `SubtractiveVoice` — osc → LP biquad → ADSR gain |
| `waveshaper` | `Waveshaper`, `Curve` — distortion, **no oversampling** |
| `delay` | `Delay` — feedback delay, **integer delay only** |
| `reverb` | `ConvolutionReverb`, `generate_decay_ir` — FFT convolver wrapper |
| `eq` | `Eq` — cascaded biquad peaking/shelf |
| `envelope` | `Adsr`, `EnvelopeState` |
| `graph` | `AudioGraph`, traits `AudioNode`/`Source`/`Sink`, `ClosureNode`/`ClosureSource`/`ClosureSink`, `Passthrough` |
| `engine` | `RealtimeEngine`, `BLOCK_128`, `BLOCK_256` |

### 5.3 `tpt-dsp-analysis` (`src/lib.rs`)

| Module | Key types / functions |
| --- | --- |
| `spectrum` | `SpectrumAnalyzer`, `RealtimeSpectrumAnalyzer`, `SpectrumConfig`, `Averaging`, `SpectralPeak`,`DEFAULT_DB_FLOOR`; `find_peaks`, `peak_bin`, `dominant_frequency`, `linear_to_db`, `db_to_linear`, `parabolic_interpolate` |
| `features` | `rms`, `zero_crossing_rate`, `spectral_centroid`, `spectral_centroid_normalized` |
| `timeseries` | `MovingAverage`, `RunningMean`, `Ema`, `OutlierDetector` (median/MAD, in-place sort) |
| `spectrogram` | `Spectrogram` — ring of magnitude frames for waterfall |
| `async_adapters` (`async`) | `process_channel`, `process_stream` (tokio); runtime-agnostic `process_stream_into_sink`/`process_stream_in_place`; `tokio` and `async_std` submodules gated by features |

`async` (default) pulls in `tokio` + `futures`. The tokio runtime channel adapter
and an async-std adapter submodule exist; verify `Cargo.toml` feature wiring
(`async-tokio`, `async-std`) before relying on them.

### 5.4 `tpt-dsp-control` (`src/lib.rs`)

| Module | Key types |
| --- | --- |
| `pid` | `Pid`, `AntiWindup` (clamping / back-calculation) — derivative-on-error, fixed `dt`, `f32` |
| `input_shaping` | `InputShaper` — **ZVD only** (no ZV/ZVDD/EI/multi-mode) |
| `kinematics` | `TrapezoidalProfile` (complete), `JerkLimiter` (first-order velocity loop, **not a true S-curve**) |

### 5.5 `tpt-dsp-io` (`src/lib.rs`)

| Module | Key types | Feature |
| --- | --- | --- |
| `iq` | `parse_iq`, `IqFormat`, `IqStream` | always |
| `audio` | `run_output`, `run_input`, `run_output_on_device`, `run_input_on_device`, `list_output_devices`, `list_input_devices` | `audio` (built-in native backends: WASAPI on Windows, raw ALSA UAPI on Linux) — **mono f32 output, capture input, device selection; no duplex** |
| `serial` | `SerialReader` | `serial` — **open/read only, no write/enumeration/timeout/framing** |
| `tcp` | `serve_iq` | `tcp` — **single-connection** async IQ server |

---

## 6. `no_std` / embedded story & CI

- **`tpt-dsp-core`** is the only `no_std` crate. `cargo build -p tpt-dsp-core
  --no-default-features` succeeds; it is also checked on
  `thumbv7em-none-eabihf --no-default-features`. `alloc`-gated types
  (`Fir`, `IirFilter`, `HilbertTransformer`, `FftConvolver`, `FIRDecimator`) and
  `std`-gated types (`FftPlan`, `SpscQueue`) are excluded in `no_std`.
- **`audio` / `analysis` / `control` / `io`** are `std`-only today (they use
  `std::vec`, platform crates, or async runtimes). No `embedded-hal` port yet.
- **CI targets** (`Runs on` GitHub Actions): native `cargo build/test/clippy`,
  `wasm32-unknown-unknown` (`cargo check` of core+audio),
  `thumbv7em-none-eabihf` (`cargo check` of core, `no_std`), `cargo deny`, and
  `cargo doc`. Lint gate: `clippy --all-targets --all-features -D warnings`.

---

## 7. Known limitations (concise, accurate as of 2026-08-07)

**Build/test health** — per `todo.md`, deadlock/clippy/`no_std`/io-test issues
are resolved; full workspace suite passes. `cargo deny` passes on the modern
schema.

**Zero-alloc contract** — see §3.2. Net: allocation-free in steady state for
core math/filters/reverb/EQ/OutlierDetector, but per-frame allocation in the
async adapters and grow-only scratch in `IirFilter`/`Eq`.

**Functionality gaps:**
- Audio graph: linear mono chain only (no DAG/fan-in/fan-out/multichannel).
- Oscillators: naive/aliasing waveforms (no PolyBLEP); wavetable/FM are minimal
  (no mipmaps, 2-operator only).
- Delay: integer-only; Waveshaper: no oversampling; ConvolutionReverb: long IRs
  truncated (no partitioned convolution).
- Windows: f32-only, Hann/Hamming/Blackman only (no Kaiser/Tukey/flat-top).
- DCT: direct O(N²) only (no fast DCT).
- Hand-rolled `fft`: power-of-two only (use `FftPlan`/RustFFT for arbitrary
  lengths).
- I/O: built-in WASAPI output mono (+ capture); serial read-only; `tcp` single-connection; no
  cross-read buffering for mid-sample splits beyond `IqStream`.
- `tpt-dsp-control` depends on `tpt-dsp-core` but does not currently use it.
- `portable-simd` FFT/complex optimization: **not started**.

**Roadmap status (marked "(planned)" above):**
- MVP 1 (WASM guitar pedal UI + wasm-bindgen + GitHub Pages) — **done**.
- MVP 2 (RTL-SDR USB ingestion, desktop waterfall UI, frame-drop-free streaming
  verification, fully wired SDR app) — **waterfall UI and frame-drop-free
  streaming verification are done**; RTL-SDR USB driver and a fully wired SDR app
  remain.
- `embedded-hal` Cortex-M validation; criterion benchmark reports vs JUCE /
  libsamplerate; crates.io v1.0.0 publish.

---

## 8. Extending the framework

- New **math/transform** belongs in `tpt-dsp-core`, `no_std`-compatible and
  buffer-driven. Add a `pub use` in `core/src/lib.rs` and, if it owns buffers,
  gate behind `alloc`.
- New **audio effect** is a struct with `new` (pre-allocates scratch) and
  `process(&mut [f32])` or `tick`. Wire it into `AudioGraph` via a
  `ClosureNode`.
- New **analysis feature** goes in `tpt-dsp-analysis`; keep the per-sample/per-
  frame path allocation-free.
- Never introduce heap allocation, locks, or system calls on a hot path; reuse a
  pre-allocated scratch buffer instead. Keep `core` `#![forbid(unsafe_code)]`.

---

## 9. CLI, plugin & Python bindings (2026-08-20)

Three crates wrap the framework for use outside Rust:

- **`tpt-dsp-cli`** (workspace member) — a command-line WAV/IQ pipeline.
  `filter` runs biquad / EQ / waveshaper / delay / convolution-reverb chains
  (specs parse the same `tpt-dsp-audio` effects), `demod` FM-demodulates raw IQ
  to WAV, `spectrum` averages a one-sided magnitude spectrum and reports
  dominant frequency, peak dB, RMS, zero-crossing rate and spectral centroid,
  and `info` prints file metadata. WAV I/O uses `tpt-dsp-io`'s built-in RIFF/WAVE module; IQ parsing uses
  `tpt-dsp-io`.
- **`tpt-dsp-nihplug`** (excluded) — a CLAP/VST3 plugin wrapping the
  `tpt-dsp-audio` pedalboard (Waveshaper → Delay → ConvolutionReverb → 3-band
  EQ) as a host-parameterised chain. Built on `nice-plug` (the actively
  maintained successor to `nih-plug`, which is no longer on crates.io) and
  exported with `nice_export_clap!` / `nice_export_vst3!`.
- **`tpt-dsp-py`** (excluded) — pyo3 bindings exposing `rms`,
  `zero_crossing_rate`, `spectral_centroid`, `spectrum`, `fm_demod` and
  `analyze` as the `tpt_dsp` extension module.

`nihplug` and `py` are **excluded** from the main workspace (root `Cargo.toml`
`exclude`): the plugin pulls VST3/CLAP SDK bindings whose licences fall outside
the main `deny.toml` allow-list, and the Python module links Python
(`extension-module`) so it cannot be built or tested by the `cargo
build/test --workspace` gate. Both are verified as standalone workspaces
(`cargo build` / `cargo clippy` inside each directory).
