# tpt-dsp-core

> The mathematical engine of the [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-core` is a collection of pure, real-time-safe digital signal processing
primitives written in Rust. It is the leaf crate of the workspace: every other
`tpt-dsp-*` crate builds on it. It is **`no_std` compatible** and contains **no
`unsafe` code** (`#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`).

## Why this crate exists

Other crates in the framework (`audio`, `analysis`, `io`, `control`, `viz`)
provide domain features, but they all need the same low-level math: complex
arithmetic, transforms, windows, filters and buffers. `tpt-dsp-core` is that
shared foundation, kept dependency-free and embeddable on bare metal.

## Real-time safety

Every hot-path processing entry point operates on pre-allocated buffers supplied
by the caller. No heap allocation, lock, or system call happens inside the
processing functions. Build with `--no-default-features` to drop `std` entirely
and target bare-metal platforms such as `thumbv7em-none-eabihf` (ARM Cortex-M).

When the `alloc` feature is enabled, a handful of *owning* convenience structs
are available (`Fir`, `IirFilter`, `HilbertTransformer`, `FftConvolver`,
`FIRDecimator`, `ConvolvePlan`). These allocate **once at construction**; all
subsequent processing remains allocation-free.

## Features

| Feature   | Default | Description                                                                                   |
| --------- | ------- | --------------------------------------------------------------------------------------------- |
| `std`     | ✓       | RustFFT-backed [`FftPlan`], crossbeam [`SpscQueue`], and the `alloc` feature.                  |
| `alloc`   | (via std) | Heap-backed owning structs with allocation-free processing.                                  |
| `simd`    | ✗       | Swaps the vectorised module to `core::simd` (portable SIMD). **Nightly only.** Falls back to an identical scalar API on stable, so the public surface never changes. |

## What's inside

- **Complex math** — `Complex32`/`Complex64` (re-exports of `num_complex`), plus
  `exp_i`, `magnitude`, `magnitude_squared`, `phase`, `rotate`, and SIMD helpers
  `complex_add_simd`, `complex_mul_simd`, `magnitude_simd`.
- **Transforms** — `fft`, `ifft`, `fft_inplace`, `fft_inplace_f32`, `ifft_inplace`,
  `twiddles`, `next_power_of_two`/`is_power_of_two` helpers, and the FFTW-style
  [`FftPlan`] for reusable plans.
- **DCT** — `dct_ii`, `dct_iii`, `dct_iv`.
- **Hilbert transform** — `hilbert` (free function) and the owning
  [`HilbertTransformer`].
- **Demodulation** — `FmDemodulator`, `phase_delta`, `phase_to_audio`.
- **Windows** — `windowed` with `WindowType` (Hann, Hamming, Blackman, …).
- **Filters** — the single-stage allocation-free [`Biquad`] (`design`/`process`),
  plus owning [`Fir`], [`IirCoeffs`]/[`IirStage`]/[`IirFilter`] when `alloc` is on.
- **Convolution** — `convolve`, [`FftConvolver`], [`ConvolvePlan`].
- **Resampling** — [`FIRDecimator`] (feature `alloc`).
- **Buffers** — lock-free [`RingBuffer`] (`RingRead`/`RingWrite`) and the
  crossbeam-backed [`SpscQueue`] (feature `std`).

## Examples

### Design and run a biquad low-pass filter (allocation-free)

```rust
use tpt_dsp_core::{Biquad, BiquadType};

let mut lp = Biquad::<f32>::design(BiquadType::LowPass, 48_000.0, 1_000.0, 0.707, 0.0);
let mut out = [0.0f32; 128];
lp.process(&input_block, &mut out); // no allocation
```

### FFT of a real-time block

```rust
use tpt_dsp_core::{fft, next_power_of_two};

let n = next_power_of_two(input.len());
let mut spectrum = vec![num_complex::Complex::new(0.0f32, 0.0); n];
fft(&input, &mut spectrum);
// for repeated transforms in a hot loop, build a reusable `FftPlan` instead.
```

### `no_std` / embedded

```sh
cargo build -p tpt-dsp-core --no-default-features
cargo check -p tpt-dsp-core --target thumbv7em-none-eabihf --no-default-features
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
