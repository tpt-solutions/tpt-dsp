# tpt-dsp-cli

A command-line DSP pipeline built on the `tpt-dsp` crates.

```text
tpt-dsp-cli filter   --input in.wav --output out.wav --effect biquad:lowpass:1000 --effect delay:0.25
tpt-dsp-cli demod    --input iq.u8 --output audio.wav --format u8 --iq-rate 2400000
tpt-dsp-cli spectrum --input in.wav --fft-size 2048 --csv spectrum.csv --top 10
tpt-dsp-cli info     --input iq.u8 --format i16le
```

## Subcommands

- **`filter`** — apply a chain of effects to a WAV file (one chain per channel).
  Effects wrap the `tpt-dsp-audio` effects:
  - `biquad:<type>:<freq>[:q[:gain_db]]` — `type` is `lowpass`, `highpass`,
    `bandpass`, `notch`, `allpass`, `peaking`, `lowshelf`, `highshelf`.
  - `waveshaper:<curve>:<drive>[:mix]` — `curve` is `tanh`, `hardclip`, `cubic`
    or `poly:c0,c1,c2,c3`.
  - `delay:<seconds>[:feedback[:mix]]`
  - `reverb:<seconds>[:wet]`
  - `eq:<f0>,<g0>,<q0>;[<f1>,<g1>,<q1>;…]`
- **`demod`** — FM-demodulate a raw IQ file (8/16-bit integer, 32-bit float,
  little/big-endian) into a mono WAV file.
- **`spectrum`** — averaged magnitude spectrum plus time-domain features
  (dominant frequency, peak dB, RMS, zero-crossing rate, spectral centroid, top
  peaks); optionally dumps the spectrum to CSV.
- **`info`** — print WAV header metadata or IQ file size / format.

WAV I/O uses `hound`; IQ parsing uses `tpt-dsp-io`.

## Build

This crate is a member of the `tpt-dsp` workspace:

```sh
cargo build -p tpt-dsp-cli
cargo run   -p tpt-dsp-cli -- spectrum --input signal.wav
```

Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
