# Real-time safety guide

This guide says what the framework guarantees on the audio thread, what it does
not, and how to check your own code. The authoritative list of exceptions is
in `ARCHITECTURE.md` §3.2.

## The contract

Functions named `process`, `tick`, `render`, `convolve` and `fft` must not
allocate, lock, or make a syscall. Each exception is documented where it lives.

Two idioms are used throughout:

1. **A free function over caller-owned slices**, such as `windowed(...)` or
   `process_biquad(...)`.
2. **A struct that allocates everything in `new()`** and reuses it, such as
   `ConvolutionReverb`, `Eq`, `Oversampler` and `Freeverb`.

## What allocates, and when

| Item | Allocates | When |
|------|-----------|------|
| Constructors (`new`, `design`, `try_*`) | yes | Off the audio thread. Build everything before starting the stream. |
| `Eq`, `IirFilter` | no, after construction | Scratch is reserved up front (`with_max_block`). A block longer than that grows the buffer once. |
| `ConvolutionReverb` | no | All buffers are sized in `new()`. |
| `Freeverb`, `Chorus`, `Flanger`, `Phaser` | no | Delay lines are sized in `new()`. |
| `Compressor`, `Limiter`, `Gate`, `Lfo`, `Tremolo` | no | State is fixed-size. |
| `Oversampler` | no per sample | Filters are built in `new()`. |
| `RationalResampler::process` | only into the `out` Vec you pass | Use a Vec with enough capacity reserved. |
| `Mfcc::compute`, `LoudnessMeter::push` | no | Buffers are owned. |
| `stft`, `istft`, `time_stretch`, `yin_pitch` | yes | Offline or analysis-frame use only. Not for the callback. |
| `CicDecimator` | no | State is in `Vec`s sized in `new()`. |
| Spectrum averaging, `Ema`, `OutlierDetector::push` | no | The outlier detector sorts a reused scratch buffer. |

## Parameters from the UI thread

Parameters change on a different thread from the one that processes audio.
Use `Smoothed<F>` to turn a jumpy value into a glide, and `set_*` methods that
do not reallocate. The `set_sample_rate` methods on `Adsr`, `Eq` and
`SubtractiveVoice` may rebuild filter coefficients, so call them when the
stream is stopped or from a control thread.

## Checking your own chain

Install the counting allocator from `tpt-dsp-testkit` as the global allocator
in a test binary, then assert:

```rust,ignore
use tpt_dsp_testkit::{assert_no_alloc, CountingAllocator};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator;

#[test]
fn chain_does_not_allocate() {
    let mut chain = build_chain();          // allocate here, outside the check
    let mut block = [0.0f32; 128];
    assert_no_alloc(|| chain.process_block(&mut block));
}
```

Counting is per thread, so parallel tests do not interfere.

## Non-finite input

Every processor in the audio path sanitises NaN and infinite inputs, and
returns a finite output. This is a safety net, not a substitute for upstream
validation. A NaN that reaches a filter state is cleared on the next block.

## What is not guaranteed

- Latency. `ConvolutionReverb`, `Oversampler`, `RationalResampler` and the
  limiter (no lookahead) each have a documented, nonzero latency or transient
  behaviour.
- Timing of the stream. The native backends are validated on Windows only;
  Linux and macOS backends are compile-checked (see `ARCHITECTURE.md`).
