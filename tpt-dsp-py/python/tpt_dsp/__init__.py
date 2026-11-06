"""Python bindings for the tpt-dsp framework.

The compiled extension lives in ``tpt_dsp._native``; everything it exports is
re-exported here, so ``import tpt_dsp`` is all callers need.
"""

from ._native import (  # noqa: F401
    Biquad,
    Oscillator,
    Pid,
    analyze,
    fm_demod,
    rms,
    spectral_centroid,
    spectrum,
    zero_crossing_rate,
)

__all__ = [
    "Biquad",
    "Oscillator",
    "Pid",
    "analyze",
    "fm_demod",
    "rms",
    "spectral_centroid",
    "spectrum",
    "zero_crossing_rate",
]
