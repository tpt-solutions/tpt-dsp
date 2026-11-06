# tpt-dsp (Python)

Python bindings for the `tpt-dsp` framework, built with
[pyo3](https://pyo3.rs). The package is `tpt_dsp`; the compiled module is
`tpt_dsp._native`, which the package re-exports, and the package ships type
stubs (`py.typed`).

```python
import math
import tpt_dsp

sr = 48_000
sig = [math.sin(2 * math.pi * 1000 * i / sr) for i in range(1024)]

tpt_dsp.rms(sig)                       # RMS energy
tpt_dsp.zero_crossing_rate(sig)        # fraction of sign changes
tpt_dsp.spectral_centroid(sig, sr)     # brightness in Hz
freqs, db = tpt_dsp.spectrum(sig, sr, 1024, "hann")
tpt_dsp.analyze(sig, sr, 1024, "hann") # summary dict

# FM demodulate interleaved I/Q into audio
audio = tpt_dsp.fm_demod(i_samples, q_samples, iq_rate, deviation)

# Stateful filter and controller
lp = tpt_dsp.Biquad("lowpass", sr, 1000.0)     # kinds: lowpass, highpass,
filtered = lp.process(sig)                     # bandpass, notch, allpass,
                                               # peaking, lowshelf, highshelf
pid = tpt_dsp.Pid(2.0, 1.0, 0.0, 0.01)
pid.set_setpoint(10.0)
u = pid.update(measured)
```

All functions take a list of `float`s, or a NumPy array, and return lists. Wrap
the results with `numpy.array(...)` for heavier numerical work. With the default
build a NumPy array is converted to a list first. The `zero-copy` build reads a
1-D `float32` array in place, which is about 12x faster than passing a list
(measured on 5 million samples).

## Build a wheel

This crate is a **standalone workspace**, excluded from the top-level `tpt-dsp`
workspace, because it links Python. Build it with
[maturin](https://www.maturin.rs):

```sh
cd tpt-dsp-py
maturin build --release        # a wheel in target/wheels/
pip install target/wheels/tpt_dsp-*.whl
python -m unittest discover -s tests
```

The default wheel is built with `extension-module` and `abi3-py39`, so one
wheel covers Python 3.9 and later. The buffer protocol that zero-copy input
needs is not part of the limited API, so the `zero-copy` feature builds one
wheel per Python version instead:

```sh
maturin build --release --no-default-features --features zero-copy
```

Publishing to PyPI is not covered here.

Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.
