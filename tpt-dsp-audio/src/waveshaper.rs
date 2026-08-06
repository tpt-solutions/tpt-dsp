//! Waveshaping / distortion effect.
//!
//! Maps an input sample through a nonlinear transfer function (after an
//! optional drive gain) and blends the result with the dry signal by a `mix`
//! amount. All transfer functions are bounded so the output stays in a sane
//! range for downstream processing.

/// The transfer function applied by a [`Waveshaper`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Curve {
    /// Soft saturation: `tanh(drive·x)`.
    Tanh,
    /// Hard clip to `[-1, 1]` after drive.
    HardClip,
    /// Cubic: `x - x³/3`, a gentle even/odd mix.
    Cubic,
    /// Arbitrary polynomial `c0 + c1·x + c2·x² + c3·x³`.
    Polynomial([f32; 4]),
}

/// A waveshaping distortion effect.
#[derive(Debug, Clone)]
pub struct Waveshaper {
    drive: f32,
    mix: f32,
    curve: Curve,
}

impl Waveshaper {
    /// Create a shaper with the given curve, drive gain and wet/dry mix
    /// (`0` = dry, `1` = fully shaped).
    pub fn new(curve: Curve, drive: f32, mix: f32) -> Self {
        Self {
            drive: drive.max(0.0),
            mix: mix.clamp(0.0, 1.0),
            curve,
        }
    }

    /// Set the drive (pre-gain) amount.
    pub fn set_drive(&mut self, drive: f32) {
        self.drive = drive.max(0.0);
    }

    /// Set the wet/dry mix (`0` = dry, `1` = wet).
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Replace the transfer curve.
    pub fn set_curve(&mut self, curve: Curve) {
        self.curve = curve;
    }

    fn shape(&self, x: f32) -> f32 {
        match self.curve {
            Curve::Tanh => (self.drive * x).tanh(),
            Curve::HardClip => (self.drive * x).clamp(-1.0, 1.0),
            Curve::Cubic => {
                let d = self.drive * x;
                (d - d * d * d / 3.0).clamp(-1.0, 1.0)
            }
            Curve::Polynomial(c) => {
                let d = self.drive * x;
                (c[0] + c[1] * d + c[2] * d * d + c[3] * d * d * d).clamp(-1.0, 1.0)
            }
        }
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let wet = self.shape(x);
        x * (1.0 - self.mix) + wet * self.mix
    }

    /// Process a whole block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tanh_soft_clips_large_input() {
        let mut ws = Waveshaper::new(Curve::Tanh, 10.0, 1.0);
        let out = ws.tick(1.0);
        assert!(out <= 1.0 && out > 0.9, "out {out}");
    }

    #[test]
    fn hard_clip_bounds_output() {
        let mut ws = Waveshaper::new(Curve::HardClip, 50.0, 1.0);
        assert!((ws.tick(1.0) - 1.0).abs() < 1e-6);
        assert!((ws.tick(-1.0) + 1.0).abs() < 1e-6);
    }

    #[test]
    fn zero_mix_passthrough() {
        let mut ws = Waveshaper::new(Curve::Cubic, 10.0, 0.0);
        assert_eq!(ws.tick(0.5), 0.5);
    }

    #[test]
    fn unity_gain_at_small_signal() {
        let mut ws = Waveshaper::new(Curve::Tanh, 1.0, 1.0);
        let out = ws.tick(0.01);
        assert!((out - 0.01).abs() < 1e-3);
    }
}
