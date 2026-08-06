//! Real-time spectrum analysis: running FFT magnitude averaging and peak
//! detection over magnitude spectra.
//!
//! [`SpectrumAnalyzer`] averages magnitude frames that the caller has already
//! transformed. [`RealtimeSpectrumAnalyzer`] is the full chain: it windows a
//! block of real samples, transforms it with the RustFFT-backed
//! [`FftPlan`](tpt_dsp_core::FftPlan), normalises the one-sided magnitude
//! spectrum, averages it and converts it to dB. Every buffer is allocated at
//! construction, so [`process`](RealtimeSpectrumAnalyzer::process) is
//! allocation-free.

use tpt_dsp_core::{windowed, FftPlan, WindowType, C32};

/// Default lower clamp for dB conversion.
pub const DEFAULT_DB_FLOOR: f32 = -120.0;

/// A running-averaged magnitude spectrum for streaming display / analysis.
///
/// Each new magnitude frame is blended into the stored average with an
/// exponential factor `averaging` in `(0, 1]`: `avg = (1-a)·avg + a·frame`.
/// `averaging = 1` disables smoothing; smaller values give a steadier
/// waterfall. All state is pre-allocated, so [`push`](Self::push) is
/// allocation-free.
#[derive(Debug, Clone)]
pub struct SpectrumAnalyzer {
    size: usize,
    avg: Vec<f32>,
    averaging: f32,
}

impl SpectrumAnalyzer {
    /// Create an analyzer for spectra of `size` bins.
    ///
    /// # Panics
    ///
    /// Panics if `size` is zero or `averaging` is outside `(0, 1]`.
    pub fn new(size: usize, averaging: f32) -> Self {
        assert!(size > 0, "spectrum size must be positive");
        assert!(
            (0.0..=1.0).contains(&averaging),
            "averaging must be in (0, 1]"
        );
        Self {
            size,
            avg: vec![0.0; size],
            averaging,
        }
    }

    /// Number of bins.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Blend a new magnitude frame into the running average.
    pub fn push(&mut self, frame: &[f32]) {
        assert_eq!(frame.len(), self.size, "frame length mismatch");
        let a = self.averaging;
        let inv = 1.0 - a;
        for (slot, &x) in self.avg.iter_mut().zip(frame.iter()) {
            *slot = inv * *slot + a * x;
        }
    }

    /// The current averaged magnitude spectrum.
    pub fn spectrum(&self) -> &[f32] {
        &self.avg
    }

    /// Reset the average to zero.
    pub fn reset(&mut self) {
        for s in self.avg.iter_mut() {
            *s = 0.0;
        }
    }
}

/// Find local maxima (peaks) in a magnitude spectrum.
///
/// A bin is a peak when it is strictly greater than both neighbours and its
/// value exceeds `threshold · max`. Endpoints are compared against their
/// single neighbour only.
pub fn find_peaks(magnitude: &[f32], threshold: f32) -> Vec<usize> {
    if magnitude.is_empty() {
        return Vec::new();
    }
    let max = magnitude.iter().cloned().fold(0.0f32, f32::max);
    let cutoff = threshold * max;
    let mut peaks = Vec::new();
    for i in 0..magnitude.len() {
        let v = magnitude[i];
        if v <= cutoff {
            continue;
        }
        let left_ok = i == 0 || v > magnitude[i - 1];
        let right_ok = i + 1 >= magnitude.len() || v >= magnitude[i + 1];
        if left_ok && right_ok {
            peaks.push(i);
        }
    }
    peaks
}

/// Index of the strongest bin in a magnitude spectrum (0 for an empty one).
pub fn peak_bin(magnitude: &[f32]) -> usize {
    magnitude
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Estimated dominant frequency from a magnitude spectrum.
///
/// `sample_rate` and `fft_size` map bin `k` to `k · sample_rate / fft_size`
/// Hz. Returns `0.0` for an empty spectrum.
pub fn dominant_frequency(magnitude: &[f32], sample_rate: f32, fft_size: usize) -> f32 {
    if magnitude.is_empty() || fft_size == 0 {
        return 0.0;
    }
    let k = peak_bin(magnitude);
    k as f32 * sample_rate / fft_size as f32
}

/// Convert a linear magnitude to decibels relative to `reference`.
///
/// Returns `20·log10(magnitude / reference)` clamped from below at
/// `floor_db`; non-positive magnitudes map to `floor_db`.
///
/// # Panics
///
/// Panics if `reference` is not positive.
pub fn linear_to_db(magnitude: f32, reference: f32, floor_db: f32) -> f32 {
    assert!(reference > 0.0, "dB reference must be positive");
    if magnitude <= 0.0 {
        return floor_db;
    }
    let db = 20.0 * (magnitude / reference).log10();
    if db < floor_db {
        floor_db
    } else {
        db
    }
}

/// Convert a level in decibels back to a linear magnitude relative to
/// `reference`.
///
/// # Panics
///
/// Panics if `reference` is not positive.
pub fn db_to_linear(db: f32, reference: f32) -> f32 {
    assert!(reference > 0.0, "dB reference must be positive");
    reference * 10.0f32.powf(db / 20.0)
}

/// Parabolic (quadratic) interpolation of the vertex around `bin`.
///
/// Returns the sub-bin offset in `[-0.5, 0.5]` and the interpolated vertex
/// value. Fit a parabola through `values[bin-1..=bin+1]`; the classic QIFFT
/// estimator expects `values` on a logarithmic (dB) scale. Edge bins and
/// degenerate (flat) neighbourhoods yield a zero offset.
pub fn parabolic_interpolate(values: &[f32], bin: usize) -> (f32, f32) {
    let Some(&centre) = values.get(bin) else {
        return (0.0, 0.0);
    };
    if bin == 0 || bin + 1 >= values.len() {
        return (0.0, centre);
    }
    let (left, right) = (values[bin - 1], values[bin + 1]);
    let denom = left - 2.0 * centre + right;
    if denom.abs() <= f32::EPSILON {
        return (0.0, centre);
    }
    let offset = (0.5 * (left - right) / denom).clamp(-0.5, 0.5);
    (offset, centre - 0.25 * (left - right) * offset)
}

/// How successive magnitude frames are combined into the displayed spectrum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Averaging {
    /// No smoothing: each frame replaces the previous one.
    None,
    /// Exponential smoothing with a fixed per-frame coefficient in `(0, 1]`.
    Exponential(f32),
    /// Exponential smoothing specified as a time constant in seconds; the
    /// coefficient is derived from the block duration `fft_size / sample_rate`.
    TimeConstant(f32),
    /// Cumulative (linear) average of every frame since the last reset.
    Linear,
}

/// Configuration for a [`RealtimeSpectrumAnalyzer`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectrumConfig {
    /// Transform length, and the block size accepted by `process`.
    pub fft_size: usize,
    /// Sample rate in Hz, used to map bins to frequencies.
    pub sample_rate: f32,
    /// Analysis window applied before the transform.
    pub window: WindowType,
    /// How magnitude frames are averaged over time.
    pub averaging: Averaging,
    /// Linear magnitude that reads as 0 dB (1.0 = full-scale sine).
    pub reference: f32,
    /// Lower clamp applied to every dB value.
    pub floor_db: f32,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: 1024,
            sample_rate: 48_000.0,
            window: WindowType::Hann,
            averaging: Averaging::Exponential(0.25),
            reference: 1.0,
            floor_db: DEFAULT_DB_FLOOR,
        }
    }
}

/// A spectral peak with a sub-bin frequency estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpectralPeak {
    /// Interpolated (fractional) bin index.
    pub bin: f32,
    /// Interpolated peak frequency in Hz.
    pub frequency: f32,
    /// Interpolated peak level in dB.
    pub magnitude_db: f32,
    /// Interpolated peak level as a linear magnitude.
    pub magnitude: f32,
}

/// A windowed, FFT-based, time-averaged spectrum analyzer for real samples.
///
/// Feed it blocks of exactly [`fft_size`](Self::fft_size) samples. Each block
/// is windowed, transformed, folded into a one-sided amplitude-corrected
/// magnitude spectrum of [`bins`](Self::bins) entries, blended into the
/// running average and converted to dB. A full-scale sine at a bin centre
/// reads `1.0` in [`magnitude`](Self::magnitude) and `0.0` dB in
/// [`magnitude_db`](Self::magnitude_db) with the default reference.
///
/// The first frame after construction or [`reset`](Self::reset) seeds the
/// average directly instead of rising from zero.
///
/// # Examples
///
/// ```
/// use tpt_dsp_analysis::{RealtimeSpectrumAnalyzer, SpectrumConfig};
///
/// let mut analyzer = RealtimeSpectrumAnalyzer::new(SpectrumConfig::default());
/// let n = analyzer.fft_size();
/// let block: Vec<f32> = (0..n)
///     .map(|i| (core::f32::consts::TAU * 64.0 * i as f32 / n as f32).sin())
///     .collect();
/// analyzer.process(&block);
///
/// let peak = analyzer.peak().unwrap();
/// assert!((peak.frequency - 3000.0).abs() < 1.0);
/// assert!(peak.magnitude_db.abs() < 0.1);
/// ```
pub struct RealtimeSpectrumAnalyzer {
    config: SpectrumConfig,
    coefficient: f32,
    window: Vec<f32>,
    window_gain: f32,
    buffer: Vec<C32>,
    magnitude: Vec<f32>,
    db: Vec<f32>,
    fft: FftPlan,
    frames: u64,
}

impl RealtimeSpectrumAnalyzer {
    /// Create an analyzer from `config`, pre-allocating every buffer.
    ///
    /// # Panics
    ///
    /// Panics if `fft_size` is below 2, if `sample_rate` or `reference` is not
    /// positive, if `floor_db` is not finite, or if the averaging parameters
    /// are out of range (`Exponential` needs a coefficient in `(0, 1]`,
    /// `TimeConstant` a positive number of seconds).
    pub fn new(config: SpectrumConfig) -> Self {
        assert!(config.fft_size >= 2, "fft_size must be at least 2");
        assert!(config.sample_rate > 0.0, "sample_rate must be positive");
        assert!(config.reference > 0.0, "reference must be positive");
        assert!(config.floor_db.is_finite(), "floor_db must be finite");
        validate_averaging(config.averaging);

        let mut window = vec![0.0f32; config.fft_size];
        windowed(config.window, config.fft_size, &mut window);
        let window_gain = window.iter().sum::<f32>();
        assert!(window_gain > 0.0, "window has zero gain");

        let bins = config.fft_size / 2 + 1;
        Self {
            config,
            coefficient: coefficient_for(config),
            window,
            window_gain,
            buffer: vec![C32::default(); config.fft_size],
            magnitude: vec![0.0; bins],
            db: vec![config.floor_db; bins],
            fft: FftPlan::new_forward(config.fft_size),
            frames: 0,
        }
    }

    /// Create an analyzer with the default window, averaging and dB settings.
    pub fn with_size(fft_size: usize, sample_rate: f32) -> Self {
        Self::new(SpectrumConfig {
            fft_size,
            sample_rate,
            ..SpectrumConfig::default()
        })
    }

    /// The active configuration.
    pub fn config(&self) -> SpectrumConfig {
        self.config
    }

    /// Transform length, i.e. the required block size.
    pub fn fft_size(&self) -> usize {
        self.config.fft_size
    }

    /// Number of one-sided bins (`fft_size / 2 + 1`).
    pub fn bins(&self) -> usize {
        self.magnitude.len()
    }

    /// Spacing between bins in Hz.
    pub fn bin_width(&self) -> f32 {
        self.config.sample_rate / self.config.fft_size as f32
    }

    /// Centre frequency of bin `k` in Hz.
    pub fn bin_frequency(&self, k: usize) -> f32 {
        k as f32 * self.bin_width()
    }

    /// Frames processed since construction or the last [`reset`](Self::reset).
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Change the averaging mode, keeping the current average.
    ///
    /// # Panics
    ///
    /// Panics if the averaging parameters are out of range.
    pub fn set_averaging(&mut self, averaging: Averaging) {
        validate_averaging(averaging);
        self.config.averaging = averaging;
        self.coefficient = coefficient_for(self.config);
    }

    /// Window, transform and average one block of `fft_size` real samples.
    ///
    /// Allocation-free.
    ///
    /// # Panics
    ///
    /// Panics if `block.len() != fft_size`.
    pub fn process(&mut self, block: &[f32]) {
        assert_eq!(block.len(), self.config.fft_size, "block length mismatch");

        for ((slot, &x), &w) in self
            .buffer
            .iter_mut()
            .zip(block.iter())
            .zip(self.window.iter())
        {
            *slot = C32::new(x * w, 0.0);
        }
        self.fft.process_inplace(&mut self.buffer);

        let n = self.config.fft_size;
        let scale = 1.0 / self.window_gain;
        let blend = self.blend_coefficient();
        let keep = 1.0 - blend;
        for (k, (slot, z)) in self
            .magnitude
            .iter_mut()
            .zip(self.buffer.iter())
            .enumerate()
        {
            // Fold the negative-frequency half onto every bin except DC and,
            // for even lengths, Nyquist, which have no mirrored partner.
            let fold = if k == 0 || (n % 2 == 0 && k == n / 2) {
                1.0
            } else {
                2.0
            };
            let m = z.norm() * scale * fold;
            *slot = keep * *slot + blend * m;
        }

        let (reference, floor) = (self.config.reference, self.config.floor_db);
        for (slot, &m) in self.db.iter_mut().zip(self.magnitude.iter()) {
            *slot = linear_to_db(m, reference, floor);
        }
        self.frames += 1;
    }

    /// The averaged one-sided magnitude spectrum (linear amplitude).
    pub fn magnitude(&self) -> &[f32] {
        &self.magnitude
    }

    /// The averaged spectrum in dB relative to the configured reference.
    pub fn magnitude_db(&self) -> &[f32] {
        &self.db
    }

    /// The strongest bin, with a parabolically interpolated sub-bin estimate.
    ///
    /// Returns `None` before the first frame or for an all-zero spectrum.
    pub fn peak(&self) -> Option<SpectralPeak> {
        if self.frames == 0 {
            return None;
        }
        let k = peak_bin(&self.magnitude);
        if self.magnitude[k] <= 0.0 {
            return None;
        }
        Some(self.peak_at(k))
    }

    /// Interpolate a peak around an arbitrary bin, e.g. one returned by
    /// [`find_peaks`].
    ///
    /// # Panics
    ///
    /// Panics if `bin` is out of range.
    pub fn peak_at(&self, bin: usize) -> SpectralPeak {
        assert!(bin < self.magnitude.len(), "bin index out of range");
        let (offset, db) = parabolic_interpolate(&self.db, bin);
        let interpolated = bin as f32 + offset;
        SpectralPeak {
            bin: interpolated,
            frequency: interpolated * self.bin_width(),
            magnitude_db: db,
            magnitude: db_to_linear(db, self.config.reference),
        }
    }

    /// Clear the average and the frame counter.
    pub fn reset(&mut self) {
        for m in self.magnitude.iter_mut() {
            *m = 0.0;
        }
        for d in self.db.iter_mut() {
            *d = self.config.floor_db;
        }
        self.frames = 0;
    }

    fn blend_coefficient(&self) -> f32 {
        if self.frames == 0 {
            return 1.0;
        }
        match self.config.averaging {
            Averaging::Linear => 1.0 / (self.frames + 1) as f32,
            _ => self.coefficient,
        }
    }
}

impl core::fmt::Debug for RealtimeSpectrumAnalyzer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RealtimeSpectrumAnalyzer")
            .field("config", &self.config)
            .field("bins", &self.magnitude.len())
            .field("frames", &self.frames)
            .finish()
    }
}

fn validate_averaging(averaging: Averaging) {
    match averaging {
        Averaging::Exponential(a) => assert!(
            a > 0.0 && a <= 1.0,
            "exponential averaging must be in (0, 1]"
        ),
        Averaging::TimeConstant(tau) => {
            assert!(tau > 0.0, "averaging time constant must be positive")
        }
        Averaging::None | Averaging::Linear => {}
    }
}

fn coefficient_for(config: SpectrumConfig) -> f32 {
    match config.averaging {
        Averaging::None | Averaging::Linear => 1.0,
        Averaging::Exponential(a) => a,
        Averaging::TimeConstant(tau) => {
            let block = config.fft_size as f32 / config.sample_rate;
            1.0 - (-block / tau).exp()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averaging_smooths_towards_frames() {
        let mut sa = SpectrumAnalyzer::new(8, 1.0);
        sa.push(&[1.0; 8]);
        assert!(sa.spectrum().iter().all(|&x| (x - 1.0).abs() < 1e-6));
        // Half-weight new frame.
        let mut sa = SpectrumAnalyzer::new(8, 0.5);
        sa.push(&[0.0; 8]);
        sa.push(&[1.0; 8]);
        let expected = 0.5;
        assert!(sa.spectrum().iter().all(|&x| (x - expected).abs() < 1e-6));
    }

    #[test]
    fn find_peaks_locates_singletons() {
        let mut mag = vec![0.0f32; 32];
        mag[5] = 1.0;
        mag[20] = 0.6;
        let peaks = find_peaks(&mag, 0.5);
        assert_eq!(peaks, vec![5, 20]);
    }

    #[test]
    fn find_peaks_ignores_below_threshold() {
        let mut mag = vec![0.0f32; 16];
        mag[3] = 0.1;
        let peaks = find_peaks(&mag, 0.5); // cutoff = 0.05; 0.1 > 0.05
        assert_eq!(peaks, vec![3]);
        let none = find_peaks(&mag, 0.9); // cutoff = 0.09; 0.1 > 0.09 still
        assert_eq!(none, vec![3]);
    }

    #[test]
    fn dominant_frequency_maps_bins() {
        let mut mag = vec![0.0f32; 64];
        mag[10] = 1.0;
        let f = dominant_frequency(&mag, 6400.0, 64);
        assert!((f - 1000.0).abs() < 1e-3, "freq {f}");
    }

    const SR: f32 = 48_000.0;
    const N: usize = 1024;

    fn sine(bin: f32, amplitude: f32) -> Vec<f32> {
        (0..N)
            .map(|i| {
                let phase = core::f32::consts::TAU * bin * i as f32 / N as f32;
                amplitude * phase.sin()
            })
            .collect()
    }

    fn analyzer(averaging: Averaging) -> RealtimeSpectrumAnalyzer {
        RealtimeSpectrumAnalyzer::new(SpectrumConfig {
            fft_size: N,
            sample_rate: SR,
            averaging,
            ..SpectrumConfig::default()
        })
    }

    #[test]
    fn linear_to_db_matches_reference() {
        assert!(linear_to_db(1.0, 1.0, -120.0).abs() < 1e-6);
        assert!((linear_to_db(0.5, 1.0, -120.0) + 6.0206).abs() < 1e-3);
        assert!((linear_to_db(10.0, 1.0, -120.0) - 20.0).abs() < 1e-4);
        assert!((linear_to_db(2.0, 2.0, -120.0)).abs() < 1e-6);
        assert_eq!(linear_to_db(0.0, 1.0, -120.0), -120.0);
        assert_eq!(linear_to_db(-1.0, 1.0, -90.0), -90.0);
        assert_eq!(linear_to_db(1e-30, 1.0, -100.0), -100.0);
    }

    #[test]
    fn db_to_linear_roundtrips() {
        for m in [1e-4f32, 0.01, 0.25, 1.0, 4.0] {
            let db = linear_to_db(m, 1.0, -200.0);
            assert!(
                (db_to_linear(db, 1.0) - m).abs() < 1e-5 * m.max(1.0),
                "m {m}"
            );
        }
        assert!((db_to_linear(-6.0206, 1.0) - 0.5).abs() < 1e-4);
        assert!((db_to_linear(0.0, 2.5) - 2.5).abs() < 1e-6);
    }

    #[test]
    fn parabolic_interpolate_finds_vertex() {
        // y = -(x - 4.25)^2 + 3 sampled on integers: vertex at 4.25, value 3.
        let values: Vec<f32> = (0..9)
            .map(|i| {
                let d = i as f32 - 4.25;
                3.0 - d * d
            })
            .collect();
        let (offset, value) = parabolic_interpolate(&values, 4);
        assert!((offset - 0.25).abs() < 1e-4, "offset {offset}");
        assert!((value - 3.0).abs() < 1e-4, "value {value}");
    }

    #[test]
    fn parabolic_interpolate_handles_edges_and_flats() {
        let values = [1.0f32, 2.0, 1.0];
        assert_eq!(parabolic_interpolate(&values, 0), (0.0, 1.0));
        assert_eq!(parabolic_interpolate(&values, 2), (0.0, 1.0));
        assert_eq!(parabolic_interpolate(&values, 9), (0.0, 0.0));
        assert_eq!(parabolic_interpolate(&[5.0; 5], 2), (0.0, 5.0));
    }

    #[test]
    fn sine_peaks_at_expected_bin() {
        let mut sa = analyzer(Averaging::None);
        sa.process(&sine(64.0, 1.0));

        assert_eq!(sa.bins(), N / 2 + 1);
        assert_eq!(peak_bin(sa.magnitude()), 64);

        let peak = sa.peak().expect("peak");
        assert!((peak.bin - 64.0).abs() < 0.05, "bin {}", peak.bin);
        assert!(
            (peak.frequency - 3000.0).abs() < 0.5,
            "freq {}",
            peak.frequency
        );
        // Amplitude-corrected: a unit sine reads 0 dBFS.
        assert!(
            peak.magnitude_db.abs() < 0.05,
            "level {} dB",
            peak.magnitude_db
        );
        assert!(
            (peak.magnitude - 1.0).abs() < 0.01,
            "mag {}",
            peak.magnitude
        );
    }

    #[test]
    fn sine_level_tracks_amplitude() {
        let mut sa = analyzer(Averaging::None);
        sa.process(&sine(100.0, 0.5));
        let peak = sa.peak().expect("peak");
        assert!(
            (peak.magnitude_db + 6.0206).abs() < 0.1,
            "level {} dB",
            peak.magnitude_db
        );
        assert!(
            (peak.magnitude - 0.5).abs() < 0.01,
            "mag {}",
            peak.magnitude
        );
    }

    #[test]
    fn interpolation_resolves_sub_bin_frequency() {
        for offset in [-0.4f32, -0.25, 0.0, 0.25, 0.4] {
            let bin = 100.0 + offset;
            let mut sa = analyzer(Averaging::None);
            sa.process(&sine(bin, 1.0));
            let peak = sa.peak().expect("peak");
            assert!(
                (peak.bin - bin).abs() < 0.06,
                "offset {offset}: got bin {}",
                peak.bin
            );
            let expected_hz = bin * SR / N as f32;
            assert!(
                (peak.frequency - expected_hz).abs() < 3.0,
                "offset {offset}: got {} Hz, want {expected_hz}",
                peak.frequency
            );
        }
    }

    #[test]
    fn no_averaging_replaces_previous_frame() {
        let mut sa = analyzer(Averaging::None);
        sa.process(&sine(64.0, 1.0));
        sa.process(&sine(64.0, 0.25));
        let peak = sa.peak().expect("peak");
        assert!(
            (peak.magnitude - 0.25).abs() < 0.01,
            "mag {}",
            peak.magnitude
        );
    }

    #[test]
    fn exponential_averaging_converges() {
        let mut sa = analyzer(Averaging::Exponential(0.2));
        let quiet = sine(64.0, 0.1);
        let loud = sine(64.0, 1.0);

        sa.process(&quiet);
        let seeded = sa.magnitude()[64];
        assert!((seeded - 0.1).abs() < 0.01, "seed {seeded}");

        let mut previous = seeded;
        for _ in 0..3 {
            sa.process(&loud);
            let now = sa.magnitude()[64];
            assert!(now > previous, "average must rise: {previous} -> {now}");
            assert!(now < 1.0, "average must not overshoot: {now}");
            previous = now;
        }
        for _ in 0..120 {
            sa.process(&loud);
        }
        let settled = sa.magnitude()[64];
        assert!((settled - 1.0).abs() < 0.01, "settled {settled}");
        assert_eq!(sa.frames(), 124);
    }

    #[test]
    fn time_constant_averaging_matches_exponential() {
        // One block is N/SR seconds; a tau of that length gives 1 - 1/e.
        let tau = N as f32 / SR;
        let mut sa = analyzer(Averaging::TimeConstant(tau));
        let expected = 1.0 - (-1.0f32).exp();

        sa.process(&vec![0.0f32; N]);
        sa.process(&sine(64.0, 1.0));
        let got = sa.magnitude()[64];
        assert!((got - expected).abs() < 0.02, "got {got}, want {expected}");
    }

    #[test]
    fn linear_averaging_is_a_cumulative_mean() {
        let mut sa = analyzer(Averaging::Linear);
        sa.process(&sine(64.0, 1.0));
        sa.process(&vec![0.0f32; N]);
        assert!(
            (sa.magnitude()[64] - 0.5).abs() < 0.01,
            "mean {}",
            sa.magnitude()[64]
        );
        sa.process(&vec![0.0f32; N]);
        sa.process(&vec![0.0f32; N]);
        assert!(
            (sa.magnitude()[64] - 0.25).abs() < 0.01,
            "mean {}",
            sa.magnitude()[64]
        );
    }

    #[test]
    fn reset_clears_average_and_reseeds() {
        let mut sa = analyzer(Averaging::Exponential(0.1));
        sa.process(&sine(64.0, 1.0));
        sa.reset();
        assert_eq!(sa.frames(), 0);
        assert!(sa.peak().is_none());
        assert!(sa.magnitude().iter().all(|&m| m == 0.0));
        assert!(sa.magnitude_db().iter().all(|&d| d == DEFAULT_DB_FLOOR));

        sa.process(&sine(64.0, 0.5));
        assert!((sa.magnitude()[64] - 0.5).abs() < 0.01);
    }

    #[test]
    fn silence_stays_at_the_db_floor() {
        let mut sa = analyzer(Averaging::None);
        sa.process(&vec![0.0f32; N]);
        assert!(sa.magnitude_db().iter().all(|&d| d == DEFAULT_DB_FLOOR));
        assert!(sa.peak().is_none());
    }

    #[test]
    fn set_averaging_switches_mode() {
        let mut sa = analyzer(Averaging::Exponential(0.5));
        sa.process(&sine(64.0, 1.0));
        sa.set_averaging(Averaging::None);
        assert_eq!(sa.config().averaging, Averaging::None);
        sa.process(&sine(64.0, 0.125));
        assert!((sa.magnitude()[64] - 0.125).abs() < 0.01);
    }

    #[test]
    fn geometry_helpers_are_consistent() {
        let sa = RealtimeSpectrumAnalyzer::with_size(512, 44_100.0);
        assert_eq!(sa.fft_size(), 512);
        assert_eq!(sa.bins(), 257);
        assert!((sa.bin_width() - 44_100.0 / 512.0).abs() < 1e-3);
        assert!((sa.bin_frequency(10) - 10.0 * sa.bin_width()).abs() < 1e-3);
        assert_eq!(sa.frames(), 0);
    }

    #[test]
    fn peak_at_interpolates_secondary_tones() {
        let mut sa = analyzer(Averaging::None);
        let block: Vec<f32> = sine(64.0, 1.0)
            .iter()
            .zip(sine(200.25, 0.3).iter())
            .map(|(a, b)| a + b)
            .collect();
        sa.process(&block);

        let peaks = find_peaks(sa.magnitude(), 0.1);
        assert!(peaks.contains(&64), "peaks {peaks:?}");
        let secondary = peaks
            .iter()
            .copied()
            .find(|&k| (198..=202).contains(&k))
            .expect("secondary peak");
        let interpolated = sa.peak_at(secondary);
        assert!(
            (interpolated.bin - 200.25).abs() < 0.1,
            "bin {}",
            interpolated.bin
        );
    }

    #[test]
    #[should_panic(expected = "block length mismatch")]
    fn process_rejects_wrong_block_size() {
        let mut sa = analyzer(Averaging::None);
        sa.process(&[0.0; 16]);
    }

    #[test]
    #[should_panic(expected = "exponential averaging must be in (0, 1]")]
    fn invalid_averaging_is_rejected() {
        let _ = analyzer(Averaging::Exponential(0.0));
    }
}
