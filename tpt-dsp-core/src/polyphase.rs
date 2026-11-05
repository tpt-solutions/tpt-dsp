//! Polyphase rational resampling and Farrow fractional delay.
//!
//! [`RationalResampler`] converts a stream by the rational factor `L/M`
//! (interpolate by `L`, low-pass, decimate by `M`) using a polyphase FIR. Only
//! the output samples that are kept are computed, so the cost is one
//! `taps_per_phase`-tap dot product per output sample.
//!
//! [`FarrowDelay`] delays a signal by a fractional number of samples using
//! cubic Lagrange interpolation written in Farrow form: four fixed polynomial
//! coefficient sets, each evaluated by Horner's rule in the fractional part.
//! It is allocation-free.

// `no_std` builds have no inherent float math; see `windows.rs`.
#[allow(unused_imports)]
use num_traits::Float;

#[cfg(feature = "alloc")]
use crate::windows::windowed_symmetric_f64;
#[cfg(feature = "alloc")]
use crate::WindowType;

/// Kaiser shape parameter for the resampler's prototype filter (about 60 dB
/// stopband).
#[cfg(feature = "alloc")]
const KAISER_BETA: f32 = 6.0;

/// Streaming rational resampler by `up / down`.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub struct RationalResampler {
    up: usize,
    down: usize,
    taps_per_phase: usize,
    /// Prototype filter scaled by `up`; phase `p`, tap `k` is `h[p + k·up]`.
    h: alloc::vec::Vec<f32>,
    history: alloc::vec::Vec<f32>,
    n_in: usize,
    m_out: usize,
}

#[cfg(feature = "alloc")]
impl RationalResampler {
    /// Create a resampler by `up / down`.
    ///
    /// `taps_per_phase` (clamped to at least 2) sets the quality: 8–16 is a
    /// reasonable range. Zero factors are clamped to 1.
    pub fn new(up: usize, down: usize, taps_per_phase: usize) -> Self {
        let up = up.max(1);
        let down = down.max(1);
        let k = taps_per_phase.max(2);
        let n = up * k;
        // Cutoff at the lower of the two Nyquist limits, in cycles per
        // upsampled sample.
        let fc = 0.5 / up.max(down) as f64;
        let mut window = alloc::vec![0.0f64; n];
        windowed_symmetric_f64(WindowType::Kaiser(KAISER_BETA), n, &mut window);
        let centre = (n as f64 - 1.0) / 2.0;
        let h: alloc::vec::Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 - centre;
                let sinc = if t == 0.0 {
                    2.0 * fc
                } else {
                    (2.0 * core::f64::consts::PI * fc * t).sin() / (core::f64::consts::PI * t)
                };
                (up as f64 * sinc * window[i]) as f32
            })
            .collect();
        let hist_len = k + up.div_ceil(down) + 2;
        Self {
            up,
            down,
            taps_per_phase: k,
            h,
            history: alloc::vec![0.0; hist_len],
            n_in: 0,
            m_out: 0,
        }
    }

    /// The rational factor `(up, down)`.
    pub fn ratio(&self) -> (usize, usize) {
        (self.up, self.down)
    }

    /// Filter length per phase.
    pub fn taps_per_phase(&self) -> usize {
        self.taps_per_phase
    }

    /// Group delay in input samples (the prototype's linear-phase delay).
    pub fn latency_input_samples(&self) -> f32 {
        (self.h.len() as f32 - 1.0) / 2.0 / self.up as f32
    }

    /// Clear the history.
    pub fn reset(&mut self) {
        self.history.iter_mut().for_each(|v| *v = 0.0);
        self.n_in = 0;
        self.m_out = 0;
    }

    /// Feed input samples; resampled output is appended to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut alloc::vec::Vec<f32>) {
        let hl = self.history.len();
        for &x in input {
            let x = if x.is_finite() { x } else { 0.0 };
            self.history[self.n_in % hl] = x;
            self.n_in += 1;
            // Emit every output whose base input sample has arrived.
            loop {
                let t = self.m_out * self.down;
                let i = t / self.up;
                if i >= self.n_in {
                    break;
                }
                let phase = t % self.up;
                let mut acc = 0.0f32;
                for k in 0..self.taps_per_phase {
                    // Input index i - k; anything before the stream start is 0.
                    let Some(j) = i.checked_sub(k) else { break };
                    acc += self.h[phase + k * self.up] * self.history[j % hl];
                }
                out.push(acc);
                self.m_out += 1;
            }
        }
    }

    /// Resample a whole buffer in one call (state carries over between calls).
    pub fn resample(&mut self, input: &[f32]) -> alloc::vec::Vec<f32> {
        let mut out = alloc::vec::Vec::with_capacity(input.len() * self.up / self.down + 1);
        self.process(input, &mut out);
        out
    }
}

/// Cubic Lagrange fractional delay in Farrow form.
///
/// Output `y[n] = x[n − d]` for a delay `d` (in samples, `2 ≤ d ≤ 6`). The
/// interpolation uses four neighbouring samples around the fractional
/// position, so the delay must be at least 2 samples.
#[derive(Debug, Clone)]
pub struct FarrowDelay {
    ring: [f32; 8],
    n: usize,
    delay: f32,
}

/// Farrow coefficient polynomials, one per tap (positions −1, 0, 1, 2 around
/// the fractional point), in increasing powers of the fraction `μ`.
const FARROW: [[f32; 4]; 4] = [
    [0.0, -1.0 / 3.0, 0.5, -1.0 / 6.0],
    [1.0, -0.5, -1.0, 0.5],
    [0.0, 1.0, 0.5, -0.5],
    [0.0, -1.0 / 6.0, 0.0, 1.0 / 6.0],
];

impl FarrowDelay {
    /// Create a delay line with delay `d` samples (clamped to `2..=6`).
    pub fn new(delay: f32) -> Self {
        let mut f = Self {
            ring: [0.0; 8],
            n: 0,
            delay: 2.0,
        };
        f.set_delay(delay);
        f
    }

    /// Set the delay in samples, clamped to `2..=6`. NaN is ignored.
    pub fn set_delay(&mut self, delay: f32) {
        if delay.is_finite() {
            self.delay = delay.clamp(2.0, 6.0);
        }
    }

    /// Clear the history.
    pub fn reset(&mut self) {
        self.ring = [0.0; 8];
        self.n = 0;
    }

    fn sample(&self, index: isize) -> f32 {
        // `index` is absolute; the ring holds the most recent 8 samples.
        let last = self.n as isize - 1;
        if index < 0 || index < last - 7 || index > last {
            return 0.0;
        }
        self.ring[(index as usize) % 8]
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        self.ring[self.n % 8] = x;
        self.n += 1;

        // Fractional read position in absolute sample indices.
        let pos = (self.n as f32 - 1.0) - self.delay;
        let base = pos.floor();
        let mu = pos - base;
        let b = base as isize;

        let mut y = 0.0f32;
        for (k, coeffs) in FARROW.iter().enumerate() {
            // Tap k sits at position b + (k − 1).
            let s = self.sample(b + k as isize - 1);
            // Horner in μ.
            let poly = ((coeffs[3] * mu + coeffs[2]) * mu + coeffs[1]) * mu + coeffs[0];
            y += poly * s;
        }
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    fn sine(f: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (i as f32 * f * core::f32::consts::TAU / FS).sin())
            .collect()
    }

    fn zero_crossing_rate(x: &[f32], fs: f32) -> f32 {
        let mut c = 0usize;
        for w in x.windows(2) {
            if (w[0] < 0.0) != (w[1] < 0.0) {
                c += 1;
            }
        }
        // Two crossings per cycle.
        c as f32 / 2.0 / (x.len() as f32 / fs)
    }

    #[test]
    fn output_length_matches_the_ratio() {
        let mut r = RationalResampler::new(3, 2, 12);
        let y = r.resample(&sine(1000.0, 4800));
        // 4800 in at 3/2 → 7200 out, within a sample of rounding.
        assert!((y.len() as i64 - 7200).abs() <= 2, "len {}", y.len());
    }

    #[test]
    fn sine_frequency_is_preserved_across_rates() {
        // 48 kHz → 32 kHz (L/M = 2/3): a 1 kHz tone stays 1 kHz.
        let mut r = RationalResampler::new(2, 3, 12);
        let y = r.resample(&sine(1000.0, 48_000));
        let fs_out = 32_000.0;
        let f = zero_crossing_rate(&y[2000..], fs_out);
        assert!((f - 1000.0).abs() < 10.0, "frequency {f}");
    }

    #[test]
    fn upsample_then_downsample_is_near_identity() {
        let x = sine(500.0, 9600);
        let mut up = RationalResampler::new(4, 1, 16);
        let mut down = RationalResampler::new(1, 4, 16);
        let mid = up.resample(&x);
        let back = down.resample(&mid);
        // The upsampler's delay is in input samples (= output samples here).
        // The decimator's delay is in its own input samples, which are four
        // times denser than its output, so divide by the decimation factor.
        let total = up.latency_input_samples() + down.latency_input_samples() / 4.0;
        assert!((total - 9.75).abs() < 0.01, "total delay {total}");
        let delay = total.round() as usize;
        let mut err = 0.0f32;
        let start = 400;
        for (i, &xi) in x.iter().enumerate().take(x.len() - 400).skip(start) {
            let j = i + delay;
            if j < back.len() {
                err = err.max((back[j] - xi).abs());
            }
        }
        assert!(err < 0.03, "round-trip error {err}");
    }

    #[test]
    fn streaming_matches_one_shot() {
        let x = sine(700.0, 2000);
        let mut a = RationalResampler::new(3, 4, 10);
        let whole = a.resample(&x);
        let mut b = RationalResampler::new(3, 4, 10);
        let mut parts = Vec::new();
        for chunk in x.chunks(37) {
            b.process(chunk, &mut parts);
        }
        assert_eq!(whole.len(), parts.len());
        for (p, q) in whole.iter().zip(parts.iter()) {
            assert!((p - q).abs() < 1e-6);
        }
    }

    #[test]
    fn farrow_integer_delay_is_exact() {
        let mut d = FarrowDelay::new(4.0);
        let x = sine(300.0, 200);
        let y: Vec<f32> = x.iter().map(|&v| d.tick(v)).collect();
        for n in 10..200 {
            assert!((y[n] - x[n - 4]).abs() < 1e-5, "n {n}");
        }
    }

    #[test]
    fn farrow_fractional_delay_is_accurate_at_low_frequency() {
        let mut d = FarrowDelay::new(3.5);
        let f = 200.0;
        let x = sine(f, 400);
        let y: Vec<f32> = x.iter().map(|&v| d.tick(v)).collect();
        for (n, &yn) in y.iter().enumerate().take(400).skip(20) {
            let t = n as f32 - 3.5;
            let want = (t * f * core::f32::consts::TAU / FS).sin();
            assert!((yn - want).abs() < 2e-3, "n {n}: {yn} vs {want}");
        }
    }

    #[test]
    fn farrow_stays_finite_on_nan() {
        let mut d = FarrowDelay::new(2.5);
        for i in 0..100 {
            let x = if i == 50 { f32::NAN } else { 0.1 };
            assert!(d.tick(x).is_finite());
        }
    }
}
