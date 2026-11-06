"""Array inputs: a NumPy array must give the same results as the equivalent list.

With the default abi3 build, every input is extracted into a list of floats
first. With the `zero-copy` build, a 1-D C-contiguous float32 array is read in
place. These tests check the results agree in both builds, and that the
fallback inputs (float64, strided views) still work.
"""

import math
import unittest

import tpt_dsp

SR = 48_000.0

try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy is optional
    np = None


@unittest.skipIf(np is None, "numpy not installed")
class ArrayInputTests(unittest.TestCase):
    def setUp(self):
        self.x = np.sin(2 * np.pi * 1000.0 * np.arange(4096) / SR).astype(np.float32)
        self.y = np.roll(self.x, 7)

    def test_rms_matches_the_list(self):
        self.assertAlmostEqual(tpt_dsp.rms(self.x), tpt_dsp.rms(self.x.tolist()), places=6)

    def test_zero_crossing_rate_matches_the_list(self):
        self.assertAlmostEqual(
            tpt_dsp.zero_crossing_rate(self.x),
            tpt_dsp.zero_crossing_rate(self.x.tolist()),
            places=9,
        )

    def test_spectral_centroid_matches_the_list(self):
        self.assertAlmostEqual(
            tpt_dsp.spectral_centroid(self.x, SR),
            tpt_dsp.spectral_centroid(self.x.tolist(), SR),
            places=3,
        )

    def test_spectrum_matches_the_list(self):
        f_arr, d_arr = tpt_dsp.spectrum(self.x, SR, 1024, "hann")
        f_list, d_list = tpt_dsp.spectrum(self.x.tolist(), SR, 1024, "hann")
        self.assertEqual(len(f_arr), len(f_list))
        for a, b in zip(d_arr, d_list):
            self.assertAlmostEqual(a, b, places=4)

    def test_analyze_matches_the_list(self):
        from_arr = tpt_dsp.analyze(self.x, SR, 1024, "hann")
        from_list = tpt_dsp.analyze(self.x.tolist(), SR, 1024, "hann")
        self.assertAlmostEqual(from_arr["dominant_hz"], from_list["dominant_hz"], places=3)
        self.assertAlmostEqual(from_arr["rms"], from_list["rms"], places=6)
        self.assertAlmostEqual(from_arr["peak_db"], from_list["peak_db"], places=3)

    def test_biquad_process_matches_the_list(self):
        a = tpt_dsp.Biquad("lowpass", SR, 2000.0)
        b = tpt_dsp.Biquad("lowpass", SR, 2000.0)
        out_arr = a.process(self.x)
        out_list = b.process(self.x.tolist())
        self.assertEqual(len(out_arr), len(out_list))
        for u, v in zip(out_arr, out_list):
            self.assertAlmostEqual(u, v, places=6)

    def test_fm_demod_matches_the_list(self):
        arr = tpt_dsp.fm_demod(self.x, self.y, SR, 5000.0)
        lst = tpt_dsp.fm_demod(self.x.tolist(), self.y.tolist(), SR, 5000.0)
        self.assertEqual(len(arr), len(lst))
        for u, v in zip(arr, lst):
            self.assertAlmostEqual(u, v, places=5)

    def test_fm_demod_rejects_mismatched_lengths_for_arrays(self):
        with self.assertRaises(ValueError):
            tpt_dsp.fm_demod(self.x, self.y[:-1], SR, 5000.0)

    def test_strided_view_matches_its_list(self):
        view = self.x[::2]
        self.assertFalse(view.flags["C_CONTIGUOUS"])
        self.assertAlmostEqual(tpt_dsp.rms(view), tpt_dsp.rms(view.tolist()), places=6)

    def test_float64_array_is_still_accepted(self):
        wide = self.x.astype(np.float64)
        self.assertAlmostEqual(tpt_dsp.rms(wide), tpt_dsp.rms(wide.tolist()), places=6)

    def test_empty_array_matches_the_empty_list(self):
        empty = np.array([], dtype=np.float32)
        self.assertEqual(tpt_dsp.rms(empty), tpt_dsp.rms([]))
        self.assertEqual(tpt_dsp.zero_crossing_rate(empty), tpt_dsp.zero_crossing_rate([]))

    def test_rms_of_a_sine_is_one_over_root_two(self):
        self.assertAlmostEqual(tpt_dsp.rms(self.x), 1 / math.sqrt(2), places=2)


if __name__ == "__main__":
    unittest.main()
