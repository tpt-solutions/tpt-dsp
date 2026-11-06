# tpt-dsp

> One dependency for the [tpt-dsp](https://github.com/TPT-Solutions/tpt-dsp) framework.

The facade re-exports the framework's crates behind feature flags and gives you a
`prelude` with the types most programs use. Depend on it instead of the individual
crates when you want several of them.

```toml
[dependencies]
tpt-dsp = { git = "https://github.com/TPT-Solutions/tpt-dsp" }                 # default features
# tpt-dsp = { git = "...", default-features = false, features = ["io"] }      # pick your own
```

The crate is not on crates.io yet (see [`PUBLISHING.md`](https://github.com/TPT-Solutions/tpt-dsp/blob/master/PUBLISHING.md)),
so depend on it by git or path.

## Features

| Feature    | Adds                                  | Default |
| ---------- | ------------------------------------- | ------- |
| `audio`    | synthesis, effects, dynamics, graphs  | yes     |
| `analysis` | spectra, features, STFT, loudness     | yes     |
| `control`  | PID, input shaping, kinematics        | yes     |
| `io`       | WAV, IQ streams, native audio I/O     | no      |

`tpt-dsp-core` is always included. It is `no_std`, so `default-features = false`
keeps the core usable on bare metal.

## Re-exports

- `tpt_dsp::core` is always available, and it re-exports `tpt-dsp-core`.
- `tpt_dsp::audio`, `tpt_dsp::analysis`, `tpt_dsp::control` and `tpt_dsp::io` are
  present when their features are enabled.
- `tpt_dsp::prelude::*` brings in the common types from each enabled crate.

## Example

```rust
use tpt_dsp::prelude::*;

let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let mut block = [0.5f32; 64];
lp.process(&block.clone(), &mut block);
```

The `walkthrough` module (`tpt_dsp::walkthrough`) is a compiled end-to-end example:
it reads a WAV file, filters it, analyses it and writes the result. Read its source
for a worked pipeline.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
