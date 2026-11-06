# tpt-dsp-viz

> Real-time waterfall spectrum and live spectrum-line desktop UI for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-viz` is the desktop visualization front end of the framework. It builds
on [`tpt-dsp-core`](https://github.com/TPT-Solutions/tpt-dsp/tree/master/tpt-dsp-core) (math + FFT) and
[`tpt-dsp-analysis`](https://github.com/TPT-Solutions/tpt-dsp/tree/master/tpt-dsp-analysis) (spectrum estimation, peak finding,
spectrogram) to render live signal displays:

- **Waterfall spectrogram** — a scrolling heat-map (time on one axis, frequency
  on the other, intensity as colour) produced by `tpt-dsp-analysis`'s
  [`Spectrogram`].
- **Live spectrum line** — a real-time line plot of the current magnitude
  spectrum with dB scaling, driven by [`SpectrumAnalyzer`]/[`RealtimeSpectrumAnalyzer`].
- **Status bar** — source label, peak frequency and level, and a pause/resume button.

The rendering is in-tree. A small software canvas draws into a `0x00RRGGBB`
pixel buffer, using a built-in 5×7 bitmap font. The only third-party dependency
for the window is [`minifb`](https://crates.io/crates/minifb) (MIT OR Apache-2.0),
which opens the native window and handles input and presentation. There is no
GUI toolkit (no `egui`/`eframe`). The crate is built on `crossbeam-channel` so a
capture/producer thread can stream blocks to the UI thread without blocking the
render loop.

## Features

| Feature  | Default | Description                                          |
| -------- | ------- | ---------------------------------------------------- |
| `audio`  | ✗       | Live capture from the system audio device via the built-in audio backend.  |

With the default features the crate exposes the visualization primitives; enable
`audio` to pull samples directly from a microphone/loopback device.

## Using it as a library

[`run`] takes a [`Source`] and blocks until the window closes:

```rust
use tpt_dsp_viz::{run, Source};

fn main() -> tpt_dsp_viz::Result {
    run(Source::Synthetic)
}
```

For a custom producer, build a [`VizApp`] from a `crossbeam-channel` receiver of
[`SpectrumFrame`]s and hand it to [`run_app`]. Your own producer thread can use
[`run_synthetic`] or [`run_audio_input`] (feature `audio`), or your own loop of
[`analyze_block`] calls. See `examples/custom_waterfall.rs`.

To embed the display in another loop, call [`VizApp::update`] once per tick,
forward clicks to [`VizApp::on_click`], and call [`VizApp::draw_into`] with your
own pixel buffer.

To render a frame to a PNG without opening a window (useful on headless
machines):

```sh
cargo run -p tpt-dsp-viz --example snapshot -- out.png 1024 720
```

## Controls

- Click **Pause** / **Resume**, or press `P` or Space, to freeze the display.
- Press Escape or close the window to exit.

## Architecture

```text
   producer thread                 UI thread (minifb window)
   ───────────────                ─────────────────────────
   capture / generate
        │  crossbeam channel
        ▼
   tpt-dsp-analysis (FFT, spectrogram)
        │
        ▼
   VizApp::update ── software canvas (colormap, lines, bitmap font)
                          │
                          ▼
                    minifb presents the pixel buffer
```

Because all DSP runs on `tpt-dsp-core`/`tpt-dsp-analysis`, the visualization is
allocation-light and reuses the same transforms used in the audio, RF and
control crates — what you see is what the rest of the framework computes.

## Status

This crate is **implemented**: `cargo run -p tpt-dsp-viz` opens a window with a
scrolling waterfall spectrogram (colour-mapped black → blue → cyan → yellow →
red) and a live dB spectrum line, with a peak-frequency/dB readout and a pause
toggle. With no hardware attached it renders a deterministic synthetic
multi-tone + noise signal; pass `--features audio -- --audio` to capture from
the default system audio input device via the built-in backend instead.

## Building

```sh
cargo build -p tpt-dsp-viz            # default features
cargo build -p tpt-dsp-viz --features audio
```

The crate uses the workspace's Rust 1.85 minimum. A window needs a display, so
the window loop is not exercised in headless CI. The rendering, layout and input
logic are covered by unit tests, and the snapshot example checks the output
pixels.

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
