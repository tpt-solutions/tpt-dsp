# tpt-dsp

A pure-Rust, real-time-safe digital signal processing framework.

`tpt-dsp` provides the building blocks for audio, RF/SDR, control and
telemetry DSP: FFT/DCT/Hilbert transforms, filters (biquad/FIR/IIR),
convolution, windowing, oscillators and synthesis, spectrum analysis,
time-series statistics, and hardware I/O (audio, serial, streaming IQ).

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.

---

## Features

- **Real-time safe.** Hot-path processing operates on caller-provided,
  pre-allocated buffers. Where a struct owns buffers, those buffers are
  allocated once at construction and every `process`/`tick` call is
  allocation-free.
- **`no_std` compatible.** `tpt-dsp-core` builds for bare metal
  (`--no-default-features`) and is verified on `thumbv7em-none-eabihf`
  (ARM Cortex-M).
- **Composable.** Small, single-purpose primitives that combine into
  audio graphs, analysis pipelines and control loops.
- **License-clean.** Strictly MIT / Apache-2.0; `cargo-deny` blocks any
  copyleft (GPL/LGPL/AGPL) dependency.

## Workspace layout

| Crate                | Purpose                                                              |
| -------------------- | -------------------------------------------------------------------- |
| `tpt-dsp-core`       | Complex math, FFT/DCT/Hilbert, windows, biquad/FIR/IIR, convolution, ring buffers, SPSC queues. `no_std`. |
| `tpt-dsp-audio`      | Oscillators, wavetable/FM/subtractive synthesis, waveshaping, delay, reverb, EQ, audio graph, real-time engine. |
| `tpt-dsp-analysis`   | Spectrum analysis, peak detection, spectrograms, moving averages, EMA, outlier detection, RMS, spectral centroid, async (tokio/futures) adapters. |
| `tpt-dsp-control`    | PID with anti-windup, input shaping (ZVD), trapezoidal & jerk-limited trajectory planning. |
| `tpt-dsp-io`         | IQ byte-stream parsing, cpal audio output, serial reader, async TCP IQ server. |

## Architecture

```text
                 ┌──────────────┐
   raw samples → │ tpt-dsp-io  │ (audio / serial / TCP IQ)
                 └──────┬───────┘
                        │ Complex32 / f32
        ┌───────────────┼───────────────────────┐
        ▼               ▼                       ▼
 ┌─────────────┐ ┌──────────────┐      ┌────────────────┐
 │tpt-dsp-core │ │tpt-dsp-audio │      │tpt-dsp-analysis│
 │ FFT/filters │ │ synth/effects│      │ spectrum/stats │
 │ convolution │ └──────────────┘      └───────┬────────┘
 └──────┬──────┘                              │
        │                                     │
        └──────────────► tpt-dsp-control ◄────┘
                    (PID / shaping / planning)
```

The dependency graph is acyclic: `core` is the leaf, `audio`/`analysis`/
`control`/`io` build on top of it.

## Getting started

```toml
[dependencies]
tpt-dsp-core = "0.1"
tpt-dsp-audio = "0.1"
```

### Design a biquad low-pass filter

```rust
use tpt_dsp_core::{Biquad, BiquadType};

let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let mut out = [0.0f32; 128];
lp.process(&input_block, &mut out); // allocation-free
```

### FFT of a real-time block

```rust
use tpt_dsp_core::{fft, ifft, next_power_of_two};

let n = next_power_of_two(input.len());
let mut spectrum = vec![num_complex::Complex::new(0.0f32, 0.0); n];
fft(&input, &mut spectrum); // see tpt-dsp-core::plan::FftPlan for FFTW-style plans
```

### Build an audio graph

```rust
use tpt_dsp_audio::{
    graph::{AudioGraph, ClosureNode, ClosureSink, ClosureSource},
    oscillator::{Oscillator, Waveform},
};

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

## Web demo

The `tpt-dsp-wasm` crate powers a browser guitar-pedalboard
(Waveshaper → Delay → ConvolutionReverb → EQ) running inside an
`AudioWorklet`. Once the repo's GitHub Pages "GitHub Actions" source is
enabled, the live demo is served from the [`www/`](www) directory — see
[`.github/workflows/pages.yml`](.github/workflows/pages.yml). Locally:

```sh
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
samples (and on RF/control data), and `tpt-dsp-io` uses `cpal` under the hood
for its audio source. `dasp` is a broad, trait-based DSP toolkit with a
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
# cross-compile to Cortex-M
cargo check -p tpt-dsp-core --target thumbv7em-none-eabihf --no-default-features
```

The `alloc` feature (enabled by `std`) adds owning convenience structs
(`Fir`, `IirFilter`, `HilbertTransformer`, `FftConvolver`, `ConvolvePlan`);
the single-stage `Biquad` and the free `process_*` functions are always
available.

## Testing & CI

```sh
cargo build --workspace --all-features
cargo test  --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt   --all -- --check
cargo deny  check
```

CI also builds for `wasm32-unknown-unknown` and `thumbv7em-none-eabihf`.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
