"""Reference magnitudes for Chebyshev type II band-pass and band-stop filters.

Run from this directory:  python generate_cheby2_band.py  -> cheby2_band.csv
Band-pass gains are relative to the digital centre frequency, band-stop gains
are relative to DC, where the passband sits.
"""

import csv
import math
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FL, FH = 1000.0, 3000.0
ATTEN = 40.0
FREQS = [200.0, 500.0, 800.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 4000.0, 8000.0]


def centre(fs, fl, fh):
    wl = 2 * fs * math.tan(math.pi * fl / fs)
    wh = 2 * fs * math.tan(math.pi * fh / fs)
    return fs / math.pi * math.atan(math.sqrt(wl * wh) / (2 * fs))


def main():
    rows = []
    ref = {"bandpass": centre(FS, FL, FH), "bandstop": 0.0}
    for order in (1, 2, 3):
        designs = {
            "bandpass": signal.cheby2(order, ATTEN, [FL, FH], btype="bandpass", fs=FS, output="sos"),
            "bandstop": signal.cheby2(order, ATTEN, [FL, FH], btype="bandstop", fs=FS, output="sos"),
        }
        for kind, sos in designs.items():
            _, h0 = signal.sosfreqz(sos, worN=np.array([ref[kind]]), fs=FS)
            base = 20 * np.log10(abs(h0[0]))
            _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
            for f, hv in zip(FREQS, h):
                rows.append((kind, order, FS, FL, FH, ATTEN, f,
                             repr(float(20 * np.log10(abs(hv)) - base))))
    with open(os.path.join(HERE, "cheby2_band.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["kind", "order", "fs", "fl", "fh", "atten", "freq_hz", "mag_db_rel"])
        out.writerows(rows)
    print(f"cheby2_band.csv: {len(rows)} rows")


if __name__ == "__main__":
    main()
