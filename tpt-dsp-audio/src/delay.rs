//! Delay effect with feedback and wet/dry mix.
//!
//! A single-tap delay using a pre-allocated circular buffer. The delay time
//! is set in samples (or seconds at a known sample rate) and the feedback
//! gain controls how much of the delayed signal is fed back into the buffer,
//! producing repeating echoes. All state is fixed at construction, so
//! processing is allocation-free and real-time safe.

/// A feedback delay line.
pub struct Delay {
    buffer: Vec<f32>,
    pos: usize,
    delay_samples: usize,
    feedback: f32,
    mix: f32,
}

impl Delay {
    /// Create a delay with capacity for `max_delay_samples` of latency.
    ///
    /// # Panics
    ///
    /// Panics if `max_delay_samples` is zero.
    pub fn new(max_delay_samples: usize) -> Self {
        assert!(max_delay_samples > 0, "delay capacity must be positive");
        Self {
            buffer: vec![0.0; max_delay_samples],
            pos: 0,
            delay_samples: (max_delay_samples / 2).min(max_delay_samples - 1),
            feedback: 0.3,
            mix: 0.5,
        }
    }

    /// Set the delay time in samples (clamped to the buffer capacity).
    pub fn set_delay_samples(&mut self, samples: usize) {
        self.delay_samples = samples.min(self.buffer.len() - 1).max(1);
    }

    /// Set the delay time in seconds at the given sample rate.
    pub fn set_delay_seconds(&mut self, seconds: f32, sample_rate: f32) {
        self.set_delay_samples((seconds * sample_rate).round() as usize);
    }

    /// Set the feedback gain (`0` = single echo, near `1` = infinite repeat).
    pub fn set_feedback(&mut self, feedback: f32) {
        self.feedback = feedback.clamp(0.0, 0.99);
    }

    /// Set the wet/dry mix (`0` = dry, `1` = fully wet).
    pub fn set_mix(&mut self, mix: f32) {
        self.mix = mix.clamp(0.0, 1.0);
    }

    /// Clear the delay memory.
    pub fn reset(&mut self) {
        for s in self.buffer.iter_mut() {
            *s = 0.0;
        }
        self.pos = 0;
    }

    /// Process one sample: `out = dry·x + wet·delayed`.
    pub fn tick(&mut self, x: f32) -> f32 {
        let len = self.buffer.len();
        let read = (self.pos + len - self.delay_samples) % len;
        let delayed = self.buffer[read];
        let written = x + delayed * self.feedback;
        self.buffer[self.pos] = written;
        self.pos = (self.pos + 1) % len;
        x * (1.0 - self.mix) + delayed * self.mix
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
    fn silence_delayed_silence() {
        let mut d = Delay::new(4800);
        d.set_mix(1.0);
        for _ in 0..100 {
            assert_eq!(d.tick(0.0), 0.0);
        }
    }

    #[test]
    fn impulse_echoes_after_delay() {
        let mut d = Delay::new(1000);
        d.set_delay_samples(100);
        d.set_feedback(0.0);
        d.set_mix(1.0);
        let mut seen = vec![0.0f32; 300];
        for (i, s) in seen.iter_mut().enumerate() {
            let x = if i == 0 { 1.0 } else { 0.0 };
            *s = d.tick(x);
        }
        assert_eq!(seen[0], 0.0); // impulse goes into the line first
        assert!((seen[100] - 1.0).abs() < 1e-6, "echo at {}", seen[100]);
    }

    #[test]
    fn feedback_decay_produces_trailing_echos() {
        let mut d = Delay::new(500);
        d.set_delay_samples(50);
        d.set_feedback(0.5);
        d.set_mix(1.0);
        // Feed one impulse; subsequent echoes should be ~0.5^k of the previous.
        let _ = d.tick(1.0);
        let mut prev = 0.0f32;
        let mut first = true;
        let mut echoes_found = 0;
        for _ in 0..200 {
            let tap = d.tick(0.0);
            if tap > 0.01 {
                if first {
                    assert!((tap - 1.0).abs() < 1e-4, "first echo {tap}");
                    first = false;
                    prev = 1.0;
                } else {
                    assert!(
                        (tap - prev * 0.5).abs() < 1e-4,
                        "tap {tap} expected {}",
                        prev * 0.5
                    );
                    prev = tap;
                }
                echoes_found += 1;
            }
        }
        assert!(echoes_found >= 3, "found {echoes_found} echoes");
    }
}
