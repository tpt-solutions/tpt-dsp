# tpt-dsp-wasm

MVP 1: a web-native guitar effects pedal. This crate is the WebAssembly front
end for [`tpt-dsp`](../README.md); the whole pedalboard —
**distortion → delay → reverb → EQ** — is `tpt-dsp-audio` DSP compiled to wasm
and driven from an `AudioWorklet` at one 128-sample render quantum per call.

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.

## Signal chain

| Stage      | `tpt-dsp-audio` type | Controls                                       |
| ---------- | -------------------- | ---------------------------------------------- |
| Distortion | `Waveshaper`         | `set_distortion` (drive), `set_distortion_mix`, `set_distortion_curve` |
| Delay      | `Delay`              | `set_delay_time` (s, ≤ 2 s), `set_delay_feedback`, `set_delay_mix` |
| Reverb     | `ConvolutionReverb`  | `set_reverb_mix`                                |
| EQ         | `Eq` (3 peaking bands: 100 Hz / 800 Hz / 3.2 kHz) | `set_eq_gain(band, dB)`, `set_output_gain` |

The four effects are wrapped in a single `AudioNode` implementation, so the
chain can be dropped straight into an `AudioGraph` (see `render_demo`, which
auditions the chain from an oscillator source without a microphone).

## Real-time contract

`process_block_128(&mut self, input: &[f32; 128], output: &mut [f32; 128])` is
the reference hot path: 128 samples in, 128 out, **no heap allocation**. Every
buffer it touches (the delay line, the reverb FFT scratch, the EQ scratch, the
wet mix buffer) is allocated in `Pedalboard::new`/`with_sample_rate`.

That claim is enforced, not asserted: the test
`process_block_128_does_not_allocate` installs a counting global allocator
(`std::alloc::System` behind a per-thread counter), warms the chain up once and
then requires **zero** allocations across 64 blocks. A companion test
(`counting_allocator_actually_sees_allocations`) proves the probe is wired up,
so a silently-broken hook cannot make the first test pass by accident.

```sh
cargo test -p tpt-dsp-wasm
```

Exceptions, stated plainly:

- `Pedalboard::process(&[f32]) -> Vec<f32>` allocates its return buffer. It is
  the offline/testing path, not the callback path.
- `set_eq_gain` rebuilds the biquad cascade (`tpt-dsp-audio`'s `Eq` has no
  in-place coefficient update), which allocates and resets filter state. Call
  it from the worklet's message handler, never from `process()`.
- `render_demo` allocates freely; it is an offline helper.

For JS the zero-copy route is `input_ptr()` / `output_ptr()` +
`process_internal_block()`: both buffers live in wasm linear memory and are
viewed directly as `Float32Array`s, so a render quantum costs two `set()` copies
and no boundary marshalling.

## Build

```sh
# from this directory — writes into www/pkg, which is what www/ imports
wasm-pack build tpt-dsp-wasm --target web --out-dir ../www/pkg
```

That produces `www/pkg/tpt_dsp_wasm.js` + `www/pkg/tpt_dsp_wasm_bg.wasm`, which
is exactly what `www/main.js` and `www/pedal-processor.js` import. Plain cargo
also works if you only want to type-check the wasm target:

```sh
cargo build -p tpt-dsp-wasm                              # host build (tests, CI)
cargo build -p tpt-dsp-wasm --target wasm32-unknown-unknown
rustup target add wasm32-unknown-unknown                 # if the target is missing
```

Optional feature `async` pulls in `wasm-bindgen-futures` and adds
`open_microphone()`, an `async` `getUserMedia` wrapper (with echo cancellation,
noise suppression and AGC disabled — all three destroy a guitar signal):

```sh
wasm-pack build --target web -- --features async
```

## Run

`www/main.js` and `www/pedal-processor.js` resolve `./pkg/tpt_dsp_wasm.*`
relative to `www/`, so serve the **`www/` directory** (not the crate root):

```sh
python -m http.server 8080 --directory www   # or: npx serve www
# open http://localhost:8080
```

`localhost` counts as a secure context, so `getUserMedia` works without TLS.
Click **Start**, allow microphone access, and use headphones — an open speaker
plus a live mic through a distortion pedal is a feedback loop.

## How the pieces fit

```text
main thread                          audio thread (AudioWorkletGlobalScope)
───────────                          ──────────────────────────────────────
main.js
  init(wasm)                         pedal-processor.js
  register_worklet(ctx, url)  ─────► initSync({ module })
  WebAssembly.compileStreaming ────► Pedalboard.with_sample_rate(sampleRate)
  create_pedal_node(ctx, {module})   Float32Array view over input_ptr()/output_ptr()
  connect_stream(ctx, mic, node)     process(): copy 128 in →
  slider → node.port.postMessage ──► set_*()      process_internal_block() → copy 128 out
```

The worklet needs its own instance of the module (it runs in a separate realm),
so the main thread compiles a `WebAssembly.Module` once and passes it through
`processorOptions`.

Files in `www/`:

| File                  | Role                                                        |
| --------------------- | ------------------------------------------------------------ |
| `index.html`          | Controls for every pedal parameter.                            |
| `main.js`             | Main-thread setup: wasm init, worklet load, mic → node → out.  |
| `pedal-processor.js`  | The `AudioWorkletProcessor` that runs the DSP.                 |

Browser support notes:

- `www/pedal-processor.js` uses a static `import` inside the worklet module,
  which Chromium-based browsers support. If your target browser rejects module
  imports in worklets, bundle `pkg/tpt_dsp_wasm.js` into the processor script
  instead of importing it.
- There is deliberately no `ScriptProcessorNode` fallback: it is deprecated and
  runs on the main thread, so it cannot honour the allocation-free 128-sample
  contract. If you need one anyway, call `process_block(input, output)` from
  `onaudioprocess` and accept the latency and jitter.

## Known limitations

- **Mono.** The chain processes channel 0 and copies it to every output
  channel.
- **Short reverb.** `ConvolutionReverb` is built with a 128-sample block, and
  `tpt-dsp-core`'s `FftConvolver` truncates its kernel to the FFT length and
  carries only one block of overlap-add tail. The impulse response is therefore
  capped at the block size (~2.7 ms at 48 kHz), which is an ambience/early
  reflection, not a hall. A partitioned convolution in `tpt-dsp-core` would lift
  this without raising latency.
- **Parameter changes are stepped**, not smoothed; large jumps in drive or EQ
  gain can click.
