"""Smoke and behaviour tests for the tpt_dsp Python bindings.

Run after installing the wheel:  python -m unittest discover -s tests
"""

import math
import unittest

import tpt_dsp

SR = 48_000.0


def sine(freq, n, sr=SR, amp=1.0):
    return [amp * math.sin(2 * math.pi * freq * i / sr) for i in range(n)]


class FunctionTests(unittest.TestCase):
    def test_rms_of_a_unit_sine(self):
        self.assertAlmostEqual(tpt_dsp.rms(sine(1000, 4800)), 1 / math.sqrt(2), places=2)

    def test_zero_crossing_rate_matches_frequency(self):
        # A 1 kHz sine crosses zero about 2000 times per second.
        zcr = tpt_dsp.zero_crossing_rate(sine(1000, 48_000))
        self.assertAlmostEqual(zcr * SR, 2000, delta=20)

    def test_spectral_centroid_tracks_pitch(self):
        low = tpt_dsp.spectral_centroid(sine(500, 4096), SR)
        high = tpt_dsp.spectral_centroid(sine(4000, 4096), SR)
        self.assertLess(low, high)
        self.assertAlmostEqual(high, 4000, delta=200)

    def test_spectrum_peaks_at_the_tone(self):
        freqs, db = tpt_dsp.spectrum(sine(2000, 8192), SR, 1024, "hann")
        self.assertEqual(len(freqs), len(db))
        peak = freqs[db.index(max(db))]
        self.assertAlmostEqual(peak, 2000, delta=SR / 1024)

    def test_spectrum_rejects_unknown_window(self):
        with self.assertRaises(ValueError):
            tpt_dsp.spectrum(sine(1000, 2048), SR, 1024, "kaiser")

    def test_analyze_returns_a_summary_dict(self):
        summary = tpt_dsp.analyze(sine(1000, 8192), SR, 1024, "blackman")
        self.assertAlmostEqual(summary["dominant_hz"], 1000, delta=SR / 1024)
        self.assertIn("top_peaks", summary)
        self.assertGreater(summary["rms"], 0.5)

    def test_fm_demod_of_a_constant_offset_is_constant(self):
        # A constant frequency offset demodulates to a constant level.
        n = 2000
        step = 2 * math.pi * 1000 / SR
        i = [math.cos(step * k) for k in range(n)]
        q = [math.sin(step * k) for k in range(n)]
        audio = tpt_dsp.fm_demod(i, q, SR, 75_000.0)
        self.assertEqual(len(audio), n)
        tail = audio[100:]
        self.assertLess(max(tail) - min(tail), 1e-3)

    def test_fm_demod_rejects_unequal_lengths(self):
        with self.assertRaises(ValueError):
            tpt_dsp.fm_demod([0.0, 1.0], [0.0], SR, 75_000.0)


class BiquadTests(unittest.TestCase):
    def test_lowpass_passes_dc_and_attenuates_high_frequency(self):
        lp = tpt_dsp.Biquad("lowpass", SR, 1000.0)
        out = lp.process([1.0] * 4000)
        self.assertAlmostEqual(out[-1], 1.0, places=3)

        lp.reset()
        out = lp.process(sine(12_000, 4800))
        self.assertLess(max(abs(x) for x in out[2400:]), 0.1)

    def test_state_carries_across_calls(self):
        whole = tpt_dsp.Biquad("lowpass", SR, 2000.0).process(sine(300, 200))
        split = tpt_dsp.Biquad("lowpass", SR, 2000.0)
        parts = split.process(sine(300, 200)[:120]) + split.process(sine(300, 200)[120:])
        self.assertEqual(len(parts), len(whole))
        # The first call must not be identical to a fresh filter's second half.
        for a, b in zip(parts, whole):
            self.assertAlmostEqual(a, b, places=6)

    def test_bad_kind_and_frequency_raise_value_error(self):
        with self.assertRaises(ValueError):
            tpt_dsp.Biquad("wobble", SR, 1000.0)
        with self.assertRaises(ValueError):
            tpt_dsp.Biquad("lowpass", SR, 30_000.0)  # above Nyquist


class PidTests(unittest.TestCase):
    def test_pid_drives_a_first_order_plant_to_setpoint(self):
        pid = tpt_dsp.Pid(2.0, 1.0, 0.0, 0.01)
        pid.set_setpoint(10.0)
        y = 0.0
        for _ in range(2000):
            y += 0.01 * pid.update(y)
        self.assertAlmostEqual(y, 10.0, delta=0.5)

    def test_output_limits_are_respected(self):
        pid = tpt_dsp.Pid(10.0, 5.0, 0.0, 0.01)
        pid.set_output_limits(-1.0, 1.0)
        pid.set_setpoint(100.0)
        for _ in range(50):
            self.assertLessEqual(abs(pid.update(0.0)), 1.0 + 1e-6)

    def test_inverted_limits_raise(self):
        pid = tpt_dsp.Pid(1.0, 0.0, 0.0, 0.01)
        with self.assertRaises(ValueError):
            pid.set_output_limits(1.0, -1.0)

    def test_invalid_sample_time_raises(self):
        with self.assertRaises(ValueError):
            tpt_dsp.Pid(1.0, 0.0, 0.0, 0.0)


if __name__ == "__main__":
    unittest.main()
