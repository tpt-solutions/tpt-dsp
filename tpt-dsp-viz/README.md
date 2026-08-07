# tpt-dsp-viz

> Real-time waterfall spectrum and live spectrum-line desktop UI for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-viz` is the desktop visualization front end of the framework. It builds
on [`tpt-dsp-core`](../tpt-dsp-core) (math + FFT) and
[`tpt-dsp-analysis`](../tpt-dsp-analysis) (spectrum estimation, peak finding,
spectrogram) to render live signal displays with [`egui`]/[`eframe`]:

- **Waterfall spectrogram** — a scrolling heat-map (time on one axis, frequency
  on the other, intensity as colour) produced by `tpt-dsp-analysis`'s
  [`Spectrogram`].
- **Live spectrum line** — a real-time line plot of the current magnitude
  spectrum with dB scaling and configurable floor, driven by
  [`SpectrumAnalyzer`]/[`RealtimeSpectrumAnalyzer`].

The crate is built on `crossbeam-channel` so a capture/producer thread can stream
blocks to the UI thread without blocking the render loop.

## Features

| Feature  | Default | Description                                          |
| -------- | ------- | ---------------------------------------------------- |
| `audio`  | ✗       | Live capture from a system audio device via `cpal`.  |

With the default features the crate exposes the visualization primitives; enable
`audio` to pull samples directly from a microphone/loopback device.

## Architecture

```text
   producer thread                 UI thread (eframe / egui)
   ───────────────                ─────────────────────────
   capture / generate
        │  crossbeam channel
        ▼
   tpt-dsp-analysis (FFT, spectrogram)
        │
        ▼
   egui widgets (waterfall texture, spectrum line)
```

Because all DSP runs on `tpt-dsp-core`/`tpt-dsp-analysis`, the visualization is
allocation-light and reuses the same transforms used in the audio, RF and
control crates — what you see is what the rest of the framework computes.

## Status

This crate is **early-stage**: the rendering pipeline and egui widgets are being
assembled. The public API is not yet stable; expect the module and type names to
change between `0.1.x` releases. The core dependency stack
(`tpt-dsp-core` → `tpt-dsp-analysis` → `egui`/`eframe`) is fixed.

## Building

```sh
cargo build -p tpt-dsp-viz            # default features
cargo build -p tpt-dsp-viz --features audio
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
