//! Time-series statistics: moving averages, exponential smoothing and
//! outlier detection for noisy sensor / telemetry streams.
//!
//! All estimators keep their window state in pre-allocated buffers and update
//! in O(1), so they are suitable for continuous real-time streams.

/// A fixed-window simple moving average.
///
/// Keeps the last `window_size` samples in a ring; [`push`](Self::push)
/// returns the mean of the window in constant time.
#[derive(Debug, Clone)]
pub struct MovingAverage {
    window: Vec<f32>,
    pos: usize,
    filled: usize,
    sum: f32,
}

impl MovingAverage {
    /// Create an averager over `window_size` samples.
    ///
    /// # Panics
    ///
    /// Panics if `window_size` is zero.
    pub fn new(window_size: usize) -> Self {
        assert!(window_size > 0, "window size must be positive");
        Self {
            window: vec![0.0; window_size],
            pos: 0,
            filled: 0,
            sum: 0.0,
        }
    }

    /// Window length.
    pub fn window_size(&self) -> usize {
        self.window.len()
    }

    /// Push a sample and return the current window mean.
    pub fn push(&mut self, x: f32) -> f32 {
        let n = self.window.len();
        if self.filled == n {
            self.sum -= self.window[self.pos];
        } else {
            self.filled += 1;
        }
        self.window[self.pos] = x;
        self.sum += x;
        self.pos = (self.pos + 1) % n;
        self.sum / self.filled as f32
    }

    /// The most recent mean (0.0 before any sample is pushed).
    pub fn value(&self) -> f32 {
        if self.filled == 0 {
            0.0
        } else {
            self.sum / self.filled as f32
        }
    }
}

/// A numerically stable running mean over an unbounded stream.
///
/// Uses Welford's update so it never overflows and needs only O(1) state.
#[derive(Debug, Clone)]
pub struct RunningMean {
    count: u64,
    mean: f64,
}

impl RunningMean {
    /// Create an empty running mean.
    pub fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
        }
    }

    /// Add a sample and return the updated mean.
    pub fn push(&mut self, x: f64) -> f64 {
        self.count += 1;
        self.mean += (x - self.mean) / self.count as f64;
        self.mean
    }

    /// Number of samples seen.
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Current mean.
    pub fn value(&self) -> f64 {
        self.mean
    }
}

impl Default for RunningMean {
    fn default() -> Self {
        Self::new()
    }
}

/// Exponential moving average (single-pole low-pass).
#[derive(Debug, Clone)]
pub struct Ema {
    alpha: f32,
    value: f32,
    primed: bool,
}

impl Ema {
    /// Create an EMA with smoothing factor `alpha` in `(0, 1]`.
    ///
    /// Larger `alpha` weights recent samples more heavily.
    pub fn new(alpha: f32) -> Self {
        assert!((0.0..=1.0).contains(&alpha), "alpha must be in (0, 1]");
        Self {
            alpha,
            value: 0.0,
            primed: false,
        }
    }

    /// Push a sample and return the smoothed value.
    pub fn push(&mut self, x: f32) -> f32 {
        if !self.primed {
            self.value = x;
            self.primed = true;
        } else {
            self.value += self.alpha * (x - self.value);
        }
        self.value
    }

    /// Current smoothed value.
    pub fn value(&self) -> f32 {
        self.value
    }
}

/// A sliding-window outlier detector based on Median Absolute Deviation.
///
/// A sample is flagged when it lies more than `sigma` median deviations away
/// from the window median. MAD is robust to the outliers themselves, so a
/// burst of bad samples does not mask further ones.
#[derive(Debug, Clone)]
pub struct OutlierDetector {
    window: Vec<f32>,
    pos: usize,
    filled: usize,
    sigma: f32,
    min_samples: usize,
}

impl OutlierDetector {
    /// Create a detector over `window_size` samples.
    ///
    /// * `sigma` — how many median absolute deviations a value may deviate
    ///   before it is flagged (typically 3–5).
    pub fn new(window_size: usize, sigma: f32) -> Self {
        assert!(window_size >= 4, "window must be at least 4 samples");
        Self {
            window: vec![0.0; window_size],
            pos: 0,
            filled: 0,
            sigma,
            min_samples: window_size,
        }
    }

    /// Push a sample and return `true` if it is an outlier.
    pub fn push(&mut self, x: f32) -> bool {
        let n = self.window.len();
        if self.filled == n {
            self.window[self.pos] = x;
        } else {
            self.window[self.filled] = x;
            self.filled += 1;
        }
        self.pos = (self.pos + 1) % n;

        if self.filled < self.min_samples {
            return false;
        }

        let mut sorted: Vec<f32> = self.window[..self.filled].to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = sorted[sorted.len() / 2];
        let mut devs: Vec<f32> = sorted.iter().map(|v| (v - median).abs()).collect();
        devs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = devs[devs.len() / 2].max(1e-9);
        (x - median).abs() > self.sigma * 1.4826 * mad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_average_tracks_mean() {
        let mut ma = MovingAverage::new(4);
        assert_eq!(ma.push(2.0), 2.0);
        assert_eq!(ma.push(4.0), 3.0);
        assert_eq!(ma.push(6.0), 4.0);
        assert_eq!(ma.push(8.0), 5.0);
        // Window now holds [2,4,6,8]; push a new value replacing 2.
        assert_eq!(ma.push(10.0), 7.0);
    }

    #[test]
    fn running_mean_matches_reference() {
        let mut rm = RunningMean::new();
        let data: Vec<f64> = (0..1000).map(|i| (i as f64 * 1.3).sin()).collect();
        let mut ref_sum = 0.0;
        for (i, &x) in data.iter().enumerate() {
            ref_sum += x;
            assert!((rm.push(x) - ref_sum / (i as f64 + 1.0)).abs() < 1e-9);
        }
    }

    #[test]
    fn ema_converges() {
        let mut ema = Ema::new(0.5);
        for _ in 0..100 {
            ema.push(1.0);
        }
        assert!((ema.value() - 1.0).abs() < 1e-3);
    }

    #[test]
    fn ema_steps_toward_new_value() {
        let mut ema = Ema::new(0.5);
        ema.push(0.0);
        let v = ema.push(1.0);
        assert!((v - 0.5).abs() < 1e-6);
    }

    #[test]
    fn outlier_detector_flags_spike() {
        let mut od = OutlierDetector::new(16, 4.0);
        for _ in 0..20 {
            assert!(!od.push(1.01));
        }
        // A massive spike should be flagged.
        assert!(od.push(100.0));
        // Flush the spike out of the window, then a value at the median is
        // not flagged.
        for _ in 0..16 {
            od.push(1.01);
        }
        assert!(!od.push(1.01));
    }
}
