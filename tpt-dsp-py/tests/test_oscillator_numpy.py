"""Oscillator behaviour, and interop with numpy arrays (skipped without numpy)."""

import math
import unittest

import tpt_dsp

SR = 48_000.0


class OscillatorTests(unittest.TestCase):
    def test_sine_has_unit_peak_and_the_right_period(self):
        osc = tpt_dsp.Oscillator(SR, 1000.0, "sine")
        x = osc.process(4800)  # 100 periods
        self.assertAlmostEqual(max(x), 1.0, places=3)
        rising = sum(1 for a, b in zip(x, x[1:]) if a < 0.0 <= b)
        self.assertAlmostEqual(rising, 100, delta=1)

    def test_phase_carries_across_calls(self):
        whole = tpt_dsp.Oscillator(SR, 440.0, "sine").process(1000)
        osc = tpt_dsp.Oscillator(SR, 440.0, "sine")
        parts = osc.process(400) + osc.process(600)
        for a, b in zip(parts, whole):
            self.assertAlmostEqual(a, b, places=6)

    def test_sawtooth_and_square_stay_in_range(self):
        for shape in ("sawtooth", "saw", "square", "triangle"):
            x = tpt_dsp.Oscillator(SR, 220.0, shape).process(2000)
            self.assertLessEqual(max(abs(v) for v in x), 1.0 + 1e-6, shape)

    def test_reset_restarts_the_waveform(self):
        osc = tpt_dsp.Oscillator(SR, 300.0, "square")
        first = osc.process(50)
        osc.process(123)
        osc.reset()
        self.assertEqual(osc.process(50), first)

    def test_set_frequency_changes_the_pitch(self):
        osc = tpt_dsp.Oscillator(SR, 100.0, "sine")
        osc.set_frequency(1000.0)
        x = osc.process(4800)
        rising = sum(1 for a, b in zip(x, x[1:]) if a < 0.0 <= b)
        self.assertAlmostEqual(rising, 100, delta=1)

    def test_unknown_waveform_raises(self):
        with self.assertRaises(ValueError):
            tpt_dsp.Oscillator(SR, 440.0, "noise")


try:
    import numpy as np
except ImportError:  # pragma: no cover - numpy is optional
    np = None


@unittest.skipIf(np is None, "numpy not installed")
class NumpyInteropTests(unittest.TestCase):
    def test_numpy_arrays_are_accepted_as_sample_lists(self):
        n = np.arange(4800)
        sig = np.sin(2 * np.pi * 1000.0 * n / SR)
        self.assertAlmostEqual(tpt_dsp.rms(sig), 1 / math.sqrt(2), places=2)
        freqs, db = tpt_dsp.spectrum(sig, SR, 1024, "hann")
        peak = freqs[db.index(max(db))]
        self.assertAlmostEqual(peak, 1000.0, delta=SR / 1024)

    def test_numpy_output_can_be_wrapped_back_into_an_array(self):
        out = tpt_dsp.Oscillator(SR, 440.0, "sine").process(256)
        arr = np.array(out)
        self.assertEqual(arr.shape, (256,))
        self.assertTrue(np.all(np.isfinite(arr)))


if __name__ == "__main__":
    unittest.main()
