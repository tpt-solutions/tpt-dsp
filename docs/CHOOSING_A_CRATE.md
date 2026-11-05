# Choosing a crate

`tpt-dsp` is one framework split into focused crates. Most users want the
facade and nothing else.

| You want to… | Use | Notes |
|--------------|-----|-------|
| Build an audio effect or instrument | `tpt-dsp` with `audio` (default) | Effects, synthesis, dynamics, oversampling. |
| Analyse a signal (spectrum, pitch, MFCC, loudness) | `tpt-dsp` with `analysis` (default) | Frame-based and offline functions. |
| Run a PID loop or shape a motion profile | `tpt-dsp` with `control` (default) | Also `TrapezoidalProfile` and `InputShaper`. |
| Read or write WAV files, or stream IQ | `tpt-dsp` with `io` | Native audio I/O is behind `io`'s `audio` feature. |
| Use the math only, in `no_std` firmware | `tpt-dsp-core` with `default-features = false` | Verified for `thumbv7em-none-eabihf`. |
| Check your own code is allocation-free | `tpt-dsp-testkit` (dev-dependency) | See `docs/RT_SAFETY.md`. |
| Process files from a terminal | `tpt-dsp-cli` (binary) | WAV and IQ filter, spectrum and analysis subcommands. |
| Run a live spectrum viewer | `tpt-dsp-viz` | egui desktop app; needs a display. |
| Run a browser pedalboard | `tpt-dsp-wasm` | Built with `wasm-pack`; see `www/`. |
| Use from Python | `tpt-dsp-py` (standalone) | Not in the default workspace. |
| Build a CLAP/VST3 plugin | `tpt-dsp-nihplug` (standalone) | Not in the default workspace. |

## Choosing between the common building blocks

- **Filter.** A single biquad (`Biquad`) for simple EQ. Use `butterworth` or
  `chebyshev1_lowpass` / `chebyshev2_lowpass` for steeper cascades. Use
  `Svf` when the cutoff moves every sample.
- **FIR.** `FirDesign` (Blackman by default, or any window with
  `try_design_windowed`). Use `halfband_fir` for decimation by two.
- **Resampling.** `RationalResampler` for any ratio L/M. `CicDecimator` for a
  large integer decimation with no multipliers. `FarrowDelay` for a fractional
  delay of a few samples.
- **Pitch.** `yin_pitch` for a monophonic fundamental. `Goertzel` if you only
  need a few known tones.
- **Reverb.** `Freeverb` for a quick algorithmic room (mono). `ConvolutionReverb`
  when you have a measured impulse response.
- **Demodulation.** `FmDemodulator`, `AmDemodulator`, `SsbDemodulator`.
  `Pll` for carrier tracking, `Squelch` to mute noise.
