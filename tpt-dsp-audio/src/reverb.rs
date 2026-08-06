//! Convolution reverb using a pre-allocated impulse-response buffer.
//!
//! Wraps the real-time, zero-allocation [`FftConvolver`] from `tpt-dsp-core`
//! in an effect that streams arbitrary-length input through fixed-size
//! blocks. A helper is provided to synthesize a decaying-noise impulse
//! response for quick experimentation.

use tpt_dsp_core::FftConvolver;

/// A convolution reverb effect.
pub struct ConvolutionReverb {
    convolver: FftConvolver<f32>,
    block_size: usize,
    wet: f32,
    block_in: Vec<f32>,
    block_out: Vec<f32>,
}

impl ConvolutionReverb {
    /// Create a reverb from an impulse response and a processing block size.
    ///
    /// The block size is the unit of real-time processing; smaller blocks
    /// reduce latency at the cost of more FFT overhead. All buffers are
    /// allocated here, so [`process`](Self::process) is allocation-free.
    ///
    /// # Panics
    ///
    /// Panics if `block_size` is zero.
    pub fn new(impulse_response: &[f32], block_size: usize) -> Self {
        assert!(block_size > 0, "block size must be nonzero");
        Self {
            convolver: FftConvolver::new(impulse_response, block_size),
            block_size,
            wet: 1.0,
            block_in: vec![0.0; block_size],
            block_out: vec![0.0; block_size],
        }
    }

    /// Set the wet/dry mix (`0` = dry, `1` = fully wet).
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = wet.clamp(0.0, 1.0);
    }

    /// Block size configured at construction.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Process `input`, writing the (wet/dry mixed) result into `output`.
    ///
    /// Both slices must be the same length. Internally the signal is split
    /// into `block_size`-sized pieces; a final short block is zero-padded.
    /// Allocation-free: the per-block scratch buffers are reused.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len(), "input/output length mismatch");
        let bs = self.block_size;
        let mut i = 0;
        while i < input.len() {
            let end = (i + bs).min(input.len());
            let len = end - i;
            self.block_in[..len].copy_from_slice(&input[i..end]);
            for s in &mut self.block_in[len..] {
                *s = 0.0;
            }
            self.convolver.process(&self.block_in, &mut self.block_out);
            for (o, (x, y)) in output[i..end]
                .iter_mut()
                .zip(input[i..end].iter().zip(self.block_out[..len].iter()))
            {
                *o = x * (1.0 - self.wet) + y * self.wet;
            }
            i = end;
        }
    }
}

/// Synthesize a simple exponentially-decaying stereo-in-mono impulse response.
///
/// `length` samples of white noise are multiplied by `exp(-t / tau)` where
/// `tau` is `decay_seconds · sample_rate`. Useful for demonstration and
/// tests without shipping an audio file.
pub fn generate_decay_ir(length: usize, sample_rate: f32, decay_seconds: f32) -> Vec<f32> {
    assert!(length > 0, "IR length must be positive");
    let tau = (decay_seconds * sample_rate).max(1.0);
    let mut state = 0x12345678u32;
    (0..length)
        .map(|n| {
            // xorshift32 PRNG for deterministic noise without external deps.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let u = (state as f32) / (u32::MAX as f32); // [0, 1)
            let noise = u * 2.0 - 1.0;
            let t = n as f32;
            noise * (-t / tau).exp()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_kernel_is_passthrough() {
        let ir = [1.0f32];
        let mut rev = ConvolutionReverb::new(&ir, 8);
        let input: Vec<f32> = (0..32).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut out = vec![0.0f32; 32];
        rev.process(&input, &mut out);
        for (a, b) in out.iter().zip(input.iter()) {
            assert!((a - b).abs() < 1e-4, "got {a} want {b}");
        }
    }

    #[test]
    fn reverb_of_impulse_is_ir_shape() {
        let ir = generate_decay_ir(64, 48000.0, 0.05);
        let mut rev = ConvolutionReverb::new(&ir, 16);
        rev.set_wet(1.0);
        let mut out = vec![0.0f32; 64];
        let input = [1.0f32; 1];
        let padded_in: Vec<f32> = std::iter::once(1.0f32)
            .chain(std::iter::repeat(0.0))
            .take(64)
            .collect();
        let _ = input;
        rev.process(&padded_in, &mut out);
        // First sample should equal the first IR tap.
        assert!((out[0] - ir[0]).abs() < 1e-3, "{} vs {}", out[0], ir[0]);
    }

    #[test]
    fn output_is_finite_for_noise() {
        let ir = generate_decay_ir(128, 48000.0, 0.1);
        let mut rev = ConvolutionReverb::new(&ir, 32);
        let input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.3).sin() * 0.5).collect();
        let mut out = vec![0.0f32; 256];
        rev.process(&input, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
