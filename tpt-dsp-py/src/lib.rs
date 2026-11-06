//! `tpt-dsp-py` — Python bindings for the tpt-dsp framework (via pyo3).
//!
//! Exposes a small, real-time-safe analysis surface built on `tpt-dsp-core`
//! and `tpt-dsp-analysis`:
//!
//! Sample inputs accept a list of floats. With the `zero-copy` feature (a
//! non-abi3 build), a 1-D `float32` buffer such as a NumPy array is read in
//! place without a copy.
//!
//! - `rms(samples)` — RMS energy of a real signal.
//! - `zero_crossing_rate(samples)` — fraction of sign changes.
//! - `spectral_centroid(samples, sample_rate)` — brightness in Hz.
//! - `spectrum(samples, sample_rate, fft_size, window)` — averaged `(frequencies, db)`
//!   magnitude spectrum.
//! - `fm_demod(i, q, sample_rate, deviation)` — FM-demodulate interleaved IQ.
//! - `analyze(samples, sample_rate, fft_size, window)` — a summary `dict`
//!   (dominant frequency, peak dB, RMS, zero-crossing rate, centroid, top peaks).
//!
//! The compiled module is `tpt_dsp._native`; the `tpt_dsp` package re-exports it.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

#[cfg(feature = "zero-copy")]
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tpt_dsp_analysis::{
    find_peaks, Averaging, RealtimeSpectrumAnalyzer, SpectrumConfig, DEFAULT_DB_FLOOR,
};
use tpt_dsp_core::{BiquadType, Complex32, FmDemodulator, WindowType};

#[cfg(all(feature = "zero-copy", feature = "abi3"))]
compile_error!("the `zero-copy` feature needs a non-abi3 build: "
    "use `--no-default-features --features zero-copy`");

/// Run `f` on the samples in `obj`.
///
/// With the `zero-copy` feature, a 1-D, C-contiguous float32 buffer (a NumPy
/// `float32` array, `array.array('f')` or a `memoryview`) is read in place.
/// Anything else, such as a list, a float64 array or a strided view, is
/// extracted into a `Vec<f32>` first, via `tolist()` when the object has one.
/// The default abi3 build has no buffer access, so it always takes that path.
fn with_samples<T>(
    py: Python<'_>,
    obj: &Bound<'_, PyAny>,
    f: impl FnOnce(&[f32]) -> PyResult<T>,
) -> PyResult<T> {
    let _ = py;
    #[cfg(feature = "zero-copy")]
    {
        if let Ok(buffer) = PyBuffer::<f32>::get(obj) {
            if buffer.dimensions() == 1 && buffer.is_c_contiguous() {
                if let Some(ptr) = buffer.as_slice_ptr() {
                    // SAFETY: the pointer covers the buffer's `len` contiguous
                    // `f32`s and stays valid while `buffer` is alive, which it
                    // is for this call. We only read through it. Python code
                    // cannot run while we hold the GIL. A non-Python thread
                    // that writes to the same memory at the same time would be
                    // a data race, the same caveat as any zero-copy view of a
                    // NumPy array.
                    let slice = unsafe { ptr.as_ref() };
                    return f(slice);
                }
            }
        }
    }
    // NumPy arrays and `array.array` convert to a list in C, which is much
    // faster than extracting element by element. Lists have no `tolist`, so
    // they take the direct path.
    let owned: Vec<f32> = match obj.call_method0("tolist") {
        Ok(list) => list.extract()?,
        Err(_) => obj.extract()?,
    };
    f(&owned)
}

fn parse_window(name: &str) -> Result<WindowType, PyErr> {
    match name.to_ascii_lowercase().as_str() {
        "hann" | "hanning" => Ok(WindowType::Hann),
        "hamming" => Ok(WindowType::Hamming),
        "blackman" => Ok(WindowType::Blackman),
        other => Err(PyValueError::new_err(format!("unknown window `{other}`"))),
    }
}

/// RMS energy of a real signal.
#[pyfunction]
fn rms(py: Python<'_>, samples: &Bound<'_, PyAny>) -> PyResult<f64> {
    with_samples(py, samples, |s| Ok(tpt_dsp_analysis::rms(s) as f64))
}

/// Zero-crossing rate: fraction of adjacent samples that change sign.
#[pyfunction]
fn zero_crossing_rate(py: Python<'_>, samples: &Bound<'_, PyAny>) -> PyResult<f64> {
    with_samples(py, samples, |s| {
        Ok(tpt_dsp_analysis::zero_crossing_rate(s) as f64)
    })
}

/// Spectral centroid (Hz) of a real signal.
#[pyfunction]
fn spectral_centroid(
    py: Python<'_>,
    samples: &Bound<'_, PyAny>,
    sample_rate: f64,
) -> PyResult<f64> {
    with_samples(py, samples, |s| {
        let sr = sample_rate as f32;
        let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
            fft_size: next_fft(s.len()),
            sample_rate: sr,
            ..SpectrumConfig::default()
        });
        process_all(&mut analyzer, s);
        let bins = tpt_dsp_analysis::spectral_centroid(analyzer.magnitude()) as f64;
        Ok(bins * analyzer.bin_width() as f64)
    })
}

/// Averaged one-sided magnitude spectrum as `(frequencies_hz, magnitude_db)`.
#[pyfunction]
fn spectrum(
    py: Python<'_>,
    samples: &Bound<'_, PyAny>,
    sample_rate: f64,
    fft_size: usize,
    window: &str,
) -> PyResult<(Vec<f64>, Vec<f64>)> {
    let win = parse_window(window)?;
    with_samples(py, samples, |s| {
        let sr = sample_rate as f32;
        let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
            fft_size,
            sample_rate: sr,
            window: win,
            averaging: Averaging::Linear,
            ..SpectrumConfig::default()
        });
        process_all(&mut analyzer, s);
        let bin_width = analyzer.bin_width() as f64;
        let freqs: Vec<f64> = (0..analyzer.bins()).map(|b| b as f64 * bin_width).collect();
        let db: Vec<f64> = analyzer.magnitude_db().iter().map(|&d| d as f64).collect();
        Ok((freqs, db))
    })
}

/// FM-demodulate interleaved in-phase / quadrature samples into a real signal.
#[pyfunction]
fn fm_demod(
    py: Python<'_>,
    i: &Bound<'_, PyAny>,
    q: &Bound<'_, PyAny>,
    sample_rate: f64,
    deviation: f64,
) -> PyResult<Vec<f32>> {
    with_samples(py, i, |ii| {
        with_samples(py, q, |qq| {
            if ii.len() != qq.len() {
                return Err(PyValueError::new_err("i and q must have equal length"));
            }
            let iq: Vec<Complex32> = ii
                .iter()
                .zip(qq.iter())
                .map(|(&re, &im)| Complex32::new(re, im))
                .collect();
            let mut demod = FmDemodulator::with_deviation(sample_rate as f32, deviation as f32);
            let mut audio = vec![0.0f32; iq.len()];
            demod.process(&iq, &mut audio);
            Ok(audio)
        })
    })
}

/// A summary `dict` of a real signal: dominant frequency, peak dB, RMS,
/// zero-crossing rate, spectral centroid, and the strongest peaks.
#[pyfunction]
fn analyze(
    samples: &Bound<'_, PyAny>,
    sample_rate: f64,
    fft_size: usize,
    window: &str,
    py: Python<'_>,
) -> PyResult<Py<PyDict>> {
    let win = parse_window(window)?;
    with_samples(py, samples, |s| {
        let sr = sample_rate as f32;
        let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
            fft_size,
            sample_rate: sr,
            window: win,
            averaging: Averaging::Linear,
            ..SpectrumConfig::default()
        });
        process_all(&mut analyzer, s);
        let bin_width = analyzer.bin_width() as f64;
        let peak = analyzer.peak();
        let (dom_hz, peak_db) = peak
            .map(|p| (p.frequency as f64, p.magnitude_db as f64))
            .unwrap_or((0.0, DEFAULT_DB_FLOOR as f64));
        let centroid_bins = tpt_dsp_analysis::spectral_centroid(analyzer.magnitude()) as f64;
        let mut peaks: Vec<(usize, f64)> = find_peaks(analyzer.magnitude_db(), 0.1)
            .into_iter()
            .map(|b| (b, analyzer.magnitude_db()[b] as f64))
            .collect();
        peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        peaks.truncate(8);

        let dict = PyDict::new(py);
        dict.set_item("dominant_hz", dom_hz)?;
        dict.set_item("peak_db", peak_db)?;
        dict.set_item("rms", tpt_dsp_analysis::rms(s) as f64)?;
        dict.set_item(
            "zero_crossing_rate",
            tpt_dsp_analysis::zero_crossing_rate(s) as f64,
        )?;
        dict.set_item("spectral_centroid_hz", centroid_bins * bin_width)?;
        dict.set_item("top_peaks", peaks)?;
        Ok(dict.into())
    })
}

fn next_fft(n: usize) -> usize {
    let mut size = 1usize;
    while size < n && size < 8192 {
        size <<= 1;
    }
    if size < 8 {
        size = 8;
    }
    size
}

fn process_all(analyzer: &mut RealtimeSpectrumAnalyzer, samples: &[f32]) {
    let n = analyzer.fft_size();
    for chunk in samples.chunks(n) {
        let mut block = vec![0.0f32; n];
        block[..chunk.len()].copy_from_slice(chunk);
        analyzer.process(&block);
    }
}

fn parse_biquad_kind(name: &str) -> Result<BiquadType, PyErr> {
    match name.to_ascii_lowercase().as_str() {
        "lowpass" | "lp" => Ok(BiquadType::LowPass),
        "highpass" | "hp" => Ok(BiquadType::HighPass),
        "bandpass" | "bp" => Ok(BiquadType::BandPass),
        "notch" => Ok(BiquadType::Notch),
        "allpass" => Ok(BiquadType::AllPass),
        "peaking" => Ok(BiquadType::Peaking),
        "lowshelf" => Ok(BiquadType::LowShelf),
        "highshelf" => Ok(BiquadType::HighShelf),
        other => Err(PyValueError::new_err(format!(
            "unknown biquad kind `{other}`"
        ))),
    }
}

/// A stateful RBJ biquad filter.
///
/// `kind` is one of `lowpass`, `highpass`, `bandpass`, `notch`, `allpass`,
/// `peaking`, `lowshelf` or `highshelf`. `gain_db` applies to the peaking and
/// shelving kinds only.
#[pyclass(name = "Biquad", module = "tpt_dsp._native")]
pub struct PyBiquad {
    inner: tpt_dsp_core::Biquad<f32>,
}

#[pymethods]
impl PyBiquad {
    #[new]
    #[pyo3(signature = (kind, sample_rate, freq, q = 0.707, gain_db = 0.0))]
    fn new(kind: &str, sample_rate: f32, freq: f32, q: f32, gain_db: f32) -> PyResult<Self> {
        let k = parse_biquad_kind(kind)?;
        let coeffs =
            tpt_dsp_core::BiquadCoeffs::<f32>::try_design(k, sample_rate, freq, q, gain_db)
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            inner: tpt_dsp_core::Biquad::from_coeffs(coeffs),
        })
    }

    /// Filter a list or float32 array of samples. State carries over between calls.
    fn process(&mut self, py: Python<'_>, samples: &Bound<'_, PyAny>) -> PyResult<Vec<f32>> {
        with_samples(py, samples, |s| {
            Ok(s.iter().map(|&x| self.inner.tick(x)).collect())
        })
    }

    /// Clear the filter state.
    fn reset(&mut self) {
        self.inner.reset();
    }
}

/// A PID controller with anti-windup on the output limits.
#[pyclass(name = "Pid", module = "tpt_dsp._native")]
pub struct PyPid {
    inner: tpt_dsp_control::Pid,
}

#[pymethods]
impl PyPid {
    #[new]
    fn new(kp: f32, ki: f32, kd: f32, sample_time: f32) -> PyResult<Self> {
        let inner = tpt_dsp_control::Pid::try_new(kp, ki, kd, sample_time)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self { inner })
    }

    /// Set the target value.
    fn set_setpoint(&mut self, setpoint: f32) {
        self.inner.set_setpoint(setpoint);
    }

    /// Clamp the output to `[min, max]`.
    fn set_output_limits(&mut self, min: f32, max: f32) -> PyResult<()> {
        if min.is_nan() || max.is_nan() || min > max {
            return Err(PyValueError::new_err("min must be <= max"));
        }
        self.inner.set_output_limits(min, max);
        Ok(())
    }

    /// Advance one step with a new measurement and return the output.
    fn update(&mut self, measurement: f32) -> f32 {
        self.inner.update(measurement)
    }

    /// Clear the integral and derivative history.
    fn reset(&mut self) {
        self.inner.reset();
    }
}

/// A stateful oscillator producing blocks of samples.
///
/// `waveform` is `sine`, `sawtooth` (or `saw`), `square` or `triangle`. The
/// phase carries over between `process` calls.
#[pyclass(name = "Oscillator", module = "tpt_dsp._native")]
pub struct PyOscillator {
    inner: tpt_dsp_audio::Oscillator,
}

#[pymethods]
impl PyOscillator {
    #[new]
    #[pyo3(signature = (sample_rate, freq, waveform = "sine"))]
    fn new(sample_rate: f32, freq: f32, waveform: &str) -> PyResult<Self> {
        let shape = match waveform.to_ascii_lowercase().as_str() {
            "sine" => tpt_dsp_audio::Waveform::Sine,
            "sawtooth" | "saw" => tpt_dsp_audio::Waveform::Sawtooth,
            "square" => tpt_dsp_audio::Waveform::Square,
            "triangle" => tpt_dsp_audio::Waveform::Triangle,
            other => return Err(PyValueError::new_err(format!("unknown waveform `{other}`"))),
        };
        Ok(Self {
            inner: tpt_dsp_audio::Oscillator::with_waveform(sample_rate, freq, shape),
        })
    }

    /// Generate `n` samples.
    fn process(&mut self, n: usize) -> Vec<f32> {
        (0..n).map(|_| self.inner.tick()).collect()
    }

    /// Change the frequency in Hz (non-finite or negative values become 0).
    fn set_frequency(&mut self, freq: f32) {
        self.inner.set_frequency(freq);
    }

    /// Reset the phase to zero.
    fn reset(&mut self) {
        self.inner.reset();
    }
}

/// The compiled extension module; `tpt_dsp` re-exports everything from here.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rms, m)?)?;
    m.add_function(wrap_pyfunction!(zero_crossing_rate, m)?)?;
    m.add_function(wrap_pyfunction!(spectral_centroid, m)?)?;
    m.add_function(wrap_pyfunction!(spectrum, m)?)?;
    m.add_function(wrap_pyfunction!(fm_demod, m)?)?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    m.add_class::<PyBiquad>()?;
    m.add_class::<PyPid>()?;
    m.add_class::<PyOscillator>()?;
    Ok(())
}
