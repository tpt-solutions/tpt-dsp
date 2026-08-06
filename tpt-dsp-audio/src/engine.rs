//! Real-time callback engine with fixed block sizes.
//!
//! The engine drives a per-block DSP closure (`input → output`) using
//! pre-allocated buffers sized to the block. Everything is allocated up front,
//! so calling [`process_block`](RealtimeEngine::process_block) or
//! [`process_with`](RealtimeEngine::process_with) never allocates and always
//! takes a bounded amount of work — the guarantees a real-time audio thread
//! needs. The standard block sizes (128 / 256 samples) are provided as
//! constants for convenience.

/// Standard real-time audio block size (low latency).
pub const BLOCK_128: usize = 128;
/// Standard real-time audio block size (slightly higher latency, lower overhead).
pub const BLOCK_256: usize = 256;

/// Drives a per-block DSP callback with strict, bounded block processing.
///
/// The callback has the signature `FnMut(&[f32], &mut [f32])` and must write
/// exactly `block_size` output samples from `block_size` input samples. Both
/// the input and output buffers are owned by the engine and sized once at
/// construction.
pub struct RealtimeEngine<F> {
    block_size: usize,
    input: Vec<f32>,
    output: Vec<f32>,
    callback: F,
}

impl<F: FnMut(&[f32], &mut [f32])> RealtimeEngine<F> {
    /// Create an engine processing `block_size` samples per call.
    ///
    /// # Panics
    ///
    /// Panics if `block_size` is zero.
    pub fn new(block_size: usize, callback: F) -> Self {
        assert!(block_size > 0, "block size must be nonzero");
        Self {
            block_size,
            input: vec![0.0; block_size],
            output: vec![0.0; block_size],
            callback,
        }
    }

    /// Block size in samples.
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Mutable access to the engine's input buffer (fill before calling
    /// [`process_block`](Self::process_block)).
    pub fn input(&mut self) -> &mut [f32] {
        &mut self.input
    }

    /// Read-only access to the most recent output buffer.
    pub fn output(&self) -> &[f32] {
        &self.output
    }

    /// Run the callback over the current [`input`](Self::input) buffer,
    /// storing the result in the output buffer. Allocation-free.
    pub fn process_block(&mut self) {
        // Destructure to obtain disjoint borrows of the three fields.
        let Self {
            input,
            output,
            callback,
            ..
        } = self;
        (callback)(input, output);
    }

    /// Convenience: feed `input` (must be `block_size` long), run the
    /// callback, and return a slice of the output. Allocation-free.
    pub fn process_with(&mut self, input: &[f32]) -> &[f32] {
        assert_eq!(
            input.len(),
            self.block_size,
            "input must be block_size long"
        );
        self.input.copy_from_slice(input);
        (self.callback)(&self.input, &mut self.output);
        &self.output
    }

    /// Render `total_frames` samples by repeatedly calling `source` to
    /// generate input frames and writing outputs to `out` (which must be at
    /// least `total_frames` long). Used for offline rendering / tests.
    pub fn render(
        &mut self,
        total_frames: usize,
        mut source: impl FnMut(usize) -> f32,
        out: &mut [f32],
    ) {
        assert!(out.len() >= total_frames, "output too small");
        let bs = self.block_size;
        let mut written = 0;
        let mut frame = 0;
        while written < total_frames {
            for x in self.input.iter_mut() {
                *x = source(frame);
                frame += 1;
            }
            (self.callback)(&self.input, &mut self.output);
            let take = bs.min(total_frames - written);
            out[written..written + take].copy_from_slice(&self.output[..take]);
            written += take;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_callback_scales_block() {
        let mut eng = RealtimeEngine::new(BLOCK_128, |in_: &[f32], out: &mut [f32]| {
            for (o, x) in out.iter_mut().zip(in_.iter()) {
                *o = x * 0.5;
            }
        });
        let input = [1.0f32; BLOCK_128];
        let out = eng.process_with(&input);
        assert!(out.iter().all(|&x| (x - 0.5).abs() < 1e-6));
    }

    #[test]
    fn render_produces_requested_frames() {
        let mut eng = RealtimeEngine::new(64, |in_: &[f32], out: &mut [f32]| {
            out.copy_from_slice(in_);
        });
        let mut out = vec![0.0f32; 200];
        eng.render(200, |i| (i as f32 * 0.01).sin(), &mut out);
        assert!((out[199] - (199.0f32 * 0.01).sin()).abs() < 1e-5);
    }

    #[test]
    fn process_block_uses_internal_buffer() {
        let mut eng = RealtimeEngine::new(32, |in_: &[f32], out: &mut [f32]| {
            for (o, x) in out.iter_mut().zip(in_.iter()) {
                *o = x + 1.0;
            }
        });
        for x in eng.input().iter_mut() {
            *x = 2.0;
        }
        eng.process_block();
        assert!(eng.output().iter().all(|&x| (x - 3.0).abs() < 1e-6));
    }
}
