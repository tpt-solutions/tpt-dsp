"""Reference taps for the Parks-McClellan design, generated with scipy.

Run from this directory:  python generate_remez.py   -> remez.csv
The Rust test tests/golden.rs compares remez_fir output against these taps.
"""

import csv
import os

from scipy import signal

HERE = os.path.dirname(os.path.abspath(__file__))

CASES = [
    # (name, taps, bands, desired, weight)
    ("lp31", 31, [0.0, 0.2, 0.3, 0.5], [1, 0], [1, 1]),
    ("lp41_weighted", 41, [0.0, 0.2, 0.3, 0.5], [1, 0], [1, 10]),
    ("bp35", 35, [0.0, 0.1, 0.2, 0.3, 0.35, 0.5], [0, 1, 0], [5, 1, 5]),
]


def main():
    rows = []
    for name, taps, bands, desired, weight in CASES:
        h = signal.remez(taps, bands, desired, weight=weight, fs=1.0)
        for i, v in enumerate(h):
            rows.append((name, i, repr(float(v))))
    with open(os.path.join(HERE, "remez.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["case", "index", "tap"])
        out.writerows(rows)
    print(f"remez.csv: {len(rows)} rows")


if __name__ == "__main__":
    main()
