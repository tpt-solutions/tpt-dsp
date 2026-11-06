# tpt-dsp — Architecture & Design Guide

> Scope note: this document describes the framework **as it is in the source
> tree** (last revised 2026-11-07, covering the 0.2.0 release and the unreleased
> work after it). Every workspace member is implemented. Where a feature is only
> partly built, §7 says so.
>
> The crate is dual-licensed MIT / Apache-2.0 © TPT Solutions.

---

## 1. Overview & design principles

`tpt-dsp` is a pure-Rust, real-time-safe DSP framework. The workspace has ten
members:

- the libraries `tpt-dsp-core`, `tpt-dsp-audio`, `tpt-dsp-analysis`,
  `tpt-dsp-control` and `tpt-dsp-io`;
- the test helper `tpt-dsp-testkit` and the `tpt-dsp` facade crate;
- the applications `tpt-dsp-wasm` (browser pedalboard), `tpt-dsp-viz` (desktop
  viewer) and `tpt-dsp-cli` (command line).

Two more crates, `tpt-dsp-nihplug` (CLAP/VST3 plugin) and `tpt-dsp-py` (Python
bindings), are excluded from the workspace because of their licences and
toolchain needs.

The guiding contract is:

- **Hot paths never allocate.** Processing functions take caller-supplied or
  pre-allocated buffers and reuse them across calls. Where a struct owns buffers,
  those buffers are allocated **once at construction**; every `process` / `tick`
  call is allocation-free in steady state.
- **`no_std` where it counts.** `tpt-dsp-core` builds for bare metal with
  `--no-default-features` and links for `thumbv7em-none-eabihf`. The other
  library crates use `std` (see §6).
- **No `unsafe` in core.** `tpt-dsp-core` is `#![forbid(unsafe_code)]`.
- **Composable primitives.** Small, single-purpose types (filters, oscillators,
  analyzers, processors) combine into graphs and pipelines.

---

## 2. Workspace layout & dependency direction

```
                  tpt-dsp-core                 (leaf, no_std)
                 ▲     ▲      ▲
      ┌──────────┘     │      └──────────┐
 tpt-dsp-audio   tpt-dsp-analysis   tpt-dsp-io
      ▲                ▲                 ▲
      │                └──── tpt-dsp-viz ┘  (analysis, io)
      ├──── tpt-dsp-wasm   (core, audio)
      └──── tpt-dsp-cli    (core, audio, analysis, io)

 tpt-dsp-control    no workspace dependencies
 tpt-dsp-testkit    no workspace dependencies; dev-dependency of tpt-dsp-wasm
 tpt-dsp            facade: re-exports core, audio, analysis, control, io
```

- `tpt-dsp-core` is a **leaf**. It depends only on external crates
  (`num-complex`, `num-traits`, and under `std` `rustfft` and `crossbeam-channel`).
- `audio`, `analysis` and `io` each depend **only on `core`**. They do not depend
  on each other.
- `tpt-dsp-control` has no workspace dependencies. Its PID, input-shaping and
  kinematics code is self-contained `f32` code.
- `tpt-dsp-testkit` is used only as a dev-dependency, so it never ships in a
  library's dependency tree.
- The applications sit on top of the libraries. The dependency graph is acyclic.

### Crate responsibilities

| Crate | Purpose | `no_std`? |
| --- | --- | --- |
| `tpt-dsp-core` | Math engine: complex helpers, FFT/DCT/MDCT/Hilbert, windows, biquad/FIR/IIR design (Butterworth, Chebyshev I/II, elliptic), equiripple and least-squares FIR, convolution, polyphase and decimating resamplers, pitch, LPC/LSP, vector quantisation, receiver blocks (PLL, AGC, squelch, AM/SSB), RF helpers (NCO, Goertzel, CIC), processor combinators, ring buffers, SPSC queues. | **Yes** (`--no-default-features`) |
| `tpt-dsp-audio` | Oscillators and synthesis (including PolyBLEP), effects (waveshaper, delay, convolution and algorithmic reverb, EQ), dynamics, modulation, oversampling, audio graph, real-time engine. | No (uses `std`) |
| `tpt-dsp-analysis` | Spectrum analysis, spectrograms, time-series statistics, features, loudness, MFCC, STFT and phase vocoder, onset detection, YIN pitch, async stream adapters. | No (`std`; async features are opt-in) |
| `tpt-dsp-control` | PID (with builder and anti-windup), ZVD input shaping, jerk-limited and trapezoidal trajectories. | No |
| `tpt-dsp-io` | IQ byte-stream parsing, WAV read/write, `rtl_tcp` source with tuning commands, optional built-in audio I/O, serial reader, async TCP IQ server (feature-gated). | No |
| `tpt-dsp-testkit` | Counting global allocator and `assert_no_alloc` for allocation-free tests. | No |
| `tpt-dsp` | Facade: feature-gated re-exports, a `prelude`, and a compiled walkthrough. | No |
| `tpt-dsp-wasm` | WebAssembly pedalboard and filter designer for the Web Audio `AudioWorklet`. | n/a (wasm) |
| `tpt-dsp-viz` | Desktop waterfall and spectrum viewer (in-tree software renderer over `minifb`). | No |
| `tpt-dsp-cli` | Command-line WAV/IQ pipeline and design tools. | No |

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

- **`IirFilter` / `Eq` scratch buffers**: reserved at construction
  (`IirFilter::with_max_block`, `Eq::with_max_block`; the default block limit is
  `DEFAULT_MAX_BLOCK` = 1024). `process` does not allocate for blocks up to that
  size. A longer block grows the scratch buffer once, then reuses it.
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

## 4. Application data flows

### 4.1 MVP 1 — Web guitar pedal  **(implemented)**

Target: a browser/WASM audio engine driving a pedalboard chain.

```
Source (Oscillator / mic) ─▶ Distortion (Waveshaper)
        ─▶ Delay ─▶ Reverb (ConvolutionReverb) ─▶ EQ (Eq) ─▶ Sink (built-in audio / Web Audio)
```

Building blocks in `tpt-dsp-audio` (all mono, in-place `&mut [f32]` effects):

- `Waveshaper` (curve: `Tanh`/`HardClip`/`Cubic`/`Polynomial`) — distortion. It
  does not oversample itself; wrap it in an `Oversampler` where aliasing matters.
- `Delay` — feedback delay line with fractional, glided delay times
  (`set_delay_fractional`).
- `ConvolutionReverb` — partitioned FFT convolution (`core::FftConvolver`), exact
  for an impulse response of any length. The pedalboard uses a short ambience
  impulse response of one block, which gives an early-reflection character.
- `Eq` — cascaded RBJ biquads (peaking and shelves).
- `Oscillator`, `PolyBlepOscillator`, `Wavetable`, `FmSynth`, `SubtractiveVoice`
  — sources and voices.
- `AudioGraph` — one `Source`, a `Vec` of `AudioNode`s, and one `Sink`, driven
  block by block over two scratch buffers. **Linear mono chain only**: no DAG,
  fan-in, fan-out or multi-channel routing.
- `RealtimeEngine` — drives a `FnMut(&[f32], &mut [f32])` per fixed block
  (constants `BLOCK_128`, `BLOCK_256`).

**Implemented** (see `tpt-dsp-wasm` and `www/`): the `wasm32`/`wasm-bindgen`
build, the Web Audio `AudioWorklet` integration, the pedalboard UI, a filter
designer page (`www/designer.html`), and the GitHub Pages deploy
(`.github/workflows/pages.yml`). CI builds the wasm target but does not run the
browser demo. The pedalboard is a fixed chain, not a general graph.

> Note: effects expose `process(&mut [f32])` (in-place) whereas `AudioGraph`
> nodes use `AudioNode::process(&[f32], &mut [f32])`. Effects are wired into a
> graph via `ClosureNode` adapters, not by directly implementing `AudioNode`.

### 4.2 MVP 2 — SDR spectrum analyzer & FM demodulator  **(implemented, RTL-SDR via `rtl_tcp`)**

Target: ingest RTL-SDR IQ, decimate, FFT, optionally FM-demodulate, render a
waterfall.

```
IQ bytes ─▶ parse_iq / IqStream / IqReassembler ─▶ FIRDecimator or CicDecimator
   ─▶ FftPlan / RealtimeSpectrumAnalyzer ─▶ Spectrogram (waterfall)
   ─▶ (optional) FmDemodulator ─▶ audio out / analysis
```

Building blocks now present:

- **Ingestion (`tpt-dsp-io`):** `parse_iq(format, bytes, &mut [Complex32])`, the
  buffered `IqStream`, and the bounded `IqReassembler`, which keeps a sample that
  is split across reads. Formats: `U8`, `I16Le`, `I16Be`, `F32Le`.
  `TcpIqSource::connect_rtl_tcp` reads an `rtl_tcp` greeting and tunes the server
  (frequency, sample rate, gain, PPM). `serve_iq` provides an async TCP server
  (`tcp` feature).
- **Decimation and resampling (`tpt-dsp-core`):** `FIRDecimator` (integer-factor
  windowed-sinc), `CicDecimator` (multiplier-free, for large factors), and the
  polyphase `RationalResampler` for arbitrary ratios. State carries across
  blocks of any size.
- **Mixing and tracking:** `Nco` (oscillator and mixer), `Goertzel` (single-tone
  power), `Pll` with selectable detectors, `Agc`, `Squelch`, and AM and SSB
  demodulators.
- **Transform (`tpt-dsp-analysis`):** `RealtimeSpectrumAnalyzer` windows a block,
  runs `FftPlan` (RustFFT), folds to a one-sided magnitude spectrum, time-averages,
  and converts to dB. `peak()` returns a sub-bin frequency by parabolic
  interpolation. `Spectrogram` keeps the most recent `rows` frames in a
  pre-allocated ring for waterfall rendering.
- **FM demod (`tpt-dsp-core`):** `FmDemodulator` / `phase_delta` /
  `phase_to_audio` — a phase-delta discriminator, `no_std`, `Copy`, and
  allocation-free.
- **Desktop waterfall UI (`tpt-dsp-viz`):** implemented. A producer thread
  (`pipeline::run_synthetic` or, with the `audio` feature, `run_audio_input`)
  streams `SpectrumFrame`s to the UI over a `crossbeam-channel`.
  `analyze_block` windows, transforms and averages one block with
  `RealtimeSpectrumAnalyzer`. `VizApp` (`app.rs`) draws the `Spectrogram` into a
  software pixel buffer (`canvas.rs`, bitmap font in `font.rs`), colour-mapped by `colormap`
  (black → blue → cyan → yellow → red), with the latest frame as a live spectrum
  line, a peak-frequency and dB readout, and a pause toggle. `run_audio_input`
  captures the default input device through the built-in backend, downmixing
  F32/I16/U16 to mono. The only per-frame allocation is the `Vec<f32>` of dB values
  sent over the channel.

**Not yet implemented:** a native RTL-SDR USB driver. The `io` `rtl-sdr` feature
is a stub that only changes the error `RtlSdrSource` reports. The supported
hardware path is `rtl_tcp` (see `docs/RTL_SDR.md`).

---

## 5. Module-by-module overview

### 5.1 `tpt-dsp-core` (`src/lib.rs`)

| Module | Key public types / functions |
| --- | --- |
| `complex` | `Complex32`/`Complex64`, aliases `C32`/`C64`; `magnitude`, `magnitude_squared`, `phase`, `exp_i`, `rotate` |
| `fft` | `fft`, `ifft`, `fft_inplace`, `ifft_inplace`, `twiddles`, `is_power_of_two`, `next_power_of_two` — hand-rolled radix-2, power-of-two only, `no_std` |
| `plan` (`std`) | `FftPlan` — RustFFT-backed, arbitrary length, buffers allocated once |
| `dct` | `dct_ii`, `dct_iii`, `dct_iv`, `dct_2d`, `idct_2d` — direct O(N²), allocation-free. FFT-backed fixed-size plans: `FastDct2Plan`, `FastDct2dPlan`, `FastDctIvPlan`, `FastMdctPlan` |
| `mdct` | `mdct`, `imdct`, `MdctPlan`, `FastMdctPlan`, `mdct_sine_window` |
| `hilbert` | `hilbert` (free) + `HilbertTransformer` (`alloc`) — analytic-signal transform |
| `windows` | `windowed`, `windowed_symmetric`, `windowed_f64`, `windowed_symmetric_f64`; `WindowType`: `Rectangular`, `Hann`, `Hamming`, `Blackman`, `FlatTop`, `Kaiser(beta)`, `Tukey(alpha)`, `Gaussian(sigma)` |
| `filters` | `Biquad`, `BiquadCoeffs`, `BiquadBuilder`, `BiquadType`, `process_biquad`; `alloc`: `Fir`, `FirDesign`, `IirFilter`/`IirCoeffs`/`IirStage`, `halfband_fir`; `FilterDesignError` |
| `iir_design` | `OnePole`, `DcBlocker`, `Svf` (allocation-free building blocks); Butterworth (`butterworth`), Chebyshev I/II (`chebyshev1_*`, `chebyshev2_*`), elliptic (`elliptic_*`) for low-pass, high-pass, band-pass and band-stop (`alloc`); `magnitude_response`, `cascade_magnitude_response`, `cascade_group_delay`; `cascade_poles_zeros` (`alloc`) |
| `remez` (`alloc`) | `remez_fir` (Parks–McClellan), `least_squares_fir`, `RemezBand` |
| `convolution` | `convolve` (direct); `alloc`: `ConvolvePlan`, `FftConvolver` (partitioned overlap-add, exact for any impulse-response length) |
| `polyphase` | `RationalResampler` (`alloc`), `FarrowDelay` |
| `resample` (`alloc`) | `FIRDecimator` — integer decimation with windowed-sinc anti-alias |
| `rf` | `Nco`, `Goertzel`, `CicDecimator` (`alloc`) |
| `receiver` | `Pll` with `Detector` (including `DecisionDirectedQpsk`), `Agc`, `Squelch`, `AmDemodulator`, `SsbDemodulator` |
| `process` | `Processor` trait, `Chain`, `Smoothed`, `Cascade` (`alloc`) |
| `demod` | `FmDemodulator`, `phase_delta`, `phase_to_audio` |
| `lpc`, `lsp` (`alloc`), `pitch`, `psychoacoustic`, `bark` | LPC analysis, line spectral pairs, pitch estimation, Bark/ERB scales, masking model |
| `vq`, `noise_shaping` | vector quantisation (`VqCodebook`), error-feedback noise shaping (`NoiseShaper`) |
| `ring` | `RingBuffer`, traits `RingRead`/`RingWrite` |
| `spsc` (`std`) | `SpscQueue`, `Producer`, `Consumer` (crossbeam) |
| `simd` | `complex_add_simd`, `complex_mul_simd`, `magnitude_simd` (nightly `simd` feature, or a scalar fallback) |

Feature flags: `std` (default) = `alloc` + `rustfft` + `crossbeam-channel`;
`alloc` enables the owning structs and the design functions above.

### 5.2 `tpt-dsp-audio` (`src/lib.rs`)

| Module | Key types |
| --- | --- |
| `oscillator` | `Oscillator`, `Waveform` (Sine/Sawtooth/Square/Triangle) — phase accumulator, **naive (aliasing) waveforms** |
| `synthesis` | `PolyBlepOscillator`, `BlepShape`, `poly_blep`, `NoiseSource`, `PinkNoise`, `TpdfDither` — band-limited oscillator and noise sources |
| `wavetable` | `Wavetable` — single-period lookup with linear interpolation, **no mipmaps or morphing** |
| `fm` | `FmSynth` — 2-operator FM |
| `subtractive` | `SubtractiveVoice` — osc → low-pass biquad → ADSR gain |
| `waveshaper` | `Waveshaper`, `Curve` — distortion (no internal oversampling) |
| `oversampling` | `Oversampler` — runs a nonlinear closure at a higher rate, with reported latency |
| `delay` | `Delay` — feedback delay with fractional, glided times |
| `reverb` | `ConvolutionReverb`, `generate_decay_ir` — FFT convolver wrapper |
| `freeverb` | `Freeverb` (mono), `StereoFreeverb` (with stereo spread) |
| `eq` | `Eq` — cascaded biquad peaking and shelf bands; `try_new`, `with_max_block` |
| `dynamics` | `Compressor`, `Limiter` (no lookahead), `Gate` |
| `modulation` | `Lfo`, `Tremolo`, `Chorus`, `Flanger`, `Phaser` |
| `envelope` | `Adsr`, `EnvelopeState` |
| `graph` | `AudioGraph`, traits `AudioNode`/`Source`/`Sink`, `ClosureNode`/`ClosureSource`/`ClosureSink`, `Passthrough` |
| `engine` | `RealtimeEngine`, `BLOCK_128`, `BLOCK_256` |

### 5.3 `tpt-dsp-analysis` (`src/lib.rs`)

| Module | Key types / functions |
| --- | --- |
| `spectrum` | `SpectrumAnalyzer`, `RealtimeSpectrumAnalyzer`, `SpectrumConfig`, `Averaging`, `SpectralPeak`, `DEFAULT_DB_FLOOR`; `find_peaks`, `peak_bin`, `dominant_frequency`, `linear_to_db`, `db_to_linear`, `parabolic_interpolate` |
| `features` | `rms`, `zero_crossing_rate`, `spectral_centroid`, `spectral_centroid_normalized`, `spectral_flatness`, `spectral_rolloff`, `spectral_flux` |
| `timeseries` | `MovingAverage`, `RunningMean`, `Ema`, `OutlierDetector` (median/MAD, sorted window) |
| `loudness` | `LoudnessMeter` — ITU-R BS.1770 integrated loudness |
| `mfcc` | `Mfcc`, `hz_to_mel`, `mel_to_hz` |
| `stft` | `stft`, `istft`, `time_stretch`, `pitch_shift` (phase vocoder) |
| `onset` | `detect_onsets`, `pick_onsets`, `OnsetConfig` |
| `yin` | `yin_pitch` |
| `spectrogram` | `Spectrogram` — ring of magnitude frames for waterfall |
| `async_adapters` (`async-tokio` / `async-std`) | `process_channel`, `process_stream` (tokio); runtime-agnostic `process_stream_into_sink` and `process_stream_in_place`; the `tokio` and `async_std` submodules |

The default build has no async runtime. `async` is a shorthand for `async-tokio`.
`async-std` is an alternative. Both pull in `futures`.

### 5.4 `tpt-dsp-control` (`src/lib.rs`)

| Module | Key types |
| --- | --- |
| `pid` | `Pid`, `PidBuilder`, `AntiWindup` (clamping / back-calculation), `PidError` — derivative on the measurement, optional derivative filter, `update_with_dt`, fixed-`dt` `update` |
| `input_shaping` | `InputShaper`, `ShaperError` — **ZVD only** (no ZV, ZVDD, EI or multi-mode shapers) |
| `kinematics` | `TrapezoidalProfile`, `ProfileError`, `JerkLimiter` — a jerk-limited S-curve velocity smoother |

### 5.5 `tpt-dsp-io` (`src/lib.rs`)

| Module | Key types | Feature |
| --- | --- | --- |
| `iq` | `parse_iq`, `IqFormat`, `IqStream`, `IqReassembler` | always |
| `source` | `IqSource`, `SyntheticIqSource` | always |
| `tcp` | `TcpIqSource`, `RtlTcpCommand`, `RtlTcpInfo`, `read_rtl_tcp_greeting`, `serve_iq` | client always; `serve_iq` needs `tcp` — a **single-connection** async server |
| `rtlsdr` | `RtlSdrSource`, `RtlSdrConfig` — stub | always (`rtl-sdr` changes only the error) |
| `wav` | `read_wav_f32_path`, `write_wav_f32_path` and reader/writer variants | always |
| `audio` | `run_output`, `run_input`, `run_output_on_device`, `run_input_on_device`, `list_output_devices`, `list_input_devices` — mono f32 output, capture input, device selection | `audio` (built-in backends: WASAPI on Windows, raw ALSA on Linux, CoreAudio on macOS) |
| `serial` | `SerialReader` — read-only | `serial` |

### 5.6 Applications

| Crate | Key types |
| --- | --- |
| `tpt-dsp-wasm` | `Pedalboard`, `FilterSpec` (designer), `open_microphone` (feature `async`); the wasm-bindgen exports in `web.rs` |
| `tpt-dsp-viz` | `VizApp`, `run`, `Source`, `SpectrumFrame`, `run_synthetic`, `run_audio_input` (feature `audio`), `Spectrogram`-backed waterfall |
| `tpt-dsp-cli` | `read_wav`/`write_wav`, `Effect`/`EffectChain`, `read_iq`/`demod_iq`, `analyze_real`/`analyze_complex`; `tools` (`generate`, `resample`, `design_response`, `effect_response`, `parse_pipeline_json`/`_toml`) |
| `tpt-dsp-testkit` | `CountingAllocator`, `allocations_in`, `assert_no_alloc` |
| `tpt-dsp` | Re-exports, `prelude`, `walkthrough` |

---

## 6. `no_std` / embedded story & CI

- **`tpt-dsp-core`** is the only `no_std` crate. `cargo build -p tpt-dsp-core
  --no-default-features` succeeds, and it links for `thumbv7em-none-eabihf
  --no-default-features`. Types that need `alloc` are excluded without it. These
  include `Fir`, `IirFilter`, the IIR and FIR design functions, `RationalResampler`,
  `CicDecimator`, `Cascade`, `HilbertTransformer`, `FftConvolver` and
  `FIRDecimator`. Types that need `std` are excluded too: `FftPlan` and
  `SpscQueue`.
- **The other library crates** need `std`: they use `std::vec`, platform audio
  APIs, or async runtimes. No `embedded-hal` port exists yet.
- **CI** (`.github/workflows/ci.yml`): native build, test, clippy, benchmark
  compile, and `no_std` checks for core on its own and with `alloc`; the
  `wasm32-unknown-unknown` job builds `tpt-dsp-wasm` and checks core and audio;
  the `thumbv7em-none-eabihf` job builds and clippy-checks core; plus `cargo deny`,
  `cargo doc`, a feature matrix, a fuzz smoke test, a line-coverage gate, MSRV
  (excluding `tpt-dsp-viz`), Windows and macOS builds, and a Python wheel built
  and tested with NumPy.

---

## 7. Known limitations (accurate as of 2026-11-07)

**Build and test health.** The full workspace suite passes on the native gate.
Golden-vector tests check the filter designs, windows and Remez FIR against SciPy.
Property tests cover FFT, DCT, ring buffer, WAV and LPC/LSP round trips. `cargo
deny` passes on the modern schema.

**Zero-alloc contract** — see §3.2. Steady-state allocation-free for core math,
filters, reverb, EQ, dynamics, modulation and `OutlierDetector`. The exceptions are
the async adapters, which allocate one output buffer per frame, and an `IirFilter`
or `Eq` that is given a block longer than the size it was constructed for.

**Functionality gaps:**
- Audio graph: linear mono chain only (no DAG, fan-in, fan-out or multichannel).
- Oscillator: naive waveforms. Use `PolyBlepOscillator` for band-limited saw and
  square. `Wavetable`: no mipmaps. `FmSynth`: two operators only.
- Waveshaper: no internal oversampling. Wrap it in `Oversampler`.
- `tpt-dsp-control`: `InputShaper` is ZVD only. `JerkLimiter` is an S-curve velocity
  smoother, not a full multi-axis planner.
- Hand-rolled `fft`: power-of-two only. Use `FftPlan` (RustFFT) for other lengths.
  It recomputes its twiddle table on each call. For repeated transforms, prefer
  `FftPlan`.
- Windows: `WindowType` is f32 for `windowed` and has f64 variants through
  `windowed_f64` and `windowed_symmetric_f64`.
- I/O: built-in WASAPI is runtime-validated on Windows. Raw ALSA and CoreAudio
  compile on their platforms but have not been runtime-validated. Audio is mono
  output and capture only, with no duplex. Serial is read-only. The `tcp` server
  accepts one connection. The RTL-SDR USB driver is not implemented; use `rtl_tcp`.
- Pedalboard: mono, and its parameter changes are stepped rather than smoothed.
- The `simd` feature is nightly-gated (see §10). CI's native job builds on stable,
  so it only runs the scalar fallback.

**Roadmap (not started):**
- `embedded-hal` Cortex-M validation.
- Criterion benchmark comparisons against JUCE and libsamplerate.
- A crates.io 1.0.0 release. The 0.2.0 release is published (see
  [`PUBLISHING.md`](PUBLISHING.md)).

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
- Prove a new hot path is allocation-free with `tpt-dsp-testkit`.

---

## 9. CLI, plugin, Python and web bindings (2026-11-07)

Four crates wrap the framework for use outside Rust:

- **`tpt-dsp-cli`** (workspace member) — a command-line WAV/IQ pipeline.
  Subcommands: `filter` runs biquad, EQ, waveshaper, delay and convolution-reverb
  chains; `pipeline` runs the same effects from a JSON or TOML spec file; `gen`
  writes test signals; `resample` converts sample rate with the polyphase
  resampler; `design` and `response` print filter and chain responses as CSV;
  `demod` FM-demodulates raw IQ to WAV; `spectrum` reports the averaged spectrum and
  features; `info` prints file metadata; `completions` prints shell completions.
  WAV I/O uses `tpt-dsp-io`'s built-in RIFF/WAVE module.
- **`tpt-dsp-nihplug`** (excluded) — a CLAP/VST3 plugin wrapping the
  `tpt-dsp-audio` pedalboard (Waveshaper → Delay → ConvolutionReverb → 3-band
  EQ) as a host-parameterised chain. It is built on `nice-plug`, the maintained
  successor to `nih-plug`, and exported with `nice_export_clap!` /
  `nice_export_vst3!`. `templates/nihplug-effect` is a `cargo-generate` template for
  new plugins.
- **`tpt-dsp-py`** (excluded) — pyo3 bindings exposed as the `tpt_dsp` package:
  `rms`, `zero_crossing_rate`, `spectral_centroid`, `spectrum`, `fm_demod` and
  `analyze`, plus the `Biquad`, `Pid` and `Oscillator` classes. A `zero-copy`
  feature reads NumPy `float32` arrays in place.
- **`tpt-dsp-wasm`** (workspace member) — the browser pedalboard and filter designer
  described in §4.1.

`nihplug` and `py` are **excluded** from the main workspace (root `Cargo.toml`
`exclude`). The plugin pulls VST3/CLAP SDK bindings whose licences fall outside the
main `deny.toml` allow-list. The Python module links Python (`extension-module`),
so it cannot be built or tested by the `cargo build/test --workspace` gate. Both
are verified as standalone workspaces (`cargo build` or `cargo clippy` inside each
directory).

---

## 10. SIMD: architecture support & feature detection (2026-09-07)

`tpt-dsp-core::simd` (§5.1) has two implementations behind one API: a scalar
fallback (`simd_scalar.rs`, always available, the default) and a
`core::simd`-based vectorised one (`simd.rs`, nightly-only via the `simd`
feature). `lib.rs` picks between them at compile time — see §5.1 and the
module docs on `simd.rs` for the mechanics.

### Supported architectures

`core::simd` is a *portable* SIMD abstraction: there is exactly one source
file, and LLVM lowers its `f32x4` operations to whatever vector instructions
the compilation target provides. No per-architecture code exists or is
needed.

| Target | Baseline vector ISA used | Status |
| --- | --- | --- |
| x86 / x86_64 | SSE2 (guaranteed baseline on `x86_64-*`) | Compiles; not exercised in CI (see below) |
| ARM / AArch64 | NEON (guaranteed baseline on `aarch64-*`) | Compiles; not exercised in CI |
| wasm32 | `simd128`, if the target build enables it | Not currently built with `simd` on in CI |
| RISC-V | RVV once LLVM/rustc give it stable tier support | Not yet targeted; no code changes expected — the same portable source should pick it up |

Wider ISA levels (AVX2/AVX-512 on x86_64, SVE on AArch64) are a **build-time**
choice for whoever compiles the final binary (`-C target-cpu=native` or an
explicit `-C target-feature=...`), not something `tpt-dsp-core` selects
itself. Without such a flag, a release build gets only the guaranteed
baseline for its target triple.

Practical gap: the `simd` cargo feature requires nightly to actually take the
`core::simd` path (via the `tpt_portable_simd` cfg from `build.rs`), but every
CI job builds on the *stable* toolchain — including the `native` job's
`cargo build --workspace --all-features`. So `--all-features` in CI always
resolves `simd` back to the scalar fallback; the vectorised path is compiled
and tested only when a contributor runs it locally on nightly. This is a real
coverage gap, not a design conclusion — added here as an accurate account of
current CI, not a target for this section's remaining work.

### Runtime feature detection

There is deliberately no `is_x86_feature_detected!`/`is_aarch64_feature_detected!`-style
runtime dispatch to a wider ISA in this crate, for two independent reasons:

1. **`tpt-dsp-core` is `#![forbid(unsafe_code)]`** (§3.4). The standard
   runtime-multiversioning pattern — detect a CPU feature at runtime, then
   call into a function marked `#[target_feature(enable = "avx2")]` — requires
   an `unsafe` call at the call site (the compiler cannot itself prove the
   feature check dominates the call). That pattern is unavailable here by the
   crate's own safety policy, not an oversight.
2. **`core::simd` does not runtime-dispatch.** It compiles to one fixed set of
   vector instructions, chosen by the target build's compile-time flags. There
   is no "detect AVX2 at runtime and take a wider path" mode to plug in — the
   only lever is the target-feature/target-cpu flags passed when the
   *consuming* binary is built.

The nightly-vs-stable check in `build.rs` (setting `tpt_portable_simd`) is a
different kind of check — a toolchain capability probe, not a CPU-feature
probe — and remains the only "detection" this crate performs. Given the two
points above, runtime CPU-feature dispatch is judged **not appropriate** for
`tpt-dsp-core` as designed; a consumer that wants AVX2/AVX-512/SVE codegen
gets it by building the final binary with the matching `-C target-cpu`/
`-C target-feature` flags, same as they would for any other portable-SIMD
crate.

---

## 11. Real-time / Offline execution profiles (2026-09-07)

`todo.md` §14 asks for two things: explicit definitions of a **Real-time**
and an **Offline** execution profile, and a statement of which APIs satisfy
which. This section is that statement. It supersedes §3's looser prose with a
crate-by-crate table; §3 remains the canonical *reasoning* behind the
allocation caveats this section only classifies.

### 11.1 Profile definitions

**Real-time profile** — safe to call from a hard-real-time callback (an audio
device callback, an SDR sample-block callback, a control-loop tick):

1. **No allocation** — no heap traffic once the caller-owned/pre-allocated
   buffers exist.
2. **No blocking** — never waits on a lock, a channel, an I/O readiness event,
   or another thread.
3. **No I/O** — no file, socket, serial, or audio-device syscalls.
4. **No unbounded work** — per-call cost does not include an operation whose
   cost class is worse than the caller's fixed block size implies (in
   particular, no `O(n log n)`+ step hiding behind an apparently `O(n)` API).
5. **Caller-owned memory** — buffers are supplied or pre-allocated at
   construction; a function/struct never grows its own storage during
   `process`/`tick`.
6. **Deterministic execution bounds where possible** — every loop terminates
   within a bound computable from the input size or a fixed constant, so a
   worst-case execution time exists even if the typical-case count varies
   with data (a bisection search that exits early on most inputs still
   satisfies this if its *worst* case is capped).

**Offline profile** — anything not required to meet the above; explicitly
permits:

1. Dynamic allocation (per-call, not just at construction).
2. Larger/growable scratch buffers.
3. Parallel processing.
4. Expensive RDO-style search (e.g. re-trying several encodings and keeping
   the best).

Criteria 1-4 of the real-time profile are graded **strictly on the whole
workspace surface** below — a single real, reachable exception is enough to
leave a criterion unchecked in `todo.md`, even where the exception lives in a
clearly-labelled non-real-time convenience API. That is a deliberate reading:
"real-time profile" is a promise about what a *type* does, and a type that
exposes both a blocking and a non-blocking method makes that promise only for
the non-blocking half.

### 11.2 Where each criterion currently holds

- **No allocation**: true in steady state for the free-function and
  pre-allocated-struct idioms (§3.1); **not unconditional** — `tpt-dsp-
  analysis`'s `async_adapters` allocate a `Vec<f32>` per frame, and
  `IirFilter`/`Eq` reallocate scratch the first time a block is larger than
  any seen before (§3.2).
- **No blocking**: true for every free function and struct `process`/`tick`
  method in `tpt-dsp-core`, `-audio`, `-analysis` (sync path) and `-control`;
  **not unconditional** — `SpscQueue`/`Producer`/`Consumer` deliberately also
  expose blocking `send`/`recv` (crossbeam-backed) alongside the real-time-
  safe `try_send`/`try_recv` (`spsc.rs`'s own doc comments already label each
  method), and `tpt-dsp-analysis::async_adapters` blocks on an async runtime
  (`block_on`/`.await`) by design — it is a streaming/offline adapter, not a
  real-time one.
- **No I/O**: true for every type in `tpt-dsp-core`, `-audio`, `-analysis`,
  `-control` — none of the four touch a file, socket, serial port, or audio
  device. **Not** true of `tpt-dsp-io`, whose entire purpose is I/O
  (`audio::run_output`/`run_input`, `serial::SerialReader`, `tcp::serve_iq`);
  those are driver/setup functions that call into a caller-supplied
  real-time-safe processing closure, not processing functions themselves —
  the I/O happens in the driver loop around the callback, never inside it.
- **No unbounded work**: true for the large majority (fixed cost proportional
  to input length: biquad/PID/EMA are `O(1)` per sample, FIR/convolution/
  windowing/features are `O(n)`, FFT/DCT/MDCT/LPC autocorrelation are
  `O(n log n)`/`O(n²)` but bounded by the caller's chosen block size);
  **known exception** — `tpt-dsp-analysis::OutlierDetector::push` re-sorts its
  window every sample (`O(n log n)` per sample, not `O(1)`, §3.2). Adjacent
  but out of the real-time surface: `tpt-dsp-io::IqStream::feed` grows its
  internal buffer without bound if the caller never calls `drain` — a
  buffering/ingestion concern, not a per-block processing one, but worth
  naming here since it is the other unbounded-growth path in the workspace.
- **Caller-owned memory**: true everywhere, no known exception — the
  dominant idiom across every crate (§3.1).
- **Deterministic execution bounds where possible**: true everywhere audited.
  Every loop in `tpt-dsp-core` has a fixed or input-derived cap — FFT/DCT
  loop over `n`, `lsp.rs`'s root-finding grid search stops at a fixed
  `grid_points` and refines each root with an unconditional 40-iteration
  bisection, `simd.rs`'s vectorised loops process `len - len % LANES` then a
  bounded scalar tail. No polling loop, `while true`, or unbounded recursion
  exists anywhere in `tpt-dsp-core`/`-audio`/`-analysis`/`-control`. This
  criterion is about *worst-case* boundedness, so `OutlierDetector`'s
  data-independent `O(n log n)` sort satisfies it even though it fails the
  stricter "no unbounded work" reading above.

### 11.3 Per-crate / per-API classification

| Crate / module | Representative API | Profile |
| --- | --- | --- |
| `core::complex` | `Complex32`/`Complex64`, `magnitude`, `phase`, `exp_i`, `rotate` | Real-time |
| `core::fft` | `fft`, `ifft`, `fft_inplace`, `twiddles` | Real-time (recomputes twiddles per call — wasteful, not unbounded; prefer `FftPlan` for repeated transforms, §3.2) |
| `core::plan` (`std`) | `FftPlan::new` | Offline (allocates plan + scratch) |
| `core::plan` (`std`) | `FftPlan::process`/`process_inplace` | Real-time (reuses the scratch from `new`) |
| `core::dct` | `dct_ii`/`dct_iii`/`dct_iv` | Real-time (`O(n²)`, alloc-free, bounded by caller's `n`) |
| `core::mdct` (`std`) | `FastMdctPlan`/`FastDctIvPlan::new` | Offline (allocates an `FftPlan` internally) |
| `core::mdct` (`std`) | `FastMdctPlan`/`FastDctIvPlan::process` | Real-time |
| `core::hilbert` | `hilbert` (free fn) | Real-time |
| `core::hilbert` (`alloc`) | `HilbertTransformer::new` | Offline (allocates `work`/`scratch`) |
| `core::hilbert` (`alloc`) | `HilbertTransformer::process` | Real-time |
| `core::windows` | `windowed` | Real-time |
| `core::filters` | `Biquad`/`process_biquad` | Real-time |
| `core::filters` (`alloc`) | `Fir::new`, `IirFilter::new` | Offline (allocates coefficient/scratch storage) |
| `core::filters` (`alloc`) | `Fir`/`IirFilter::process` | Real-time (scratch reallocates only the first time a block exceeds every prior size seen, §3.2) |
| `core::convolution` | `convolve` (direct) | Real-time |
| `core::convolution` (`alloc`) | `ConvolvePlan::new`, `FftConvolver::new` | Offline |
| `core::convolution` (`alloc`) | `ConvolvePlan`/`FftConvolver::process` | Real-time |
| `core::ring` | `RingBuffer` (all methods) | Real-time (lock-free, single-owner) |
| `core::spsc` (`std`) | `SpscQueue::bounded`/`unbounded` | Offline (channel construction) |
| `core::spsc` (`std`) | `try_send`/`try_recv` | Real-time |
| `core::spsc` (`std`) | `send`/`recv` (blocking variants) | Offline (blocks by design — an explicit non-real-time convenience) |
| `core::demod` | `FmDemodulator`, `phase_delta`, `phase_to_audio` | Real-time |
| `core::resample` (`alloc`) | `FIRDecimator::new` | Offline |
| `core::resample` (`alloc`) | `FIRDecimator::process` | Real-time |
| `core::lpc` (`alloc`) | `LpcAnalyzer::new` | Offline (allocates `r`/`scratch`) |
| `core::lpc` (`alloc`) | `LpcAnalyzer::analyze`, free fns `autocorrelation`/`levinson_durbin`/`predict`/`residual`/`synthesize` | Real-time (write into caller-supplied `out`/`coeffs_out` slices) |
| `core::lsp` | `lpc_to_lsp`, `lsp_to_lpc`, `lsp_is_stable`, `lsp_interpolate` | Real-time (fixed-iteration bisection + a `grid_points`-bounded search, §11.2 — but conventionally called encoder-side, not on an audio callback) |
| `core::vq` | `nearest_vector`, `nearest_vector_accelerated`, `vq_encode`/`vq_decode` | Real-time (operates on caller-supplied codebook/output slices) |
| `core::bark`/`psychoacoustic` | `hz_to_bark`, `simultaneous_masking`, `combined_threshold_db`, ... | Real-time (pure per-band arithmetic on caller slices) — used encoder-side, but not disqualified by anything in its own implementation |
| `core::noise_shaping` | `NoiseShaper::new` | Offline (allocates `error_history`) |
| `core::noise_shaping` | `NoiseShaper::step` | Real-time |
| `core::pitch` | `autocorrelation_pitch`, `amdf_pitch` | Real-time (bounded by the caller's search-range slice) |
| `core::simd` | `complex_mul_simd`/`complex_add_simd`/`magnitude_simd`/`fft_butterfly` | Real-time |
| `audio::{oscillator,wavetable,fm,subtractive,waveshaper,delay,eq,envelope}` | `new` (most) | Offline (pre-allocates any owned buffer, e.g. `Wavetable`'s table) |
| `audio::{...}` | `process`/`tick` | Real-time |
| `audio::reverb::ConvolutionReverb` | `new` | Offline (builds on `FftConvolver::new`) |
| `audio::reverb::ConvolutionReverb` | `process` | Real-time |
| `audio::graph::AudioGraph` | `new` | Offline (allocates two scratch `Vec<f32>`, §3.2) |
| `audio::graph::AudioGraph` | `run`/`tick` | Real-time |
| `audio::engine::RealtimeEngine` | `run` (drives a `FnMut` per fixed block) | Real-time, contingent on the caller's closure also meeting the profile |
| `analysis::spectrum::RealtimeSpectrumAnalyzer` | `new` | Offline |
| `analysis::spectrum::RealtimeSpectrumAnalyzer` | `process`/`peak` | Real-time |
| `analysis::features` | `rms`, `zero_crossing_rate`, `spectral_centroid`(`_normalized`) | Real-time |
| `analysis::timeseries` | `MovingAverage`, `RunningMean`, `Ema` | Real-time (`O(1)` per sample) |
| `analysis::timeseries` | `OutlierDetector::push` | Real-time-capable but **not `O(1)`** — see §11.2; budget for `O(n log n)` in the window size before using it on a hard-real-time thread |
| `analysis::spectrogram::Spectrogram` | `new` | Offline (allocates the ring) |
| `analysis::spectrogram::Spectrogram` | `push_row`/frame access | Real-time |
| `analysis::async_adapters` (`async`) | `process_channel`, `process_stream`, tokio/async-std submodules | Offline (blocks on an async runtime; allocates a `Vec<f32>` per frame, §3.2) |
| `control::pid::Pid` | `update` | Real-time (`O(1)`) |
| `control::input_shaping::InputShaper` | `tick` | Real-time |
| `control::kinematics::JerkLimiter` | `update` | Real-time |
| `control::kinematics::TrapezoidalProfile` | `at` | Real-time |
| `io::iq` | `parse_iq` | Real-time (writes into a caller-supplied `&mut [Complex32]`, no I/O of its own) |
| `io::iq::IqStream` | `feed`/`drain` | Offline-leaning — `feed` grows its buffer unboundedly if `drain` is never called (documented risk); use the bounded `IqReassembler` for a real-time-safe ingestion path |
| `io::audio` | `run_output`/`run_input`, `list_*_devices` | Offline (blocking hardware I/O and device-list syscalls; these *drive* a real-time-safe callback, they are not one) |
| `io::serial::SerialReader` | `read` | Offline (blocking hardware I/O) |
| `io::tcp::serve_iq` | — | Offline (async network I/O) |
| `viz::pipeline` | `run_synthetic`/`run_audio_input` | Offline (spawns threads, `thread::sleep`-paced, and the audio-input path locks a `Mutex` around captured samples) — a UI producer thread, not a hot DSP path |
