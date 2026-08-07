# tpt-dsp-audio

> Synthesis, effects and real-time audio graphs for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-audio` builds on [`tpt-dsp-core`](../tpt-dsp-core) to provide the pieces
you need for a browser or desktop audio engine: oscillators and synthesis
voices, a set of classic effects, and an `AudioGraph` abstraction driven by an
allocation-free real-time engine.

All hot-path processing operates on caller-supplied or pre-allocated buffers and
never allocates, so it is safe to call from an audio callback.

## What's inside

### Oscillators & synthesis

- [`Oscillator`] — a free-running oscillator with selectable [`Waveform`]
  (sine, saw, square, triangle, …).
- [`Wavetable`] — band-limited wavetable oscillator.
- [`FmSynth`] — 2-operator FM synthesis voice.
- [`SubtractiveVoice`] — a subtractive-synthesis voice (oscillator → filter → amp).

### Effects

- [`Waveshaper`] — waveshaping distortion with a configurable [`Curve`].
- [`Delay`] — a delay line with feedback and wet/dry mix.
- [`ConvolutionReverb`] — FFT convolution reverb; build an impulse response with
  the `generate_decay_ir` helper.
- [`Eq`] — a multi-band peaking/tone equaliser.

### Graph & engine

- [`AudioGraph`] — a sources → nodes → sinks pipeline. Ready-made closures
  ([`ClosureSource`], [`ClosureNode`], [`ClosureSink`]), a [`Passthrough`] node,
  and the [`Source`]/[`Sink`]/[`AudioNode`] traits to implement your own.
- [`RealtimeEngine`] — drives a per-block DSP callback with fixed,
  allocation-free block processing. Block sizes are exposed as the `BLOCK_128`
  and `BLOCK_256` constants.

## Examples

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

### Design a reverb impulse response

```rust
use tpt_dsp_audio::{reverb::generate_decay_ir, ConvolutionReverb};

let ir = generate_decay_ir(48_000.0, 1.5, 2.0); // sample_rate, length_s, decay
let mut reverb = ConvolutionReverb::new(ir, 128);
let mut wet = [0.0f32; 128];
reverb.process(&input_block, &mut wet);
```

## `no_std`

`tpt-dsp-audio` reuses the allocation-free core primitives; the synthesis,
effects and graph code is written to stay allocation-free on the hot path.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
