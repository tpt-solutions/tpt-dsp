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
  little/big-endian) into a mono WAV file. `--decimate <n>` reduces the output
  rate by an integer factor.
- **`spectrum`** — averaged magnitude spectrum plus time-domain features
  (dominant frequency, peak dB, RMS, zero-crossing rate, spectral centroid, top
  peaks). `--fft-size` and `--window` (default Hann) set the analysis; `--csv`
  dumps the averaged spectrum.
- **`info`** — print WAV header metadata or IQ file size / format.

WAV I/O uses `tpt-dsp-io`'s built-in RIFF/WAVE module; IQ parsing uses `tpt-dsp-io`.

## Library

The subcommands are thin wrappers over public functions, so the same steps can be
used from Rust: `read_wav` / `write_wav`, `Effect::parse` and `EffectChain`,
`read_iq` / `demod_iq`, `analyze_real` / `analyze_complex`, and `write_spectrum_csv`.

## Build

This crate is a member of the `tpt-dsp` workspace:

```sh
cargo build -p tpt-dsp-cli
cargo run   -p tpt-dsp-cli -- spectrum --input signal.wav
```

Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
