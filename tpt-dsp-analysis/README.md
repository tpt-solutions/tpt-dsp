# tpt-dsp-analysis

> Spectrum analysis, time-series statistics and feature extraction for the
> [tpt-dsp](https://github.com/tpt-solutions/tpt-dsp) framework.

`tpt-dsp-analysis` builds on [`tpt-dsp-core`](../tpt-dsp-core) to turn raw
samples into insight: spectrum estimation, peak finding, time-series smoothing,
and a set of audio/RF feature descriptors. It is aimed at telemetry, RF/SDR and
biomedical streams, but the algorithms are general-purpose.

Everything is re-exported at the crate root, so `use tpt_dsp_analysis::Ema;`
works without naming the internal modules.

## What's inside

### Time series

- [`MovingAverage`] — average over a fixed-length window.
- [`RunningMean`] — cumulative mean of an unbounded stream, using Welford's update.
- [`Ema`] — exponential moving average.
- [`OutlierDetector`] — rolling-threshold outlier detection.

### Features

- [`rms`] — root-mean-square energy.
- [`zero_crossing_rate`] — rate at which a signal crosses zero.
- [`spectral_centroid`], [`spectral_centroid_normalized`] — "brightness" of a
  spectrum.

### Spectrum

- [`SpectrumAnalyzer`] — FFT analysis of successive frames, with the spectrum
  smoothed by an averaging coefficient.
- [`RealtimeSpectrumAnalyzer`] / [`SpectrumConfig`] — a configured analyser for live
  displays: window, dB reference and floor ([`DEFAULT_DB_FLOOR`]), and averaging.
  [`Averaging`] has `None`, `Exponential`, `TimeConstant` and `Linear` modes.
- [`find_peaks`] — indices of local maxima above a threshold; [`peak_bin`] — the
  strongest bin.
- [`SpectralPeak`] with [`parabolic_interpolate`] — sub-bin frequency and level,
  via `RealtimeSpectrumAnalyzer::peak_at`.
- [`dominant_frequency`] — the frequency of the strongest component.
- [`linear_to_db`] / [`db_to_linear`] — magnitude ↔ decibel conversion.

### Spectrogram

- [`Spectrogram`] — accumulates successive spectrum frames into a waterfall
  image (row = time, column = frequency).

### Async adapters

Runtime-agnostic streaming adapters that process channels and streams. They are
only compiled when an async feature is enabled:

- `process_stream_in_place` and `process_stream_into_sink`, built on `futures`
  `Stream`/`Sink`.
- With `async-tokio`: `process_channel`, `process_channel_in_place`,
  `process_stream` (at the crate root, and in `async_adapters::tokio`).
- With `async-std`: the equivalent adapters under `async_adapters::async_std`.

## Features

| Feature        | Default | Description                                                                 |
| -------------- | ------- | --------------------------------------------------------------------------- |
| `async-tokio`  | ✗       | tokio channel adapters, plus the futures `Stream`/`Sink` adapters.          |
| `async`        | ✗       | Shorthand for `async-tokio`.                                                |
| `async-std`    | ✗       | async-std channel adapters, plus the futures `Stream`/`Sink` adapters.      |

The default build has **no async runtime and no futures dependency**. Both runtime
features may be enabled at once.

## Examples

### Smooth a noisy signal and find its dominant frequency

```rust
use tpt_dsp_analysis::{dominant_frequency, Ema, RealtimeSpectrumAnalyzer, SpectrumConfig};

let mut ema = Ema::new(0.1);
let smoothed: Vec<f32> = signal.iter().map(|&x| ema.push(x)).collect();

let config = SpectrumConfig {
    fft_size: 1024,
    sample_rate: 48_000.0,
    ..Default::default()
};
let mut analyzer = RealtimeSpectrumAnalyzer::new(config);
analyzer.process(&smoothed[..1024]);

let f0 = dominant_frequency(analyzer.magnitude(), 48_000.0, 1024);
```

### Detect peaks in a live spectrum

```rust
use tpt_dsp_analysis::{find_peaks, SpectralPeak};

let peaks: Vec<SpectralPeak> = find_peaks(analyzer.magnitude(), 0.1)
    .into_iter()
    .map(|bin| analyzer.peak_at(bin))
    .collect();
for p in &peaks {
    println!("peak at {:.1} Hz, {:.1} dB", p.frequency, p.magnitude_db);
}
```

## License

Dual licensed under **MIT** and **Apache-2.0**. © TPT Solutions.
