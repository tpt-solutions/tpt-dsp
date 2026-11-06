"""Reference integrated loudness (ITU-R BS.1770) from pyloudnorm.

Run from this directory with an interpreter that has pyloudnorm installed:
    python generate_lufs.py   -> lufs.csv

The signals are deterministic sines, so the Rust test in tests/golden.rs can
rebuild them exactly. Keep SIGNALS in step with the Rust test.
"""

import csv
import os

import numpy as np
import pyloudnorm as pyln

HERE = os.path.dirname(os.path.abspath(__file__))
FS = 48_000


def sine(freq, amp, secs):
    n = np.arange(int(FS * secs))
    return amp * np.sin(2 * np.pi * freq * n / FS)


SIGNALS = {
    "tone1k_quiet": lambda: sine(1000.0, 0.1, 10.0),
    "tone1k_loud": lambda: sine(1000.0, 0.5, 10.0),
    "tone60_low": lambda: sine(60.0, 0.2, 10.0),
    "gated_loud_then_quiet": lambda: np.concatenate(
        [sine(250.0, 0.3, 5.0), sine(4000.0, 0.03, 5.0)]
    ),
}


def main():
    # "DeMan" is the BS.1770 coefficient set; the default "K-weighting" class in
    # pyloudnorm uses a 1500 Hz, 4 dB shelf and is a different (older) design.
    meter = pyln.Meter(FS, filter_class="DeMan")
    rows = []
    for name, make in SIGNALS.items():
        x = make()
        rows.append((name, repr(float(meter.integrated_loudness(x)))))
    with open(os.path.join(HERE, "lufs.csv"), "w", newline="") as f:
        out = csv.writer(f)
        out.writerow(["signal", "integrated_lufs"])
        out.writerows(rows)
    for r in rows:
        print(r)


if __name__ == "__main__":
    main()
