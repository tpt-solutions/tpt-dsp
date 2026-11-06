# Changelog

All notable changes to `tpt-dsp-py` (the Python package `tpt-dsp`) are documented in
this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

For the whole-workspace history see the [root `CHANGELOG.md`](../CHANGELOG.md).
This crate is excluded from the workspace build and is not published to PyPI or
crates.io yet (see [`README.md`](README.md#build-a-wheel)).

## [Unreleased]

### Added
- Python package `tpt_dsp`, built with pyo3 and maturin. The compiled module is
  `tpt_dsp._native`, and the package re-exports it and ships `py.typed` and type
  stubs (`_native.pyi`).
- Analysis functions: `rms`, `zero_crossing_rate`, `spectral_centroid`, `spectrum`
  (averaged magnitude spectrum with a Hann, Hamming or Blackman window), `analyze`
  (summary dict) and `fm_demod` (I/Q to audio).
- Stateful classes: `Biquad` (RBJ kinds, `process` keeps state between calls), `Pid`
  (setpoint, output limits, `update`, `reset`) and `Oscillator` (sine, sawtooth,
  square and triangle, with phase kept between calls).
- `zero-copy` feature: a 1-D `float32` NumPy array is read in place, which is about
  12x faster than a list on 5 million samples. It builds a per-version wheel, because
  the buffer protocol is not part of the stable ABI.
- Default build is one `abi3-py39` wheel, so it covers Python 3.9 and later.
- Python tests for the bindings, for the NumPy interop, and for the array inputs. CI
  builds the wheel and runs these tests against NumPy.
