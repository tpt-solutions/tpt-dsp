// SPDX-License-Identifier: MIT OR Apache-2.0
//! The pedalboard signal chain: distortion → delay → reverb → EQ.
//!
//! The chain is assembled from `tpt-dsp-audio` effects and exposed to
//! JavaScript through [`Pedalboard`]. Every buffer the hot path touches is
//! owned by the struct and sized at construction, so
//! [`Pedalboard::process_block_128`] and
//! [`Pedalboard::process_internal_block`] never allocate.

use std::cell::RefCell;
use std::rc::Rc;

use tpt_dsp_audio::{
    generate_decay_ir, AudioGraph, AudioNode, ClosureSink, ClosureSource, ConvolutionReverb, Curve,
    Delay, Eq, Oscillator, Waveform, Waveshaper, BLOCK_128,
};
use wasm_bindgen::prelude::*;

/// Real-time block size in samples — one Web Audio render quantum.
pub const BLOCK: usize = BLOCK_128;

/// Number of peaking EQ bands in the tone stack.
pub const EQ_BAND_COUNT: usize = 3;

const DEFAULT_SAMPLE_RATE: f32 = 48_000.0;
const MAX_DELAY_SECONDS: f32 = 2.0;
const EQ_FREQUENCIES: [f32; EQ_BAND_COUNT] = [100.0, 800.0, 3_200.0];
const EQ_Q: f32 = 0.9;
const EQ_MAX_GAIN_DB: f32 = 24.0;
const AMBIENCE_DECAY_SECONDS: f32 = 0.01;

/// Transfer curve used by the distortion stage.
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistortionCurve {
    /// Soft tube-like saturation, `tanh(drive·x)`.
    Tanh = 0,
    /// Hard clipping — fuzz territory.
    HardClip = 1,
    /// Cubic soft clipping.
    Cubic = 2,
}

impl From<DistortionCurve> for Curve {
    fn from(curve: DistortionCurve) -> Self {
        match curve {
            DistortionCurve::Tanh => Curve::Tanh,
            DistortionCurve::HardClip => Curve::HardClip,
            DistortionCurve::Cubic => Curve::Cubic,
        }
    }
}

fn build_eq(sample_rate: f32, gains: &[f32; EQ_BAND_COUNT]) -> Eq {
    let bands: [(f32, f32, f32); EQ_BAND_COUNT] =
        core::array::from_fn(|i| (EQ_FREQUENCIES[i], gains[i], EQ_Q));
    let mut eq = Eq::new(sample_rate, &bands);
    // `Eq` grows its internal scratch buffer lazily on the first call, so
    // prime it here with a silent block to keep the hot path allocation-free.
    let mut warmup = [0.0f32; BLOCK];
    eq.process(&mut warmup);
    eq
}

fn ambience_ir(sample_rate: f32) -> Vec<f32> {
    let mut ir = generate_decay_ir(BLOCK, sample_rate, AMBIENCE_DECAY_SECONDS);
    let energy = ir.iter().map(|x| x * x).sum::<f32>().sqrt();
    if energy > f32::EPSILON {
        let norm = 1.0 / energy;
        for tap in ir.iter_mut() {
            *tap *= norm;
        }
    }
    ir
}

/// The pedal chain as a single graph node.
struct PedalChain {
    sample_rate: f32,
    distortion: Waveshaper,
    delay: Delay,
    reverb: ConvolutionReverb,
    eq: Eq,
    eq_gains: [f32; EQ_BAND_COUNT],
    wet: [f32; BLOCK],
    output_gain: f32,
}

impl PedalChain {
    fn new(sample_rate: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            DEFAULT_SAMPLE_RATE
        };
        let max_delay = ((MAX_DELAY_SECONDS * sample_rate) as usize).max(BLOCK);
        let mut delay = Delay::new(max_delay);
        delay.set_delay_seconds(0.25, sample_rate);
        delay.set_feedback(0.3);
        delay.set_mix(0.25);
        let mut reverb = ConvolutionReverb::new(&ambience_ir(sample_rate), BLOCK);
        reverb.set_wet(0.2);
        let eq_gains = [0.0f32; EQ_BAND_COUNT];
        Self {
            sample_rate,
            distortion: Waveshaper::new(Curve::Tanh, 4.0, 0.8),
            delay,
            reverb,
            eq: build_eq(sample_rate, &eq_gains),
            eq_gains,
            wet: [0.0; BLOCK],
            output_gain: 1.0,
        }
    }

    /// Run one block through the chain. Allocation-free; blocks longer than
    /// [`BLOCK`] are truncated and the remainder of `output` is silenced.
    fn render(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len()).min(BLOCK);
        output[..len].copy_from_slice(&input[..len]);
        self.distortion.process(&mut output[..len]);
        self.delay.process(&mut output[..len]);
        self.reverb.process(&output[..len], &mut self.wet[..len]);
        output[..len].copy_from_slice(&self.wet[..len]);
        self.eq.process(&mut output[..len]);
        for sample in output[..len].iter_mut() {
            *sample *= self.output_gain;
        }
        for sample in output[len..].iter_mut() {
            *sample = 0.0;
        }
    }

    fn set_eq_gain(&mut self, band: usize, gain_db: f32) {
        if band >= EQ_BAND_COUNT {
            return;
        }
        self.eq_gains[band] = gain_db.clamp(-EQ_MAX_GAIN_DB, EQ_MAX_GAIN_DB);
        self.eq = build_eq(self.sample_rate, &self.eq_gains);
    }

    fn reset(&mut self) {
        self.delay.reset();
        self.eq.reset();
        self.wet = [0.0; BLOCK];
    }
}

impl AudioNode for PedalChain {
    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        self.render(input, output);
    }
}

/// A four-stage guitar pedalboard: distortion → delay → reverb → EQ.
///
/// Construct once, then drive it from an `AudioWorkletProcessor`. The
/// preferred hot path is [`input_ptr`](Self::input_ptr) /
/// [`output_ptr`](Self::output_ptr) plus
/// [`process_internal_block`](Self::process_internal_block), which lets JS
/// write and read the 128-sample buffers directly in linear memory with no
/// copies and no allocation.
#[wasm_bindgen]
pub struct Pedalboard {
    chain: PedalChain,
    input: [f32; BLOCK],
    output: [f32; BLOCK],
}

#[wasm_bindgen]
impl Pedalboard {
    /// Create a pedalboard at the default 48 kHz sample rate.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Pedalboard {
        Self::with_sample_rate(DEFAULT_SAMPLE_RATE)
    }

    /// Create a pedalboard for a specific sample rate (use the worklet's
    /// `sampleRate` global). Non-finite or non-positive rates fall back to
    /// 48 kHz.
    pub fn with_sample_rate(sample_rate: f32) -> Pedalboard {
        Pedalboard {
            chain: PedalChain::new(sample_rate),
            input: [0.0; BLOCK],
            output: [0.0; BLOCK],
        }
    }

    /// Sample rate the chain was designed for.
    #[wasm_bindgen(getter)]
    pub fn sample_rate(&self) -> f32 {
        self.chain.sample_rate
    }

    /// Block size of the real-time contract, in samples (always 128).
    #[wasm_bindgen(getter)]
    pub fn block_size(&self) -> u32 {
        BLOCK as u32
    }

    /// Distortion pre-gain (`0` = clean, ~`20` = saturated).
    pub fn set_distortion(&mut self, drive: f32) {
        self.chain.distortion.set_drive(drive);
    }

    /// Distortion wet/dry blend (`0` = bypassed, `1` = fully shaped).
    pub fn set_distortion_mix(&mut self, mix: f32) {
        self.chain.distortion.set_mix(mix);
    }

    /// Select the distortion transfer curve.
    pub fn set_distortion_curve(&mut self, curve: DistortionCurve) {
        self.chain.distortion.set_curve(curve.into());
    }

    /// Delay time in seconds (clamped to the 2 s line allocated in `new`).
    pub fn set_delay_time(&mut self, seconds: f32) {
        let sample_rate = self.chain.sample_rate;
        self.chain
            .delay
            .set_delay_seconds(seconds.clamp(0.0, MAX_DELAY_SECONDS), sample_rate);
    }

    /// Delay feedback gain (`0` = single repeat, `0.99` = near-infinite).
    pub fn set_delay_feedback(&mut self, feedback: f32) {
        self.chain.delay.set_feedback(feedback);
    }

    /// Delay wet/dry blend.
    pub fn set_delay_mix(&mut self, mix: f32) {
        self.chain.delay.set_mix(mix);
    }

    /// Reverb wet/dry blend.
    pub fn set_reverb_mix(&mut self, mix: f32) {
        self.chain.reverb.set_wet(mix);
    }

    /// Set the gain of one EQ band (0 = 100 Hz, 1 = 800 Hz, 2 = 3.2 kHz) in
    /// decibels, clamped to ±24 dB.
    ///
    /// This is a control-path call, **not** a real-time one: `tpt-dsp-audio`'s
    /// [`struct@Eq`] has no in-place coefficient update, so the biquad cascade is
    /// rebuilt (which allocates and resets the filter state). Call it from the
    /// message handler, never from inside `process`.
    pub fn set_eq_gain(&mut self, band: u32, gain_db: f32) {
        self.chain.set_eq_gain(band as usize, gain_db);
    }

    /// Master output gain applied after the EQ.
    pub fn set_output_gain(&mut self, gain: f32) {
        self.chain.output_gain = gain.clamp(0.0, 4.0);
    }

    /// Clear delay, reverb-adjacent and filter state.
    pub fn reset(&mut self) {
        self.chain.reset();
        self.input = [0.0; BLOCK];
        self.output = [0.0; BLOCK];
    }

    /// Address of the shared 128-sample input buffer in linear memory.
    ///
    /// Only meaningful on `wasm32`, where pointers are 32-bit; on a 64-bit
    /// host the value is truncated and must not be dereferenced. JS should
    /// create a `Float32Array(memory.buffer, ptr, 128)` view over it and
    /// re-create the view after any memory growth.
    pub fn input_ptr(&self) -> u32 {
        self.input.as_ptr() as usize as u32
    }

    /// Address of the shared 128-sample output buffer. Same caveats as
    /// [`input_ptr`](Self::input_ptr).
    pub fn output_ptr(&self) -> u32 {
        self.output.as_ptr() as usize as u32
    }

    /// Process the shared input buffer into the shared output buffer.
    ///
    /// This is the zero-copy, zero-allocation call an `AudioWorkletProcessor`
    /// makes once per render quantum.
    pub fn process_internal_block(&mut self) {
        let Self {
            chain,
            input,
            output,
        } = self;
        chain.render(input, output);
    }

    /// Process `input` into `output` (both copied across the JS boundary).
    ///
    /// Convenience for callers that do not want to manage memory views;
    /// prefer [`process_internal_block`](Self::process_internal_block) in the
    /// audio callback.
    pub fn process_block(&mut self, input: &[f32], output: &mut [f32]) {
        self.chain.render(input, output);
    }

    /// Process an arbitrary-length buffer, returning a freshly allocated
    /// `Float32Array`.
    ///
    /// Offline / test path only: the returned `Vec` allocates. The input is
    /// split into [`BLOCK`]-sized chunks; a trailing partial chunk is
    /// zero-padded inside the convolution reverb, so feed multiples of 128 for
    /// a gap-free reverb tail.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let mut output = vec![0.0f32; input.len()];
        for (chunk_in, chunk_out) in input.chunks(BLOCK).zip(output.chunks_mut(BLOCK)) {
            self.chain.render(chunk_in, chunk_out);
        }
        output
    }
}

impl Pedalboard {
    /// The real-time callback contract: exactly 128 samples in, 128 out.
    ///
    /// Not exported to JS (wasm-bindgen has no fixed-size array ABI); it is
    /// the Rust-side proof that the chain runs on caller-owned buffers with no
    /// heap traffic. Verified by the `process_block_128_does_not_allocate`
    /// test, which installs a counting global allocator.
    pub fn process_block_128(&mut self, input: &[f32; BLOCK], output: &mut [f32; BLOCK]) {
        self.chain.render(input, output);
    }
}

impl Default for Pedalboard {
    fn default() -> Self {
        Self::new()
    }
}

/// Render a dry-signal-free demo: an oscillator driven through a fresh pedal
/// chain, wired with the `tpt-dsp-audio` [`AudioGraph`].
///
/// Offline helper for auditioning the chain without a microphone; it
/// allocates and is not real-time safe.
#[wasm_bindgen]
pub fn render_demo(sample_rate: f32, frequency: f32, frames: usize) -> Vec<f32> {
    let mut osc = Oscillator::with_waveform(sample_rate, frequency, Waveform::Sawtooth);
    let captured = Rc::new(RefCell::new(Vec::with_capacity(frames)));
    let sink_handle = Rc::clone(&captured);
    let mut graph = AudioGraph::new(
        BLOCK,
        Box::new(ClosureSource(move |out: &mut [f32]| {
            for sample in out.iter_mut() {
                *sample = osc.tick() * 0.5;
            }
        })),
        vec![Box::new(PedalChain::new(sample_rate))],
        Box::new(ClosureSink(move |block: &[f32]| {
            sink_handle.borrow_mut().extend_from_slice(block);
        })),
    );
    graph.run(frames.div_ceil(BLOCK));
    let mut rendered = captured.borrow().clone();
    rendered.truncate(frames);
    rendered
}

#[cfg(test)]
#[global_allocator]
static ALLOCATOR: alloc_probe::CountingAllocator = alloc_probe::CountingAllocator;

/// A counting global allocator used to prove the real-time path is
/// allocation-free. Counting is per-thread and opt-in, so parallel tests do
/// not disturb each other.
#[cfg(test)]
mod alloc_probe {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }

    pub struct CountingAllocator;

    fn record() {
        let armed = ARMED.try_with(|a| a.get()).unwrap_or(false);
        if armed {
            let _ = COUNT.try_with(|c| c.set(c.get() + 1));
        }
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            record();
            unsafe { System.alloc_zeroed(layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            record();
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    /// Count the allocations made by `f` on the current thread.
    pub fn count_allocations(f: impl FnOnce()) -> usize {
        COUNT.with(|c| c.set(0));
        ARMED.with(|a| a.set(true));
        f();
        ARMED.with(|a| a.set(false));
        COUNT.with(|c| c.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_block() -> [f32; BLOCK] {
        core::array::from_fn(|i| (i as f32 * 0.05).sin() * 0.4)
    }

    #[test]
    fn process_block_128_does_not_allocate() {
        let mut pedal = Pedalboard::new();
        let input = test_block();
        let mut output = [0.0f32; BLOCK];
        pedal.process_block_128(&input, &mut output);

        let allocations = alloc_probe::count_allocations(|| {
            for _ in 0..64 {
                pedal.process_block_128(&input, &mut output);
            }
        });
        assert_eq!(
            allocations, 0,
            "real-time path allocated {allocations} times"
        );
    }

    #[test]
    fn counting_allocator_actually_sees_allocations() {
        let allocations = alloc_probe::count_allocations(|| {
            let v: Vec<f32> = vec![0.0; 1024];
            core::hint::black_box(&v);
        });
        assert!(allocations > 0, "allocation probe is not wired up");
    }

    #[test]
    fn chain_output_is_finite_and_bounded() {
        let mut pedal = Pedalboard::with_sample_rate(44_100.0);
        pedal.set_distortion(12.0);
        pedal.set_delay_time(0.1);
        pedal.set_reverb_mix(0.4);
        pedal.set_eq_gain(2, 6.0);
        let input = test_block();
        let mut output = [0.0f32; BLOCK];
        for _ in 0..32 {
            pedal.process_block_128(&input, &mut output);
        }
        assert!(output.iter().all(|x| x.is_finite()));
        assert!(output.iter().all(|x| x.abs() < 8.0));
    }

    #[test]
    fn silence_in_silence_out() {
        let mut pedal = Pedalboard::new();
        let input = [0.0f32; BLOCK];
        let mut output = [1.0f32; BLOCK];
        pedal.process_block_128(&input, &mut output);
        assert!(output.iter().all(|x| x.abs() < 1e-6));
    }

    #[test]
    fn fully_dry_chain_is_near_transparent() {
        let mut pedal = Pedalboard::new();
        pedal.set_distortion_mix(0.0);
        pedal.set_delay_mix(0.0);
        pedal.set_reverb_mix(0.0);
        let input = test_block();
        let mut output = [0.0f32; BLOCK];
        pedal.process_block_128(&input, &mut output);
        for (out, dry) in output.iter().zip(input.iter()) {
            assert!((out - dry).abs() < 0.05, "{out} vs {dry}");
        }
    }

    #[test]
    fn process_handles_arbitrary_lengths() {
        let mut pedal = Pedalboard::new();
        let input: Vec<f32> = (0..300).map(|i| (i as f32 * 0.02).sin()).collect();
        let output = pedal.process(&input);
        assert_eq!(output.len(), input.len());
        assert!(output.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn demo_render_returns_requested_frames() {
        let rendered = render_demo(48_000.0, 220.0, 500);
        assert_eq!(rendered.len(), 500);
        assert!(rendered.iter().all(|x| x.is_finite()));
    }
}
