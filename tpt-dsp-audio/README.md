# tpt-dsp-audio

> Synthesis, effects and real-time audio graphs for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-audio` builds on [`tpt-dsp-core`](https://github.com/TPT-Solutions/tpt-dsp/tree/master/tpt-dsp-core) to provide the pieces
you need for a browser or desktop audio engine: oscillators and synthesis
voices, a set of classic effects, and an `AudioGraph` abstraction driven by an
allocation-free real-time engine.

All hot-path processing operates on caller-supplied or pre-allocated buffers and
never allocates, so it is safe to call from an audio callback.

## What's inside

### Oscillators & synthesis

- [`Oscillator`] — a free-running oscillator with selectable [`Waveform`]
  (`Sine`, `Sawtooth`, `Square`, …).
- [`Wavetable`] — band-limited wavetable oscillator.
- [`FmSynth`] — 2-operator FM synthesis voice.
- [`SubtractiveVoice`] — a subtractive-synthesis voice (oscillator → filter → amp).
- [`Adsr`] and [`EnvelopeState`] — an attack/decay/sustain/release envelope generator.
- [`PolyBlepOscillator`] — a band-limited oscillator (with [`BlepShape`]) that uses the
  `poly_blep` correction to suppress aliasing on saw and square waves.
- [`NoiseSource`], [`PinkNoise`] and [`TpdfDither`] — white and pink noise, and
  triangular-PDF dither for quantising to a chosen bit depth.

### Effects

- [`Waveshaper`] — waveshaping distortion with a configurable [`Curve`]
  (`Tanh`, `HardClip`, `Cubic`, `Polynomial`). Run it inside an [`Oversampler`] to
  reduce aliasing.
- [`Delay`] — a delay line with feedback and wet/dry mix, and fractional, glided
  delay times (`set_delay_fractional`).
- [`ConvolutionReverb`] — FFT convolution reverb for an impulse response of any
  length; build one with the `generate_decay_ir` helper.
- [`Freeverb`] and [`StereoFreeverb`] — algorithmic reverb with room size, damping and
  wet level. The stereo version spreads its right channel for width.
- [`Eq`] — a multi-band peaking equaliser (`try_new` for fallible construction).
- [`Oversampler`] — runs a nonlinear closure at a higher rate and decimates back, with
  a reported latency.

### Dynamics

- [`Compressor`] — feed-forward compressor (threshold, ratio, attack, release) with
  makeup gain and gain-reduction metering.
- [`Limiter`] — a ceiling and release time. It has no lookahead, so a fast transient
  can cause an audible gain step.
- [`Gate`] — noise gate (threshold, ratio, attack, release) that reports its current
  gain with `gain_db`.

All three share one envelope-follower structure and allocate nothing in `tick` or
`process`.

### Modulation

- [`Lfo`] — low-frequency oscillator with a selectable [`Waveform`], unipolar mode and
  rate changes.
- [`Tremolo`] — amplitude modulation by an `Lfo`-controlled gain.
- [`Chorus`], [`Flanger`] and [`Phaser`] — modulation effects with a mix control.

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
    AudioGraph, ClosureNode, ClosureSink, ClosureSource, Oscillator, Waveform,
};

// A 220 Hz sine source, a gain node, and a sink that forwards to a DAC.
let mut osc = Oscillator::with_waveform(48_000.0, 220.0, Waveform::Sine);
let mut graph = AudioGraph::new(
    128,
    Box::new(ClosureSource(move |out: &mut [f32]| {
        osc.process(out);
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
use tpt_dsp_audio::{generate_decay_ir, ConvolutionReverb};

let sample_rate = 48_000.0;
let length = (1.5 * sample_rate) as usize; // impulse response length in samples
let ir = generate_decay_ir(length, sample_rate, 2.0); // length, sample_rate, decay seconds
let mut reverb = ConvolutionReverb::new(&ir, 128);
let mut wet = [0.0f32; 128];
reverb.process(&input_block, &mut wet);
```

## Platform

`tpt-dsp-audio` is a `std` crate: it uses the default features of
`tpt-dsp-core`. The hot-path processing functions do not allocate, so they are
safe to call from an audio callback once the objects are constructed.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
