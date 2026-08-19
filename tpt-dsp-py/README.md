# tpt-dsp-py

Python bindings for the `tpt-dsp` framework, built with
[pyo3](https://pyo3.rs). The extension module is named `tpt_dsp`.

```python
import math, tpt_dsp

sr = 48_000
sig = [math.sin(2 * math.pi * 1000 * i / sr) for i in range(1024)]

tpt_dsp.rms(sig)                       # RMS energy
tpt_dsp.zero_crossing_rate(sig)        # fraction of sign changes
tpt_dsp.spectral_centroid(sig, sr)     # brightness in Hz
freqs, db = tpt_dsp.spectrum(sig, sr, 1024, "hann")
tpt_dsp.analyze(sig, sr, 1024, "hann") # summary dict

# FM demodulate interleaved I/Q into audio
audio = tpt_dsp.fm_demod(i_samples, q_samples, iq_rate, deviation)
```

All functions operate on plain Python lists of `float`s (or, for `fm_demod`, two
parallel lists of in-phase / quadrature samples) and return Python lists — wrap
them with `numpy.array(...)` for heavier numerical work.

## Build

This crate is **intentionally excluded** from the top-level `tpt-dsp` workspace
(see the root `Cargo.toml` `exclude` list) because it links Python and cannot be
built or tested by the normal `cargo build/test --workspace` gate. Build it as a
standalone workspace:

```sh
cd tpt-dsp-py
cargo build --release
# On Windows the cdylib is built as `target/release/tpt_dsp.dll`;
# copy/rename it to `tpt_dsp.pyd` to import it from Python.
```

It is compiled with the `extension-module` and `abi3-py38` features, so it is a
proper importable extension module that resolves the Python C-API at load time
and builds without a Python interpreter present.

Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
