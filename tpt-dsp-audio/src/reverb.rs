//! Convolution reverb using a pre-allocated impulse-response buffer.
//!
//! Wraps the real-time, zero-allocation [`FftConvolver`] from `tpt-dsp-core`
//! in an effect that streams arbitrary-length input through fixed-size
//! blocks. A helper is provided to synthesize a decaying-noise impulse
//! response for quick experimentation.
//!
//! # Latency
//!
//! The output lags the input by exactly one block (`block_size` samples). The
//! dry signal is delayed by the same amount so wet/dry mixing stays aligned.
//! Input is accumulated until a whole block is available, so the convolver
//! always sees full blocks and the stream stays continuous across calls of
//! any length; a short final chunk never advances the convolver's state by
//! padding.

use tpt_dsp_core::FftConvolver;

/// Magnitudes below this are flushed to zero on output, so decaying tails
/// can't sink into subnormal arithmetic.
const FLUSH_THRESHOLD: f32 = 1.0e-20;

/// A convolution reverb effect.
pub struct ConvolutionReverb {
    convolver: FftConvolver<f32>,
    block_size: usize,
    wet: f32,
    /// The input block currently being collected.
    block_in: Vec<f32>,
    /// The previous complete input block: the dry signal, one block late.
    prev_in: Vec<f32>,
    /// Convolved output of the previous complete block.
    wet_block: Vec<f32>,
    /// Samples collected so far in `block_in`.
    fill: usize,
}

impl ConvolutionReverb {
    /// Create a reverb from an impulse response and a processing block size.
    ///
    /// The block size is the unit of real-time processing and also the
    /// output latency; smaller blocks reduce latency at the cost of more FFT
    /// overhead. All buffers are allocated here, so
    /// [`process`](Self::process) is allocation-free.
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
            prev_in: vec![0.0; block_size],
            wet_block: vec![0.0; block_size],
            fill: 0,
        }
    }

    /// Set the wet/dry mix (`0` = dry, `1` = fully wet). NaN becomes `0`.
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = if wet.is_finite() {
            wet.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// Block size configured at construction (also the output latency in
    /// samples).
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Process `input`, writing the (wet/dry mixed) result into `output`.
    ///
    /// Both slices must be the same length. Output lags input by
    /// [`block_size`](Self::block_size) samples; the first block of output is
    /// silence. Allocation-free: the per-block scratch buffers are reused, and
    /// successive calls continue the same stream regardless of chunk length.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        assert_eq!(input.len(), output.len(), "input/output length mismatch");
        for (&x, o) in input.iter().zip(output.iter_mut()) {
            let x = if x.is_finite() { x } else { 0.0 };
            let j = self.fill;
            let dry = self.prev_in[j];
            let wet = self.wet_block[j];
            self.block_in[j] = x;
            self.fill += 1;

            *o = flush(dry * (1.0 - self.wet) + wet * self.wet);

            if self.fill == self.block_size {
                self.convolver.process(&self.block_in, &mut self.wet_block);
                std::mem::swap(&mut self.block_in, &mut self.prev_in);
                self.fill = 0;
            }
        }
    }
}

/// Clamp non-finite values and tiny magnitudes to zero.
fn flush(v: f32) -> f32 {
    if v.is_finite() && v.abs() >= FLUSH_THRESHOLD {
        v
    } else {
        0.0
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
    fn identity_kernel_is_passthrough_with_one_block_latency() {
        let ir = [1.0f32];
        let bs = 8;
        let mut rev = ConvolutionReverb::new(&ir, bs);
        let input: Vec<f32> = (0..32).map(|i| (i as f32 * 0.1).sin()).collect();
        let mut out = vec![0.0f32; 32];
        rev.process(&input, &mut out);
        for (i, v) in out.iter().enumerate().take(bs) {
            assert!(v.abs() < 1e-6, "priming silence at {i}: {v}");
        }
        for i in bs..32 {
            assert!(
                (out[i] - input[i - bs]).abs() < 1e-4,
                "got {} want {}",
                out[i],
                input[i - bs]
            );
        }
    }

    #[test]
    fn short_chunks_match_one_long_call() {
        // Splitting the stream into odd-sized chunks must not change the
        // output: the convolver must never see padding.
        let ir = generate_decay_ir(40, 48000.0, 0.02);
        let input: Vec<f32> = (0..200)
            .map(|i| ((i * 7 % 23) as f32 - 11.0) / 11.0)
            .collect();

        let mut whole = ConvolutionReverb::new(&ir, 16);
        let mut expected = vec![0.0f32; input.len()];
        whole.process(&input, &mut expected);

        let mut chunked = ConvolutionReverb::new(&ir, 16);
        let mut got = vec![0.0f32; input.len()];
        let mut at = 0;
        for &n in [5usize, 11, 1, 37, 2, 64, 80].iter().cycle() {
            if at >= input.len() {
                break;
            }
            let end = (at + n).min(input.len());
            chunked.process(&input[at..end], &mut got[at..end]);
            at = end;
        }
        for (i, (a, b)) in got.iter().zip(expected.iter()).enumerate() {
            assert!((a - b).abs() < 1e-4, "sample {i}: chunked {a} vs whole {b}");
        }
    }

    #[test]
    fn reverb_of_impulse_is_ir_shape() {
        let ir = generate_decay_ir(64, 48000.0, 0.05);
        let mut rev = ConvolutionReverb::new(&ir, 16);
        rev.set_wet(1.0);
        let mut out = vec![0.0f32; 64];
        let mut padded_in = vec![0.0f32; 64];
        padded_in[0] = 1.0;
        rev.process(&padded_in, &mut out);
        // The impulse appears one block late, so the IR starts at index 16.
        assert!((out[16] - ir[0]).abs() < 1e-3, "{} vs {}", out[16], ir[0]);
    }

    #[test]
    fn output_is_finite_for_noise() {
        let ir = generate_decay_ir(128, 48000.0, 0.1);
        let mut rev = ConvolutionReverb::new(&ir, 32);
        let mut input: Vec<f32> = (0..256).map(|i| (i as f32 * 0.3).sin() * 0.5).collect();
        input[10] = f32::NAN;
        let mut out = vec![0.0f32; 256];
        rev.process(&input, &mut out);
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
