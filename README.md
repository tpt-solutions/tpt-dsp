# tpt-dsp

[![crates.io](https://img.shields.io/crates/v/tpt-dsp-core.svg)](https://crates.io/crates/tpt-dsp-core)
[![docs.rs](https://img.shields.io/docsrs/tpt-dsp-core)](https://docs.rs/tpt-dsp-core)
[![CI](https://github.com/tpt-solutions/tpt-dsp/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-dsp/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](Cargo.toml)

A pure-Rust, real-time-safe digital signal processing framework.

**Web pedalboard demo:** [https://tpt-solutions.github.io/tpt-dsp/](https://tpt-solutions.github.io/tpt-dsp/)
(distortion → delay → reverb → EQ, running entirely in the browser via WebAssembly).

`tpt-dsp` provides the building blocks for audio, RF/SDR, control and
telemetry DSP: FFT/DCT/Hilbert transforms, filters (biquad/FIR/IIR),
convolution, windowing, oscillators and synthesis, spectrum analysis,
time-series statistics, and hardware I/O (audio, serial, streaming IQ).
It also ships a command-line pipeline, a live spectrum viewer, and a
WebAssembly build for the browser.

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.

---

## Features

- **Real-time safe.** Hot-path processing operates on caller-provided,
  pre-allocated buffers. Where a struct owns buffers, those buffers are
  allocated once at construction and `process`/`tick` calls reuse them.
  Known exceptions are catalogued in [`ARCHITECTURE.md`](ARCHITECTURE.md) §3.2.
- **`no_std` compatible.** `tpt-dsp-core` builds for bare metal
  (`--no-default-features`) and is verified on `thumbv7em-none-eabihf`
  (ARM Cortex-M).
- **Composable.** Small, single-purpose primitives that combine into
  audio graphs, analysis pipelines and control loops.
- **Self-contained audio I/O.** No external audio crates; the `audio`
  feature of `tpt-dsp-io` talks to WASAPI, ALSA and CoreAudio directly.
- **License-clean.** Strictly MIT / Apache-2.0; `cargo-deny` blocks any
  copyleft (GPL/LGPL/AGPL) dependency.

## Workspace layout

| Crate                | Purpose                                                              |
| -------------------- | -------------------------------------------------------------------- |
| `tpt-dsp-core`       | Complex math, FFT/DCT/Hilbert/MDCT, windows, biquad and IIR design (Butterworth, Chebyshev I/II, elliptic), equiripple and least-squares FIR design, convolution, resampling, receiver blocks (PLL, AGC, squelch, AM/SSB demodulators), RF helpers, pitch estimation, LPC/LSP, vector quantisation, ring buffers, SPSC queues. `no_std`, `#![forbid(unsafe_code)]`. |
| `tpt-dsp-audio`      | Oscillators, wavetable/FM/subtractive/PolyBLEP synthesis, noise and dither, waveshaping, oversampling, delay, Freeverb and stereo Freeverb, EQ, dynamics (compressor, limiter, gate), modulation (LFO, tremolo, chorus, flanger, phaser), audio graph, real-time engine. |
| `tpt-dsp-analysis`   | Spectrum analysis, STFT, phase-vocoder time-stretch and pitch shifting, YIN pitch, onset detection, spectrograms, BS.1770 loudness, MFCC, spectral descriptors, moving averages, EMA, outlier detection, RMS, async (tokio/futures) adapters. |
| `tpt-dsp-control`    | PID with anti-windup, a builder and derivative filter, input shaping (ZVD), trapezoidal & jerk-limited trajectory planning. |
| `tpt-dsp-io`         | IQ byte-stream parsing, built-in audio I/O — shared-mode WASAPI on Windows, raw ALSA UAPI on Linux, CoreAudio AudioUnits on macOS — plus RIFF/WAVE read/write, serial reader, and a TCP IQ source that can tune an `rtl_tcp` server (frequency, sample rate, gain, PPM). |
| `tpt-dsp-wasm`       | WebAssembly pedalboard (Waveshaper → Delay → ConvolutionReverb → EQ) for a Web Audio `AudioWorklet`. |
| `tpt-dsp-viz`        | Desktop viewer: scrolling waterfall, live dB spectrum line and peak readout (in-tree renderer over `minifb`). Synthetic or live audio input. |
| `tpt-dsp-cli`        | Command-line WAV/IQ pipeline: `gen`, `resample`, `design`, `response`, `filter`, `demod`, `spectrum`, `info`, `pipeline` (JSON or TOML) and `completions` subcommands. |
| `tpt-dsp-testkit`    | Test helpers that prove a path is allocation-free: a counting global allocator, `allocations_in` and `assert_no_alloc`. |
| `tpt-dsp`            | Facade crate: one dependency, feature-gated re-exports of the crates above, and a `prelude`. |

`tpt-dsp-nihplug` (a CLAP/VST3 wrapper) and `tpt-dsp-py` (Python bindings)
live in the repository but are excluded from the default workspace build;
see their own READMEs.

## Architecture

```text
                 ┌──────────────┐
   raw samples → │ tpt-dsp-io   │ (audio / serial / TCP IQ)
                 └──────┬───────┘
                        │ Complex32 / f32
        ┌───────────────┼───────────────────────┐
        ▼               ▼                       ▼
 ┌─────────────┐ ┌──────────────┐      ┌────────────────┐
 │tpt-dsp-core │ │tpt-dsp-audio │      │tpt-dsp-analysis│
 │ FFT/filters │ │ synth/effects│      │ spectrum/stats │
 │ convolution │ └──────────────┘      └────────────────┘
 └─────────────┘

 tpt-dsp-control  (standalone: PID / shaping / planning, no workspace dependencies)
```

`tpt-dsp-core` is the leaf crate that everything else builds on. The
`audio`, `analysis` and `io` crates depend on `core` only, `control` has no
workspace dependencies, and none of them depend on each other. The
applications sit on top: `tpt-dsp-wasm` (core + audio), `tpt-dsp-viz`
(analysis + io) and `tpt-dsp-cli` (core, audio, analysis, io). The dependency
graph is acyclic.

## Getting started

The publishable crates are on [crates.io](https://crates.io) at 0.2.0
(see [`PUBLISHING.md`](PUBLISHING.md) for the list and the release process):

```toml
[dependencies]
tpt-dsp-core = "0.2"
tpt-dsp-audio = "0.2"
```

### Design a biquad low-pass filter

```rust
use tpt_dsp_core::{Biquad, BiquadType};

let input_block = [0.0f32; 128];
let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let mut out = [0.0f32; 128];
lp.process(&input_block, &mut out); // allocation-free
```

### FFT of a real-time block

`fft` is out-of-place and takes a caller-provided scratch buffer, so the
transform itself allocates nothing. Input length must be a power of two.

```rust
use tpt_dsp_core::{fft, next_power_of_two, Complex32};

let input = vec![0.5f32; 1000];
let n = next_power_of_two(input.len());

let mut buf = vec![Complex32::new(0.0, 0.0); n]; // zero-padded input
for (b, &x) in buf.iter_mut().zip(input.iter()) {
    *b = Complex32::new(x, 0.0);
}
let mut spectrum = vec![Complex32::new(0.0, 0.0); n];
let mut scratch = vec![Complex32::new(0.0, 0.0); n];
fft(&buf, &mut spectrum, &mut scratch);
```

### Build an audio graph

```rust
use tpt_dsp_audio::{AudioGraph, ClosureNode, ClosureSink, ClosureSource, Oscillator, Waveform};

// A 220 Hz sine source, a gain node, and a sink that forwards to a DAC.
let mut osc = Oscillator::with_waveform(48_000.0, 220.0, Waveform::Sine);
let mut graph = AudioGraph::new(
    128,
    Box::new(ClosureSource(move |out: &mut [f32]| {
        for s in out.iter_mut() {
            *s = osc.tick();
        }
    })),
    vec![Box::new(ClosureNode(|input: &[f32], out: &mut [f32]| {
        for (o, x) in out.iter_mut().zip(input.iter()) {
            *o = x * 0.5;
        }
    }))],
    Box::new(ClosureSink(|block: &[f32]| {
        // forward `block` to the audio output device
    })),
);
graph.run(100); // render 100 blocks
```

### Command line

```sh
cargo run -p tpt-dsp-cli -- spectrum --input in.wav --fft-size 2048 --top 10
```

See [`tpt-dsp-cli/README.md`](tpt-dsp-cli/README.md) for every subcommand.

## Examples

Every example is a runnable `cargo run` target. The first four run without
hardware or extra features.

| Example | Shows | Command |
| --- | --- | --- |
| `biquad_lowpass` | Design an RBJ low-pass, filter a mixed signal, find the surviving tone with an FFT | `cargo run -p tpt-dsp-core --example biquad_lowpass` |
| `analyze_signal` | Real-time spectrum analysis of a noisy sine, with EMA smoothing and outlier detection | `cargo run -p tpt-dsp-analysis --example analyze_signal` |
| `synth_eq` | An FM synthesis voice through a 3-band parametric EQ | `cargo run -p tpt-dsp-audio --example synth_eq` |
| `pid_speed_hold` | A PID holds a simulated DC motor at a setpoint, with output limits and anti-windup | `cargo run -p tpt-dsp-control --example pid_speed_hold` |
| `sdr_pipeline` | IQ → decimate → FM demodulate → 48 kHz audio. Synthetic by default; `--tcp` reads an `rtl_tcp` server | `cargo run -p tpt-dsp-io --example sdr_pipeline -- --out audio.wav` · `--features tcp -- --tcp 127.0.0.1:1234` ([RTL-SDR guide](docs/RTL_SDR.md)) |
| `audio_check` | Live check of the built-in audio backends. Needs a sound device | `cargo run -p tpt-dsp-io --example audio_check --features audio` |
| `custom_waterfall` | The smallest custom spectrogram with `tpt-dsp-viz`. Needs a display | `cargo run -p tpt-dsp-viz --example custom_waterfall` |

The compiled walkthrough in `tpt-dsp/src/walkthrough.rs` (WAV in, filter,
analyse, WAV out) runs as a doctest under `cargo test --all-features`.

Browser examples are in [`www/`](www): the guitar pedalboard, and the
[filter designer](www/designer.html), which plots the responses of every
IIR design in the core and exports their coefficients. There is no embedded
example. CI builds `tpt-dsp-core` for `thumbv7em-none-eabihf`, but a runnable
example needs a board-specific runtime that this repository does not include.

## Web demo

The `tpt-dsp-wasm` crate powers a browser guitar-pedalboard
(Waveshaper → Delay → ConvolutionReverb → EQ) running inside an
`AudioWorklet`. The live demo is served from the [`www/`](www) directory at
[https://tpt-solutions.github.io/tpt-dsp/](https://tpt-solutions.github.io/tpt-dsp/)
(see [`.github/workflows/pages.yml`](.github/workflows/pages.yml)).

To run it locally, install `wasm-pack` once, then build into `www/pkg`
and serve `www/`:

```sh
cargo install wasm-pack
wasm-pack build tpt-dsp-wasm --target web --out-dir ../www/pkg
python -m http.server 8080 --directory www   # open http://localhost:8080
```

## How does tpt-dsp compare?

| | `no_std` | Real-time guarantee | RF/SDR support | Plugin export |
| --- | --- | --- | --- | --- |
| **`tpt-dsp`** | ✅ (`core`) | ✅ allocation-free hot paths, verified by counting-allocator tests | ✅ IQ parsing, FM demod, FIR decimation, TCP/synthetic sources | ⚠️ via `nihplug` wrapper crate (CLAP/VST3) |
| [`cpal`](https://github.com/rustaudio/cpal) | ❌ | transport only | ❌ | ❌ |
| [`dasp`](https://github.com/RustAudio/dasp) | ✅ | sample-level, no scheduler contract | ❌ | ❌ |
| [`fundsp`](https://github.com/SamiPerttu/fundsp) | ❌ | lock-free graph, allocations at build time | ❌ | ❌ |
| [JUCE](https://juce.com/) (C++) | ❌ | ✅ | partial (via add-ons) | ✅ (AU/VST3/LV2/AAX) |

Notes on the table: `cpal` is audio *transport* only — it moves samples
to/from a device; `tpt-dsp` is the *processing* layer that runs on those
samples (and on RF/control data). `tpt-dsp-io` implements its own audio
transport in-tree (shared-mode WASAPI, raw ALSA UAPI and CoreAudio
AudioUnits) with no external audio-crate dependency, so the `audio`
feature adds zero third-party code. `dasp` is a broad, trait-based DSP toolkit with a
friendly sample/Signal API; `tpt-dsp` is narrower but emphasises a hard
real-time contract (pre-allocated, allocation-free hot paths) and
`no_std`/embedded support. `fundsp` offers a composable, lazy audio-graph DSL
with deep node support; `tpt-dsp` is a lower-level, allocation-averse
primitives library that also spans RF/SDR (FM demod, IQ parsing, decimation)
and control (PID, input shaping, kinematics), verified on bare-metal Cortex-M.
JUCE is the mature C++ reference point — plugin-ready out of the box but not
Rust and not `no_std`.

## `no_std` / embedded

`tpt-dsp-core` has no standard-library dependency when built without
default features:

```sh
cargo build -p tpt-dsp-core --no-default-features
# cross-compile to Cortex-M (a real build, so the link step proves no_std)
cargo build -p tpt-dsp-core --target thumbv7em-none-eabihf --no-default-features
```

The `alloc` feature (enabled by `std`) adds owning convenience structs
(`Fir`, `IirFilter`, `HilbertTransformer`, `FftConvolver`, `ConvolvePlan`);
the single-stage `Biquad` and the free `process_*` functions are always
available.

## Testing & CI

```sh
cargo fmt   --all -- --check
cargo build --workspace --all-features
cargo test  --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny  check
cargo deny --all-features check bans licenses advisories sources
cargo doc   --workspace --all-features --no-deps
```

CI also builds `tpt-dsp-core` for `thumbv7em-none-eabihf` and
`tpt-dsp-wasm` for `wasm32-unknown-unknown`. The full job list is in
[`.github/workflows/ci.yml`](.github/workflows/ci.yml).

The workspace MSRV is Rust 1.85, and every crate, including `tpt-dsp-viz`, is checked
against it.

Contributor setup, conventions and the release process are in
[`CONTRIBUTING.md`](CONTRIBUTING.md) and [`PUBLISHING.md`](PUBLISHING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
