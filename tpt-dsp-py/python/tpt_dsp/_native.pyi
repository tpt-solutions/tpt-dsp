"""Type stubs for the compiled tpt_dsp._native extension."""

from typing import Dict, List, Tuple



def rms(samples: List[float]) -> float:
    """RMS energy of a real signal."""
    ...

def zero_crossing_rate(samples: List[float]) -> float:
    """Fraction of adjacent samples that change sign."""
    ...

def spectral_centroid(samples: List[float], sample_rate: float) -> float:
    """Spectral centroid (Hz) of a real signal."""
    ...

def spectrum(
    samples: List[float], sample_rate: float, fft_size: int, window: str
) -> Tuple[List[float], List[float]]:
    """Averaged one-sided magnitude spectrum as ``(frequencies_hz, magnitude_db)``.

    ``window`` is ``"hann"``, ``"hamming"`` or ``"blackman"``.
    """
    ...

def fm_demod(
    i: List[float], q: List[float], sample_rate: float, deviation: float
) -> List[float]:
    """FM-demodulate parallel in-phase and quadrature lists into audio."""
    ...

def analyze(
    samples: List[float], sample_rate: float, fft_size: int, window: str
) -> Dict[str, object]:
    """Summary of a real signal.

    The dict has ``dominant_hz``, ``peak_db``, ``rms``, ``zero_crossing_rate``,
    ``spectral_centroid_hz`` and ``top_peaks`` (a list of ``(bin, db)`` pairs).
    """
    ...

class Biquad:
    """A stateful RBJ biquad filter.

    ``kind`` is one of ``lowpass``, ``highpass``, ``bandpass``, ``notch``,
    ``allpass``, ``peaking``, ``lowshelf`` or ``highshelf``. ``gain_db`` only
    affects the peaking and shelving kinds.
    """

    def __init__(
        self,
        kind: str,
        sample_rate: float,
        freq: float,
        q: float = 0.707,
        gain_db: float = 0.0,
    ) -> None: ...
    def process(self, samples: List[float]) -> List[float]:
        """Filter samples; state carries over between calls."""
        ...
    def reset(self) -> None:
        """Clear the filter state."""
        ...

class Pid:
    """A PID controller with output limits and anti-windup."""

    def __init__(self, kp: float, ki: float, kd: float, sample_time: float) -> None: ...
    def set_setpoint(self, setpoint: float) -> None: ...
    def set_output_limits(self, min: float, max: float) -> None:
        """Clamp the output to ``[min, max]``; raises ``ValueError`` if min > max."""
        ...
    def update(self, measurement: float) -> float:
        """Advance one step with a new measurement and return the output."""
        ...
    def reset(self) -> None: ...

class Oscillator:
    """A stateful oscillator. ``waveform`` is ``sine``, ``sawtooth`` (or ``saw``),
    ``square`` or ``triangle``. The phase carries over between calls."""

    def __init__(self, sample_rate: float, freq: float, waveform: str = "sine") -> None: ...
    def process(self, n: int) -> List[float]:
        """Generate ``n`` samples."""
        ...
    def set_frequency(self, freq: float) -> None: ...
    def reset(self) -> None: ...
