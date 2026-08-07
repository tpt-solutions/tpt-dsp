# Benchmarks

Criterion micro-benchmarks for the `tpt-dsp` workspace. They measure the **hot paths**
of our own DSP code and, where a faithful Rust analog exists, compare against an
external implementation on the same workload.

## What is benchmarked

| Crate | Bench file | Coverage |
| --- | --- | --- |
| `tpt-dsp-core` | `dsp_bench.rs` | FFT/DCT/Hilbert, windows, complex math, FM demod, ring & SPSC buffers |
| `tpt-dsp-core` | `filter_bench.rs` | Biquad (block / free-fn / per-sample / design), IIR cascade, FIR, convolution (direct / FFT / overlap-add) |
| `tpt-dsp-core` | `resampling_bench.rs` | `FIRDecimator` vs `rubato` (FFT / sinc / poly), integer-factor + arbitrary-ratio |
| `tpt-dsp-audio` | `audio_bench.rs` | Reverb, EQ (3/10 band, shelves), delay, waveshaper, pedalboard chain, `AudioGraph`, `RealtimeEngine` |
| `tpt-dsp-audio` | `synth_bench.rs` | Oscillator, wavetable, FM, subtractive voice (mono / 8-voice), ADSR |

All numbers below are **medians** with Criterion's throughput (`Melem/s` = millions of
`f32` samples processed per second).

## How to run

```powershell
# Compile-only (CI check):
cargo bench -p tpt-dsp-core  --no-run
cargo bench -p tpt-dsp-audio --no-run

# Full runs (recommended on an idle machine):
cargo bench -p tpt-dsp-core
cargo bench -p tpt-dsp-audio

# A single suite, faster / lower-noise settings:
cargo bench -p tpt-dsp-core  --bench resampling_bench -- --warm-up-time 2 --measurement-time 5 --sample-size 100

# HTML reports in target/criterion:
cargo bench -- --plotting-backend plotters
```

Benches use the `bench` profile, which inherits `release` settings
(`lto = "thin"`, `codegen-units = 1`). No `target-cpu=native` is set, so the default
x86-64 baseline (SSE2) codegen is used. `rustfft` still dispatches to wider SIMD at
runtime, which is why FFT throughput is high.

## Environment

- **Toolchain:** `rustc 1.97.1` / `cargo 1.97.1`, `stable-x86_64-pc-windows-msvc`
- **Hardware:** 12th Gen Intel Core i7-12700 (12C/20T, 2.10 GHz base), Windows 11 Pro
  (10.0.26200), 15.7 GB RAM
- **Dependencies (locked):** `criterion 0.5.1`, `rustfft 6.4.1`, `realfft 3.5.0`,
  `rubato 1.0.1` (bench-only dev-dep), `audioadapter-buffers 2.0.0` (bench-only dev-dep)

### Important: these numbers are noisy

The machine used to gather the figures below was **not quiesced** (the IDE and other
processes were active). Across repeated runs, individual measurements varied by
**~1.5–4×**. Treat every figure as an **order-of-magnitude / relative** reference, not a
hard guarantee. Relative comparisons *within a single run* (e.g. our decimator vs
`rubato`, measured together) are fair; absolute throughputs will shift on a calm machine.
For publishable numbers, run the full command on an isolated host with CPU frequency
governor and thread affinity pinned.

---

## Core transforms — `tpt-dsp-core`

Radix-2 real FFT (in-place, `f32`):

| Size | time (median) | throughput |
| ---: | ---: | ---: |
| 128 | 2.66 µs | 48.0 Melem/s |
| 256 | 11.2 µs | 22.8 Melem/s |
| 1024 | 34.6 µs | 29.6 Melem/s |
| 4096 | 152.5 µs | 26.8 Melem/s |
| 16384 | 611.7 µs | 26.8 Melem/s |

Other transforms (1024-sample `f32` block unless noted):

| Benchmark | time | throughput |
| --- | ---: | ---: |
| `dct/iv_f32/1024` | 50.0 µs | 20.5 Melem/s |
| `dct/ii_f32/1024` | 51.2 µs | 20.0 Melem/s |
| `hilbert/1024` | 20.4 µs | 50.3 Melem/s |
| `window/hann_f32/1024` | 4.17 µs | 245 Melem/s |
| `window/blackman_f32/1024` | 4.07 µs | 251 Melem/s |
| `window/kaiser_f32/1024` | 4.25 µs | 241 Melem/s |
| `complex/mul/1024` | 2.42 µs | 423 Melem/s |
| `complex/magnitude/1024` | 4.78 µs | 215 Melem/s |
| `complex/phase/1024` | 8.06 µs | 127 Melem/s |
| `fm_demod/4096` | 42.2 µs | 97.0 Melem/s |

Buffers:

| Benchmark | time | note |
| --- | ---: | --- |
| `ring/push_pop/1024` | 1.99 µs | ~1.9 ns/sample |
| `ring/push_pop/4096` | 7.69 µs | ~1.9 ns/sample |
| `spsc/push_pop/1024` | 24.6 µs | lock-free queue, higher overhead |

---

## Filters — `tpt-dsp-core`

Biquad lowpass (chunked block processing, `f32`):

| Samples | time | throughput |
| ---: | ---: | ---: |
| 64 | 222 ns | 288 Melem/s |
| 128 | 481 ns | 266 Melem/s |
| 256 | 1.02 µs | 250 Melem/s |
| 1024 | 3.75 µs | 273 Melem/s |

Per-sample and design cost:

| Benchmark | time |
| --- | ---: |
| `biquad/tick_f32/single_sample` | 5.4 ns |
| `biquad/process_biquad_f32/256` (free fn) | 956 ns |
| `biquad/design_f32/*` (all 8 types) | 31.9–51.6 ns |

IIR cascade (256-sample block, `f32`), throughput scales ~linearly with stage count:

| Stages | time | throughput |
| ---: | ---: | ---: |
| 1 | 1.08 µs | 237 Melem/s |
| 2 | 1.95 µs | 131 Melem/s |
| 4 | 4.34 µs | 58.9 Melem/s |
| 8 | 7.73 µs | 33.1 Melem/s |

FIR lowpass (1024-sample block, `f32`) — direct polyphase-style filtering:

| Taps | time | throughput |
| ---: | ---: | ---: |
| 31 | 36.3 µs | 28.2 Melem/s |
| 63 | 78.9 µs | 13.0 Melem/s |
| 127 | 174.6 µs | 5.86 Melem/s |
| 255 | 336.2 µs | 3.05 Melem/s |

FIR coefficient design: `63` taps = 2.14 µs, `255` taps = 8.15 µs (one-off cost).

Convolution (1024-sample signal × impulse, `f32`):

| Method | Impulse | time | throughput |
| --- | ---: | ---: | ---: |
| `direct` | 16 | 12.6 µs | 81.5 Melem/s |
| `direct` | 64 | 17.0 µs | 60.1 Melem/s |
| `direct` | 256 | 42.5 µs | 24.1 Melem/s |
| `fft_plan` (cached plan) | 16 | 144.8 µs | 7.07 Melem/s |
| `fft_plan` (cached plan) | 64 | 136.4 µs | 7.51 Melem/s |
| `fft_plan` (cached plan) | 256 | 137.0 µs | 7.48 Melem/s |
| `overlap_add` | 128 | 15.6 µs | 8.18 Melem/s |
| `overlap_add` | 256 | 33.1 µs | 7.73 Melem/s |
| `overlap_add` | 512 | 68.5 µs | 7.48 Melem/s |
| `overlap_add` | 1024 | 142.9 µs | 7.17 Melem/s |

Notes: for short impulses a naïve `direct` multiply-accumulate beats the FFT path
(FFT fixed overhead dominates). The `fft_plan` variant pays a large fixed cost because
it must zero-pad to a larger transform; it only wins once the impulse is long enough
that the O(N log N) advantage overtakes setup — that crossover is **not** reached at
these sizes, so the FFT convolver is the wrong tool here. `overlap_add` is steady at
~7.5 Melem/s regardless of impulse length (good for long, streaming reverb IRs — see
audio effects below).

---

## Sample-rate conversion — `tpt-dsp-core` vs `rubato`

This is the one place we compare against an external crate, because `rubato` is a mature,
pure-Rust resampler and a fair analog to our `FIRDecimator`.

Workload: **48 kHz → 24 kHz, 2× decimation, 1024 `f32` input → 512 output**, processed in
fixed 1024-sample input chunks (rubato configured with `FixedSync::Input` /
`FixedAsync::Input` so its input block matches ours).

| Implementation | Config | time | throughput |
| --- | --- | ---: | ---: |
| `tpt/fir_decimator` | 63 taps | 17.7 µs | 60.3 Melem/s |
| `tpt/fir_decimator` | 127 taps | 79.3 µs | 13.1 Melem/s |
| `tpt/fir_decimator` | 255 taps | 147.2 µs | 7.04 Melem/s |
| `tpt/fir_filter_then_drop` | 63 taps | 36.8 µs | 28.9 Melem/s |
| `tpt/fir_filter_then_drop` | 127 taps | 170.4 µs | 6.25 Melem/s |
| `tpt/fir_filter_then_drop` | 255 taps | 326.6 µs | 3.26 Melem/s |
| `rubato/fft_sync` | chunk 512 | 6.18 µs | 165.7 Melem/s |
| `rubato/fft_sync` | chunk 1024 | 6.22 µs | 164.6 Melem/s |
| `rubato/sinc_async_cubic` | 64 | 28.6 µs | 35.7 Melem/s |
| `rubato/sinc_async_cubic` | 128 | 59.3 µs | 17.2 Melem/s |
| `rubato/poly_async_cubic` | 128 | 4.27 µs | 239.2 Melem/s |
| `rubato/poly_async_cubic` | 1024 | 4.25 µs | 240.2 Melem/s |

**Observations (relative, same run):**

- `FIRDecimator` is ~2× faster than a naïve "FIR filter then drop every other sample"
  (polyphase: only the taps that land on kept outputs are computed). At 127 taps the
  decimator does 79 µs vs 170 µs for filter-then-drop.
- `rubato`'s FFT and polynomial resamplers are substantially faster than our decimator at
  matching quality: `rubato/fft_sync` ≈ 6.2 µs (≈165 Melem/s), `rubato/poly_async_cubic`
  ≈ 4.3 µs (≈240 Melem/s) vs `fir_decimator/127` ≈ 79 µs (13 Melem/s) — roughly **12–18×**.
- The `rubato/poly_async_cubic` mode is the fastest but performs **polynomial
  interpolation without band-limited anti-aliasing** on downsampling, so it will alias on
  signals with energy near Nyquist. It is **not** quality-comparable to our anti-aliased
  FIR decimator; it is shown only to illustrate the cost floor. `rubato/fft_sync` and the
  sinc modes *do* apply anti-aliasing and remain 2–9× faster than our 127-tap decimator.

Decimation-factor sweep (`FIRDecimator`, 127 taps, 1024 input):

| Factor | time |
| ---: | ---: |
| 2 | 17.7 µs |
| 3 | 19.3 µs |
| 4 | 20.5 µs |
| 8 | 23.4 µs |

Cost is dominated by the fixed tap count, so it rises only gently with factor (fewer
output samples to compute).

Arbitrary ratio 48 kHz → 44.1 kHz (rubato only — our `FIRDecimator` is integer-factor
only):

| Implementation | time | throughput |
| --- | ---: | ---: |
| `rubato/fft_sync` | 6.81 µs | 148.5 Melem/s |
| `rubato/sinc_async_cubic` | 55.9 µs | 18.1 Melem/s |
| `rubato/poly_async_cubic` | 4.47 µs | 226.2 Melem/s |

---

## Audio effects & engine — `tpt-dsp-audio`

All `f32`, mono, block sizes noted.

| Benchmark | Size | time | throughput |
| --- | ---: | ---: | ---: |
| `effects/convolution_reverb` | 256 | 31.7 µs | 8.07 Melem/s |
| `effects/convolution_reverb` | 512 | 66.4 µs | 7.71 Melem/s |
| `effects/convolution_reverb` | 1024 | 136.4 µs | 7.51 Melem/s |
| `effects/eq/3band` | 64 | 734 ns | 87.2 Melem/s |
| `effects/eq/3band` | 256 | 2.63 µs | 97.2 Melem/s |
| `effects/eq/10band` | 64 | 188 ns | 340 Melem/s |
| `effects/eq/10band` | 256 | 672 ns | 381 Melem/s |
| `effects/eq/lowshelf` | 256 | 2.56 µs | 100 Melem/s |
| `effects/eq/highshelf` | 256 | 2.60 µs | 98.5 Melem/s |
| `effects/delay` | 256 | 629 ns | 407 Melem/s |
| `effects/delay` | 1024 | 2.52 µs | 406 Melem/s |
| `effects/waveshaper` | 256 | 1.97 µs | 130 Melem/s |
| `effects/pedalboard_chain` (4 FX) | 128 | 16.0 µs | 8.02 Melem/s |
| `graph/8_node_sum` | 256 | 1.11 µs | 232 Melem/s |
| `graph/8_node_sum` | 1024 | 4.22 µs | 243 Melem/s |
| `graph/16_node_sum` | 256 | 2.16 µs | 119 Melem/s |
| `graph/16_node_sum` | 1024 | 8.42 µs | 122 Melem/s |
| `engine/realtime` (8-node graph + 4 FX) | 256 | 3.13 µs | 81.7 Melem/s |
| `engine/realtime` (8-node graph + 4 FX) | 1024 | 12.5 µs | 81.9 Melem/s |

The `pedalboard_chain` and `engine/realtime` are dominated by the convolution reverb
inside the chain (same ~8 Melem/s ceiling). At 48 kHz a 128-sample block is 2.67 ms of
audio, so 16 µs of work is ~**160× real-time headroom** on this path; a 256-sample
reverb block (31.7 µs) is ~**168×** real-time.

---

## Synthesis — `tpt-dsp-audio`

256-sample `f32` block unless noted.

| Benchmark | time | throughput |
| --- | ---: | ---: |
| `oscillator/sine` | 1.77 µs | 144 Melem/s |
| `oscillator/sawtooth` | 433 ns | 591 Melem/s |
| `oscillator/square` | 436 ns | 587 Melem/s |
| `oscillator/triangle` | 483 ns | 530 Melem/s |
| `wavetable/256` | 1.34 µs | 191 Melem/s |
| `wavetable/1024` | 1.30 µs | 198 Melem/s |
| `wavetable/4096` | 1.35 µs | 190 Melem/s |
| `fm_2op/index 0` | 7.72 µs | 33.2 Melem/s |
| `fm_2op/index 3` | 8.40 µs | 30.5 Melem/s |
| `subtractive/single_voice` | 8.14 µs | 31.5 Melem/s |
| `subtractive/8_voice` | 68.2 µs | 30.0 Melem/s |
| `adsr` | 674 ns | 380 Melem/s |

Wavetable lookup is table-size independent (linear interpolation, one branch per sample).
The subtractive voice scales ~linearly with voice count (8 voices ≈ 8.4× one voice),
so polyphony cost is predictable.

---

## Deferred / out-of-scope comparisons

These were explicitly **not** implemented and must not be inferred from the numbers above:

- **JUCE (`juce::dsp`)** — JUCE is a C++ framework; it cannot be linked into this pure-Rust
  crate without a substantial FFI/ABI boundary (and a JUCE license/build). A fair
  comparison would require either a C++ benchmark executable or a maintained Rust binding
  (none is a workspace dependency). Deferred. Methodology note: any future comparison
  should pin the same block size, SIMD level (`target-cpu=native` on both sides), and a
  single quality-matched workload (e.g. 2× decimation or a 10-band EQ).
- **`libsamplerate` (Secret Rabbit Code)** — C library, same linking constraints as JUCE.
  `rubato` (above) is the closest pure-Rust analog and is used in its place for the
  resampling comparison. If `libsamplerate` is ever bound, the `resampling_bench.rs`
  harness already isolates the workload and can accept a third backend with minimal change.

## Honesty notes

- No external (JUCE / libsamplerate) numbers are presented; only `rubato`, which is a
  declared bench-only dev-dependency.
- Relative speedups in the resampling section are computed from a **single shared run** of
  `resampling_bench`, so they are not contaminated by run-to-run noise.
- Absolute throughputs are **noisy** on the reference machine (see Environment); re-run on
  an idle host before quoting exact figures.
- The `rubato/poly` mode is shown for cost-floor context only and is **not** quality
  equivalent to our anti-aliased FIR decimator (it can alias).
- Production code was not modified; only `Cargo.toml` dev-dependencies (`rubato`,
  `audioadapter-buffers`) and the `benches/` directories were added/extended.
