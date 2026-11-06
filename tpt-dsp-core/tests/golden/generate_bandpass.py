"""Reference magnitudes for Butterworth and Chebyshev I band-pass filters.

Run from this directory:  python generate_bandpass.py   -> bandpass.csv
Gains are relative to the digital centre frequency fs/pi * atan(w0/2fs),
with w0 = sqrt(wl*wh) and the band edges pre-warped.
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FL, FH = 1000.0, 3000.0
RIPPLE = 1.0
FREQS = [400.0, 700.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 4000.0, 6000.0]


def centre(fs, fl, fh):
    wl = 2 * fs * np.tan(np.pi * fl / fs)
    wh = 2 * fs * np.tan(np.pi * fh / fs)
    return fs / np.pi * np.arctan(np.sqrt(wl * wh) / (2 * fs))


def main():
    rows = []
    fc = centre(FS, FL, FH)
    for kind in ("butter", "cheby1"):
        for order in (1, 2, 3):
            if kind == "butter":
                sos = signal.butter(order, [FL, FH], btype="band", fs=FS, output="sos")
            else:
                sos = signal.cheby1(order, RIPPLE, [FL, FH], btype="band", fs=FS, output="sos")
            _, h_c = signal.sosfreqz(sos, worN=np.array([fc]), fs=FS)
            ref = 20 * np.log10(abs(h_c[0]))
            _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
            for f, hv in zip(FREQS, h):
                rows.append((kind, order, FS, FL, FH, RIPPLE, f,
                             repr(float(20 * np.log10(abs(hv)) - ref))))
    with open(os.path.join(HERE, "bandpass.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["kind", "order", "fs", "fl", "fh", "ripple", "freq_hz", "mag_db_rel_centre"])
        out.writerows(rows)
    print(f"bandpass.csv: {len(rows)} rows; centre {fc:.3f} Hz")


if __name__ == "__main__":
    main()
