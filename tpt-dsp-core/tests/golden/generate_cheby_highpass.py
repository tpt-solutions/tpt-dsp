"""Reference magnitudes for Chebyshev type I and II high-pass filters, from scipy.

Run from this directory:  python generate_cheby_highpass.py  -> cheby_highpass.csv
The passband of a high-pass sits near Nyquist, so gains are relative to fs/2.
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FC = 3000.0
RIPPLE, ATTEN = 1.0, 40.0
FREQS = [200.0, 500.0, 1000.0, 2000.0, 2500.0, 3000.0, 3500.0, 4000.0, 6000.0, 12000.0]
REFERENCE = FS / 2


def main():
    rows = []
    for order in (1, 2, 3, 4):
        designs = {
            "cheby1": signal.cheby1(order, RIPPLE, FC, btype="highpass", fs=FS, output="sos"),
            "cheby2": signal.cheby2(order, ATTEN, FC, btype="highpass", fs=FS, output="sos"),
        }
        for kind, sos in designs.items():
            _, h0 = signal.sosfreqz(sos, worN=np.array([REFERENCE]), fs=FS)
            base = 20 * np.log10(abs(h0[0]))
            _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
            for f, hv in zip(FREQS, h):
                rows.append((kind, order, FS, FC, RIPPLE if kind == "cheby1" else ATTEN, f,
                             repr(float(20 * np.log10(abs(hv)) - base))))
    with open(os.path.join(HERE, "cheby_highpass.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["kind", "order", "fs", "fc", "ripple_or_atten", "freq_hz", "mag_db_rel_nyquist"])
        out.writerows(rows)
    print(f"cheby_highpass.csv: {len(rows)} rows")


if __name__ == "__main__":
    main()
