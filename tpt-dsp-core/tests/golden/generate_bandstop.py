"""Reference magnitudes for Butterworth, Chebyshev I and elliptic band-stop
filters, from scipy.

Run from this directory:  python generate_bandstop.py   -> bandstop.csv
The passbands of a band-stop sit at DC and Nyquist, so gains are relative to
DC.
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FL, FH = 1000.0, 3000.0
RIPPLE, ATTEN = 1.0, 40.0
FREQS = [200.0, 500.0, 800.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 8000.0]


def main():
    rows = []
    for order in (1, 2, 3):
        designs = {
            "butter": signal.butter(order, [FL, FH], btype="bandstop", fs=FS, output="sos"),
            "cheby1": signal.cheby1(order, RIPPLE, [FL, FH], btype="bandstop", fs=FS, output="sos"),
            "ellip": signal.ellip(order, RIPPLE, ATTEN, [FL, FH], btype="bandstop", fs=FS, output="sos"),
        }
        for kind, sos in designs.items():
            _, h0 = signal.sosfreqz(sos, worN=np.array([0.0]), fs=FS)
            dc = 20 * np.log10(abs(h0[0]))
            _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
            for f, hv in zip(FREQS, h):
                rows.append((kind, order, FS, FL, FH, RIPPLE, ATTEN, f,
                             repr(float(20 * np.log10(abs(hv)) - dc))))
    with open(os.path.join(HERE, "bandstop.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["kind", "order", "fs", "fl", "fh", "ripple", "atten", "freq_hz", "mag_db_rel_dc"])
        out.writerows(rows)
    print(f"bandstop.csv: {len(rows)} rows")


if __name__ == "__main__":
    main()
