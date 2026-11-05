"""Generate golden reference vectors for tpt-dsp-core with scipy.

Run from this directory:

    python generate.py

It writes ``windows.csv`` and ``filters.csv`` next to this script. The Rust
test ``tests/golden.rs`` reads them. The vectors are generated, not
hand-written, so they are an independent check of the Rust implementation.

Conventions matched to tpt-dsp:
- Windows use the *symmetric* form (``fftbins=False``).
- Filter magnitudes are reported relative to their DC gain, so the Chebyshev I
  even-order DC ripple normalisation matches the Rust design.
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))


def write_windows():
    specs = [
        ("hann", "hann", None),
        ("hamming", "hamming", None),
        ("blackman", "blackman", None),
        ("flattop", "flattop", None),
        ("kaiser8", ("kaiser", 8.0), None),
        ("tukey0.5", ("tukey", 0.5), None),
        ("gaussian0.3", None, None),
    ]
    rows = []
    for n in (16, 31, 64):
        for name, spec, _ in specs:
            if name == "gaussian0.3":
                # scipy's gaussian takes the std in samples: sigma * (n-1)/2.
                w = signal.windows.gaussian(n, std=0.3 * (n - 1) / 2, sym=True)
            else:
                w = signal.get_window(spec, n, fftbins=False)
            for i, v in enumerate(w):
                rows.append((name, n, i, repr(float(v))))
    with open(os.path.join(HERE, "windows.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["window", "n", "index", "value"])
        out.writerows(rows)
    print(f"windows.csv: {len(rows)} rows")


def magnitude_db(sos, fs, freqs):
    _, h = signal.sosfreqz(sos, worN=freqs, fs=fs)
    return 20 * np.log10(np.maximum(np.abs(h), 1e-12))


def write_filters():
    fs = 48_000.0
    freqs = np.array([50.0, 200.0, 500.0, 800.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 8000.0])
    rows = []
    for order in (2, 3, 4, 5):
        # Butterworth low-pass at 1 kHz.
        sos = signal.butter(order, 1000.0, btype="low", fs=fs, output="sos")
        dc = magnitude_db(sos, fs, np.array([0.0]))[0]
        for f, db in zip(freqs, magnitude_db(sos, fs, freqs)):
            rows.append(("butter", order, fs, 1000.0, 0.0, f, db - dc))

        # Chebyshev I, 1 dB ripple, cutoff 1 kHz.
        sos = signal.cheby1(order, 1.0, 1000.0, btype="low", fs=fs, output="sos")
        dc = magnitude_db(sos, fs, np.array([0.0]))[0]
        for f, db in zip(freqs, magnitude_db(sos, fs, freqs)):
            rows.append(("cheby1", order, fs, 1000.0, 1.0, f, db - dc))

        # Chebyshev II, 40 dB stopband attenuation, stopband edge 3 kHz.
        sos = signal.cheby2(order, 40.0, 3000.0, btype="low", fs=fs, output="sos")
        dc = magnitude_db(sos, fs, np.array([0.0]))[0]
        for f, db in zip(freqs, magnitude_db(sos, fs, freqs)):
            rows.append(("cheby2", order, fs, 3000.0, 40.0, f, db - dc))

    with open(os.path.join(HERE, "filters.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["kind", "order", "fs", "fc", "param", "freq_hz", "mag_db_rel_dc"])
        for r in rows:
            out.writerow([r[0], r[1], r[2], r[3], r[4], r[5], repr(float(r[6]))])
    print(f"filters.csv: {len(rows)} rows")


if __name__ == "__main__":
    write_windows()
    write_filters()
