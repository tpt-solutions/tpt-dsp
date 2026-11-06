"""Reference magnitudes for the elliptic band-pass, from scipy.

Run from this directory:  python generate_ellip_band.py   -> ellip_band.csv
Gains are relative to the digital centre frequency that the pre-warped
band-pass maps the analog centre sqrt(wl*wh) to:  fc = fs/pi * atan(w0/(2 fs)).
"""

import csv
import os

import numpy as np
from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000.0
FL, FH = 1000.0, 3000.0
RP, RS = 1.0, 40.0
FREQS = [400.0, 700.0, 1000.0, 1500.0, 2000.0, 2500.0, 3000.0, 4000.0, 6000.0]


def centre(fs, fl, fh):
    wl = 2 * fs * np.tan(np.pi * fl / fs)
    wh = 2 * fs * np.tan(np.pi * fh / fs)
    w0 = np.sqrt(wl * wh)
    return fs / np.pi * np.arctan(w0 / (2 * fs))


def main():
    rows = []
    for order in (1, 2, 3):
        sos = signal.ellip(order, RP, RS, [FL, FH], btype="band", fs=FS, output="sos")
        fc = centre(FS, FL, FH)
        _, h_c = signal.sosfreqz(sos, worN=np.array([fc]), fs=FS)
        ref = 20 * np.log10(abs(h_c[0]))
        _, h = signal.sosfreqz(sos, worN=np.array(FREQS), fs=FS)
        for f, hv in zip(FREQS, h):
            rows.append((order, FS, FL, FH, RP, RS, f, repr(float(20 * np.log10(abs(hv)) - ref))))
    with open(os.path.join(HERE, "ellip_band.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["order", "fs", "fl", "fh", "rp", "rs", "freq_hz", "mag_db_rel_centre"])
        out.writerows(rows)
    print(f"ellip_band.csv: {len(rows)} rows; centre {centre(FS, FL, FH):.3f} Hz")


if __name__ == "__main__":
    main()
