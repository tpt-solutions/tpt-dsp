//! `tpt-dsp-py` — Python bindings for the tpt-dsp framework (via pyo3).
//!
//! Exposes a small, real-time-safe analysis surface built on `tpt-dsp-core`
//! and `tpt-dsp-analysis`:
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
//! The extension module is named `tpt_dsp` (see `[lib] name` in `Cargo.toml`).
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tpt_dsp_analysis::{
    find_peaks, Averaging, RealtimeSpectrumAnalyzer, SpectrumConfig, DEFAULT_DB_FLOOR,
};
use tpt_dsp_core::{Complex32, FmDemodulator, WindowType};

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
fn rms(samples: Vec<f32>) -> f64 {
    tpt_dsp_analysis::rms(&samples) as f64
}

/// Zero-crossing rate: fraction of adjacent samples that change sign.
#[pyfunction]
fn zero_crossing_rate(samples: Vec<f32>) -> f64 {
    tpt_dsp_analysis::zero_crossing_rate(&samples) as f64
}

/// Spectral centroid (Hz) of a real signal.
#[pyfunction]
fn spectral_centroid(samples: Vec<f32>, sample_rate: f64) -> f64 {
    let sr = sample_rate as f32;
    let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size: next_fft(samples.len()),
        sample_rate: sr,
        ..SpectrumConfig::default()
    });
    process_all(&mut analyzer, &samples);
    let bins = tpt_dsp_analysis::spectral_centroid(analyzer.magnitude()) as f64;
    bins * analyzer.bin_width() as f64
}

/// Averaged one-sided magnitude spectrum as `(frequencies_hz, magnitude_db)`.
#[pyfunction]
fn spectrum(
    samples: Vec<f32>,
    sample_rate: f64,
    fft_size: usize,
    window: &str,
) -> PyResult<(Vec<f64>, Vec<f64>)> {
    let win = parse_window(window)?;
    let sr = sample_rate as f32;
    let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size,
        sample_rate: sr,
        window: win,
        averaging: Averaging::Linear,
        ..SpectrumConfig::default()
    });
    process_all(&mut analyzer, &samples);
    let bin_width = analyzer.bin_width() as f64;
    let freqs: Vec<f64> = (0..analyzer.bins()).map(|b| b as f64 * bin_width).collect();
    let db: Vec<f64> = analyzer.magnitude_db().iter().map(|&d| d as f64).collect();
    Ok((freqs, db))
}

/// FM-demodulate interleaved in-phase / quadrature samples into a real signal.
#[pyfunction]
fn fm_demod(i: Vec<f32>, q: Vec<f32>, sample_rate: f64, deviation: f64) -> PyResult<Vec<f32>> {
    if i.len() != q.len() {
        return Err(PyValueError::new_err("i and q must have equal length"));
    }
    let iq: Vec<Complex32> = i
        .iter()
        .zip(q.iter())
        .map(|(&re, &im)| Complex32::new(re, im))
        .collect();
    let mut demod = FmDemodulator::with_deviation(sample_rate as f32, deviation as f32);
    let mut audio = vec![0.0f32; iq.len()];
    demod.process(&iq, &mut audio);
    Ok(audio)
}

/// A summary `dict` of a real signal: dominant frequency, peak dB, RMS,
/// zero-crossing rate, spectral centroid, and the strongest peaks.
#[pyfunction]
fn analyze(
    samples: Vec<f32>,
    sample_rate: f64,
    fft_size: usize,
    window: &str,
    py: Python<'_>,
) -> PyResult<Py<PyDict>> {
    let win = parse_window(window)?;
    let sr = sample_rate as f32;
    let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig {
        fft_size,
        sample_rate: sr,
        window: win,
        averaging: Averaging::Linear,
        ..SpectrumConfig::default()
    });
    process_all(&mut analyzer, &samples);
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
    dict.set_item("rms", tpt_dsp_analysis::rms(&samples) as f64)?;
    dict.set_item(
        "zero_crossing_rate",
        tpt_dsp_analysis::zero_crossing_rate(&samples) as f64,
    )?;
    dict.set_item("spectral_centroid_hz", centroid_bins * bin_width)?;
    dict.set_item("top_peaks", peaks)?;
    Ok(dict.into())
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

/// The `tpt_dsp` Python extension module.
#[pymodule]
fn tpt_dsp(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(rms, m)?)?;
    m.add_function(wrap_pyfunction!(zero_crossing_rate, m)?)?;
    m.add_function(wrap_pyfunction!(spectral_centroid, m)?)?;
    m.add_function(wrap_pyfunction!(spectrum, m)?)?;
    m.add_function(wrap_pyfunction!(fm_demod, m)?)?;
    m.add_function(wrap_pyfunction!(analyze, m)?)?;
    Ok(())
}
