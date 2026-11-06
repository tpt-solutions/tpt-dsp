"""Reference magnitudes for elliptic low-pass filters, from scipy.

Run from this directory:  python generate_ellip.py   -> ellip.csv
The Rust test in tests/golden.rs compares elliptic_lowpass against these.
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FC = 1000.0
RP = 1.0
RS = 40.0
FREQS = [50.0, 200.0, 500.0, 800.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0]


def main():
    rows = []
    for order in (1, 2, 3, 4, 5):
        sos = signal.ellip(order, RP, RS, FC, btype="low", fs=FS, output="sos")
        _, h0 = signal.sosfreqz(sos, worN=np.array([0.0]), fs=FS)
        dc = 20 * np.log10(abs(h0[0]))
        _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
        for f, hv in zip(FREQS, h):
            rows.append((order, FS, FC, RP, RS, f, repr(float(20 * np.log10(abs(hv)) - dc))))
    with open(os.path.join(HERE, "ellip.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["order", "fs", "fc", "rp", "rs", "freq_hz", "mag_db_rel_dc"])
        out.writerows(rows)
    print(f"ellip.csv: {len(rows)} rows")


if __name__ == "__main__":
    main()


def write_high():
    """Reference magnitudes for the elliptic high-pass (same specification).

    Gains are relative to the value at Nyquist, where the high-pass passband
    sits; the Rust side normalises to unit gain at Nyquist.
    """
    rows = []
    for order in (2, 3, 4, 5):
        sos = signal.ellip(order, RP, RS, FC, btype="high", fs=FS, output="sos")
        _, h_nyq = signal.sosfreqz(sos, worN=np.array([FS / 2 - 1.0]), fs=FS)
        ref = 20 * np.log10(abs(h_nyq[0]))
        freqs = [200.0, 500.0, 800.0, 1000.0, 1500.0, 2000.0, 4000.0, 8000.0, 16000.0]
        _, h = signal.sosfreqz(sos, worN=np.array(freqs), fs=FS)
        for f, hv in zip(freqs, h):
            rows.append((order, FS, FC, RP, RS, f, repr(float(20 * np.log10(abs(hv)) - ref))))
    with open(os.path.join(HERE, "ellip_high.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["order", "fs", "fc", "rp", "rs", "freq_hz", "mag_db_rel_nyquist"])
        out.writerows(rows)
    print(f"ellip_high.csv: {len(rows)} rows")


if __name__ == "__main__":
    write_high()
