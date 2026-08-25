# tpt-dsp Quickstart

A single runnable path from a fresh clone to seeing and hearing the framework
work. All commands run from the repository root on stable Rust 1.85+.

```sh
git clone <repository-url> tpt-dsp
cd tpt-dsp
```

## 1. Build everything

```sh
just build          # or: cargo build --workspace --all-features
```

## 2. Run the examples

Each core crate ships one runnable example. The fastest way to *see* output:

| Command | What you get |
| --- | --- |
| `just example tpt-dsp-audio synth_eq` | FM synth rendered through an EQ; writes `synth_eq.wav` |
| `cargo run -p tpt-dsp-io --example sdr_pipeline` | End-to-end IQ → decimate → FM demod pipeline (no hardware needed) |
| `just example tpt-dsp-core biquad_lowpass` | Biquad low-pass + FFT spectrum printout in the terminal |
| `just example tpt-dsp-analysis analyze_signal` | RMS / spectral centroid / outlier stats over a synthetic signal |
| `just example tpt-dsp-control pid_speed_hold` | PID speed controller with anti-windup stepping to a setpoint |
| `cargo run -p tpt-dsp-viz --release` | Desktop waterfall + live spectrum window (synthetic signal) |

Prefer not to use `just`? `just example CRATE NAME` is just
`cargo run -p CRATE --example NAME`; see the [justfile](../justfile) for the
full recipe list (`just ci` runs the complete gate CI runs).

## 3. Use it as a library

Add the crates you need (all depend on `tpt-dsp-core`, never on each other):

```toml
[dependencies]
tpt-dsp-core = "0.1"
tpt-dsp-audio = "0.1"
```

Then process blocks through an allocation-free filter:

```rust
use tpt_dsp_core::{Biquad, BiquadType};

let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let input = [0.0f32; 128];
let mut out = [0.0f32; 128];
lp.process(&input, &mut out); // no allocation, no locks, real-time safe
```

## 4. Try the web pedalboard locally

Requires [`wasm-pack`](https://rustwasm.github.io/wasm-pack/):

```sh
wasm-pack build tpt-dsp-wasm --target web --out-dir ../www/pkg
python -m http.server 8080 --directory www   # open http://localhost:8080
```

## Next steps

- [Architecture](../ARCHITECTURE.md) for dependency direction and the
  zero-allocation contract details.
- [Contributing](../CONTRIBUTING.md) before opening a pull request.
- Start a new effect/pipeline crate from our skeleton:
  `cargo generate --path templates/dsp-effect-crate -n my-effect`.
