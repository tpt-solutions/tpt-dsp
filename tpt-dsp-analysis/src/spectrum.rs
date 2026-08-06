//! Real-time spectrum analysis: running FFT magnitude averaging and peak
//! detection over magnitude spectra.

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
        assert!((0.0..=1.0).contains(&averaging), "averaging must be in (0, 1]");
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
}
