# tpt-dsp-analysis

> Spectrum analysis, time-series statistics and feature extraction for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-analysis` builds on [`tpt-dsp-core`](../tpt-dsp-core) to turn raw
samples into insight: spectrum estimation, peak finding, time-series smoothing,
and a set of audio/RF feature descriptors. It is aimed at telemetry, RF/SDR and
biomedical streams, but the algorithms are general-purpose.

## What's inside

### Time series — [`timeseries`]

- [`MovingAverage`] — windowed average.
- [`RunningMean`] — mean computed incrementally over a sliding window.
- [`Ema`] — exponential moving average.
- [`OutlierDetector`] — rolling-threshold outlier detection.

### Features — [`features`]

- [`rms`] — root-mean-square energy.
- [`zero_crossing_rate`] — rate at which a signal crosses zero.
- [`spectral_centroid`], [`spectral_centroid_normalized`] — "brightness" of a
  spectrum.

### Spectrum — [`spectrum`]

- [`SpectrumAnalyzer`] / [`SpectrumConfig`] — windowed-FFT analysis with dB
  scaling and a configurable floor ([`DEFAULT_DB_FLOOR`]).
- [`RealtimeSpectrumAnalyzer`] with [`Averaging`] (peak / exponential / windowed)
  for live displays.
- [`find_peaks`] — local maxima with parabolic interpolation
  ([`parabolic_interpolate`], [`SpectralPeak`], [`peak_bin`]).
- [`dominant_frequency`] — the strongest frequency component.
- [`linear_to_db`] / [`db_to_linear`] — magnitude ↔ decibel conversion.

### Spectrogram — [`spectrogram`]

- [`Spectrogram`] — accumulates successive spectrum frames into a waterfall
  image (row = time, column = frequency).

### Async adapters — [`async_adapters`]

Runtime-agnostic streaming adapters that process channels/streams in place:

- `process_stream_in_place`, `process_stream_into_sink` (futures `Stream`/`Sink`).
- tokio: `process_channel`, `process_channel_in_place`, `process_stream`.
- async-std: equivalent adapters under `async_adapters::async_std`.

## Features

| Feature        | Default | Description                                                                 |
| -------------- | ------- | --------------------------------------------------------------------------- |
| `async`        | ✓       | Shorthand for `async-tokio`.                                                |
| `async-tokio`  | (via async) | tokio channel + futures `Stream`/`Sink` adapters.                        |
| `async-std`    | ✗       | async-std channel adapters.                                                 |

Both runtime features may be enabled at once. With `--no-default-features` no
async runtime, executor or futures dependency is compiled in.

## Examples

### Smooth a noisy signal and find its dominant frequency

```rust
use tpt_dsp_analysis::{timeseries::Ema, spectrum::{SpectrumAnalyzer, SpectrumConfig, dominant_frequency}};

let mut ema = Ema::new(0.1);
let smoothed: Vec<f32> = signal.iter().map(|&x| ema.update(x)).collect();

let cfg = SpectrumConfig::default();
let analyzer = SpectrumAnalyzer::new(cfg, 48_000.0);
let mut mag = vec![0.0f32; 1024];
analyzer.analyze(&smoothed, &mut mag);
let f0 = dominant_frequency(&mag, 48_000.0);
```

### Detect peaks in a live spectrum

```rust
use tpt_dsp_analysis::spectrum::{find_peaks, SpectralPeak};

let peaks: Vec<SpectralPeak> = find_peaks(&magnitude, 48_000.0, 3, -60.0);
for p in &peaks {
    println!("peak at {:.1} Hz, {:.1} dB", p.frequency, p.magnitude_db);
}
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
