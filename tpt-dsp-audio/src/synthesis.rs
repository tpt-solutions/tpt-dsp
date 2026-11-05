//! Band-limited oscillators, noise sources and dither.
//!
//! - [`PolyBlepOscillator`]: sawtooth and square waves with PolyBLEP
//!   anti-aliasing (a polynomial correction applied at each discontinuity).
//!   Much cleaner than the naive [`Oscillator`] at high pitches.
//! - [`PinkNoise`]: an approximately 1/f spectrum (Paul Kellet's filter).
//! - [`TpdfDither`]: triangular-PDF dither for requantising to a lower bit
//!   depth without correlated distortion.
//! - [`NoiseSource`]: a deterministic white-noise generator underpinning the
//!   two above, so results are reproducible in tests.
//!
//! Oversampling is not provided; PolyBLEP suppresses most aliasing but not all.

use crate::util::{is_valid_sample_rate, sanitize_frequency, wrap_phase};

/// Deterministic white-noise source (xorshift32).
#[derive(Debug, Clone)]
pub struct NoiseSource {
    state: u32,
}

impl NoiseSource {
    /// Create a source from a seed. A zero seed is replaced with a fixed
    /// non-zero constant, since xorshift cannot leave state zero.
    pub fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut s = self.state;
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.state = s;
        s
    }

    /// Uniform sample in `[-1, 1)`.
    pub fn next_uniform(&mut self) -> f32 {
        (self.next_u32() as f32 / 2_147_483_648.0) - 1.0
    }
}

/// PolyBLEP residual for a phase `t` in `[0, 1)` with per-sample increment `dt`.
///
/// Returns a correction that is zero away from a discontinuity and smooths the
/// step within one sample on either side of it.
pub fn poly_blep(t: f32, dt: f32) -> f32 {
    if dt <= 0.0 {
        0.0
    } else if t < dt {
        let u = t / dt;
        2.0 * u - u * u - 1.0
    } else if t > 1.0 - dt {
        let u = (t - 1.0) / dt;
        u * u + 2.0 * u + 1.0
    } else {
        0.0
    }
}

/// Which band-limited shape a [`PolyBlepOscillator`] produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlepShape {
    /// Rising sawtooth in `[-1, 1]`.
    Sawtooth,
    /// 50% duty square in `[-1, 1]`.
    Square,
}

/// Anti-aliased sawtooth or square oscillator.
#[derive(Debug, Clone)]
pub struct PolyBlepOscillator {
    shape: BlepShape,
    sample_rate: f32,
    frequency: f32,
    phase: f32,
}

impl PolyBlepOscillator {
    /// Create an oscillator. Non-finite or negative frequencies become 0; an
    /// invalid sample rate falls back to 48 kHz.
    pub fn new(sample_rate: f32, frequency: f32, shape: BlepShape) -> Self {
        Self {
            shape,
            sample_rate: if is_valid_sample_rate(sample_rate) {
                sample_rate
            } else {
                48_000.0
            },
            frequency: sanitize_frequency(frequency),
            phase: 0.0,
        }
    }

    /// Set the frequency in Hz (non-finite or negative become 0).
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = sanitize_frequency(frequency);
    }

    /// Set the sample rate in Hz (invalid values are ignored).
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if is_valid_sample_rate(sample_rate) {
            self.sample_rate = sample_rate;
        }
    }

    /// Reset the phase to zero.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Advance one sample and return it.
    pub fn tick(&mut self) -> f32 {
        // Per-sample increment; clamp so a very high frequency can't push the
        // PolyBLEP window past a whole cycle.
        let dt = (self.frequency / self.sample_rate).clamp(0.0, 0.5);
        let t = self.phase;
        let y = match self.shape {
            BlepShape::Sawtooth => 2.0 * t - 1.0 - poly_blep(t, dt),
            BlepShape::Square => {
                let mut y = if t < 0.5 { 1.0 } else { -1.0 };
                y += poly_blep(t, dt);
                y -= poly_blep(wrap_phase(t + 0.5), dt);
                y
            }
        };
        self.phase = wrap_phase(self.phase + dt);
        y
    }

    /// Fill `out` with successive samples.
    pub fn process(&mut self, out: &mut [f32]) {
        for s in out.iter_mut() {
            *s = self.tick();
        }
    }
}

/// Approximately 1/f ("pink") noise, roughly ±1 in amplitude.
#[derive(Debug, Clone)]
pub struct PinkNoise {
    source: NoiseSource,
    b: [f32; 7],
}

impl PinkNoise {
    /// Create a pink-noise generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            source: NoiseSource::new(seed),
            b: [0.0; 7],
        }
    }

    /// Produce one sample.
    pub fn tick(&mut self) -> f32 {
        let w = self.source.next_uniform();
        let b = &mut self.b;
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.153852;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115926;
        out * 0.11
    }
}

/// Triangular-PDF dither for requantisation.
#[derive(Debug, Clone)]
pub struct TpdfDither {
    source: NoiseSource,
}

impl TpdfDither {
    /// Create a dither generator with the given seed.
    pub fn new(seed: u32) -> Self {
        Self {
            source: NoiseSource::new(seed),
        }
    }

    /// Quantise `x` (nominally in `[-1, 1]`) to `bits` of signed resolution
    /// (`2 ≤ bits ≤ 31`), adding triangular dither of ±1 LSB before rounding.
    /// Out-of-range bit depths are clamped.
    pub fn quantize(&mut self, x: f32, bits: u32) -> f32 {
        let bits = bits.clamp(2, 31);
        let scale = 2.0f32.powi(bits as i32 - 1);
        let x = if x.is_finite() { x } else { 0.0 };
        // Sum of two uniforms in [-½, ½] LSB is a triangle in [-1, 1] LSB.
        let d = 0.5 * (self.source.next_uniform() + self.source.next_uniform()) / scale;
        ((x + d) * scale).round() / scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn poly_blep_is_zero_away_from_discontinuities() {
        assert_eq!(poly_blep(0.5, 0.01), 0.0);
        assert_eq!(poly_blep(0.3, 0.0), 0.0);
    }

    #[test]
    fn saw_stays_bounded_at_high_pitch() {
        let mut o = PolyBlepOscillator::new(SR, 9_000.0, BlepShape::Sawtooth);
        for _ in 0..4800 {
            let y = o.tick();
            assert!(y.is_finite() && y.abs() <= 1.1, "{y}");
        }
    }

    #[test]
    fn saw_has_less_aliasing_than_naive_saw() {
        // Compare energy above Nyquist-ish content by counting how far the
        // waveform's sample-to-sample jumps exceed the naive saw's jump.
        let f = 5000.0;
        let mut blep = PolyBlepOscillator::new(SR, f, BlepShape::Sawtooth);
        let mut naive = crate::Oscillator::with_waveform(SR, f, crate::Waveform::Sawtooth);
        let mut max_blep = 0.0f32;
        let mut max_naive = 0.0f32;
        let mut prev_b = blep.tick();
        let mut prev_n = naive.tick();
        for _ in 0..4800 {
            let b = blep.tick();
            let n = naive.tick();
            max_blep = max_blep.max((b - prev_b).abs());
            max_naive = max_naive.max((n - prev_n).abs());
            prev_b = b;
            prev_n = n;
        }
        assert!(
            max_blep < max_naive * 0.8,
            "blep {max_blep} naive {max_naive}"
        );
    }

    #[test]
    fn square_is_balanced() {
        let mut o = PolyBlepOscillator::new(SR, 220.0, BlepShape::Square);
        let mut sum = 0.0f32;
        let n = 48_000;
        for _ in 0..n {
            sum += o.tick();
        }
        assert!((sum / n as f32).abs() < 0.01, "dc {}", sum / n as f32);
    }

    #[test]
    fn pink_noise_is_bounded_and_zero_mean() {
        let mut p = PinkNoise::new(12345);
        let mut sum = 0.0f64;
        let n = 96_000;
        for _ in 0..n {
            let y = p.tick();
            assert!(y.is_finite() && y.abs() < 2.0, "{y}");
            sum += f64::from(y);
        }
        assert!((sum / n as f64).abs() < 0.05, "mean {}", sum / n as f64);
    }

    #[test]
    fn dither_output_lies_on_the_quantisation_grid() {
        let mut d = TpdfDither::new(7);
        let bits = 8;
        let step = 2.0f32.powi(-(bits as i32 - 1));
        for i in 0..1000 {
            let x = (i as f32 * 0.01).sin();
            let q = d.quantize(x, bits);
            let k = q / step;
            assert!((k - k.round()).abs() < 1e-3, "off grid: {q}");
        }
    }

    #[test]
    fn dither_error_is_roughly_unbiased() {
        let mut d = TpdfDither::new(99);
        let x = 0.3331f32;
        let n = 20_000;
        let mut sum = 0.0f64;
        for _ in 0..n {
            sum += f64::from(d.quantize(x, 8) - x);
        }
        assert!((sum / n as f64).abs() < 0.002, "bias {}", sum / n as f64);
    }

    #[test]
    fn noise_source_is_deterministic_and_nonzero_seed_safe() {
        let mut a = NoiseSource::new(0);
        let mut b = NoiseSource::new(0);
        for _ in 0..100 {
            assert_eq!(a.next_uniform(), b.next_uniform());
        }
    }
}
