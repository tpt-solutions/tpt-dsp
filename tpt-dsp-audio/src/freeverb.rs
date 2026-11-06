//! Algorithmic reverb in the style of Freeverb.
//!
//! Eight parallel feedback comb filters (each with a one-pole low-pass in the
//! feedback path for high-frequency damping) feed four series all-pass
//! filters. The delay lengths are the Freeverb tunings, scaled from 44.1 kHz to
//! the actual sample rate. All buffers are allocated in the constructor, so
//! processing is allocation-free.
//!
//! Two variants are provided:
//!
//! - [`Freeverb`] is mono.
//! - [`StereoFreeverb`] runs a second network whose delay lengths are each
//!   lengthened by Freeverb's stereo spread (23 samples at 44.1 kHz). The two
//!   channels then decorrelate, which a mono tail does not. Both channels are
//!   fed the sum of the inputs, as in Freeverb.

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
/// Right-channel delay offset in Freeverb, at the 44.1 kHz tuning rate.
const STEREO_SPREAD: usize = 23;
const TUNING_RATE: f32 = 44_100.0;
const FIXED_GAIN: f32 = 0.015;
const ALLPASS_FEEDBACK: f32 = 0.5;
const SCALE_DAMP: f32 = 0.4;
const SCALE_ROOM: f32 = 0.28;
const OFFSET_ROOM: f32 = 0.7;

#[derive(Debug, Clone)]
struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
            store: 0.0,
        }
    }

    fn tick(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.idx] = input + self.store * feedback;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.idx = 0;
        self.store = 0.0;
    }
}

#[derive(Debug, Clone)]
struct AllPass {
    buf: Vec<f32>,
    idx: usize,
}

impl AllPass {
    fn new(len: usize) -> Self {
        Self {
            buf: vec![0.0; len.max(1)],
            idx: 0,
        }
    }

    fn tick(&mut self, input: f32) -> f32 {
        let bufout = self.buf[self.idx];
        let out = -input + bufout;
        self.buf[self.idx] = input + bufout * ALLPASS_FEEDBACK;
        self.idx = (self.idx + 1) % self.buf.len();
        out
    }

    fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.idx = 0;
    }
}

/// One comb-and-all-pass network. The spread selects the delay lengths.
#[derive(Debug, Clone)]
struct Network {
    combs: Vec<Comb>,
    allpasses: Vec<AllPass>,
}

impl Network {
    fn new(sample_rate: f32, spread: usize) -> Self {
        Self {
            combs: COMB_TUNING
                .iter()
                .map(|&n| Comb::new(scaled(n + spread, sample_rate)))
                .collect(),
            allpasses: ALLPASS_TUNING
                .iter()
                .map(|&n| AllPass::new(scaled(n + spread, sample_rate)))
                .collect(),
        }
    }

    /// Run one sample through the network. `input` is already scaled.
    fn tick(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let mut acc = 0.0f32;
        for c in self.combs.iter_mut() {
            acc += c.tick(input, feedback, damp);
        }
        for a in self.allpasses.iter_mut() {
            acc = a.tick(acc);
        }
        acc
    }

    fn reset(&mut self) {
        self.combs.iter_mut().for_each(Comb::reset);
        self.allpasses.iter_mut().for_each(AllPass::reset);
    }
}

/// Shared room, damping and wet settings for both reverb variants.
#[derive(Debug, Clone, Copy)]
struct Controls {
    feedback: f32,
    damp: f32,
    wet: f32,
}

impl Controls {
    fn new(room_size: f32, damping: f32, wet: f32) -> Self {
        let mut c = Self {
            feedback: 0.0,
            damp: 0.0,
            wet: 0.0,
        };
        c.set_room_size(room_size);
        c.set_damping(damping);
        c.set_wet(wet);
        c
    }

    fn set_room_size(&mut self, room: f32) {
        let r = if room.is_finite() {
            room.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.feedback = r * SCALE_ROOM + OFFSET_ROOM;
    }

    fn set_damping(&mut self, damping: f32) {
        let d = if damping.is_finite() {
            damping.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.damp = d * SCALE_DAMP;
    }

    fn set_wet(&mut self, wet: f32) {
        self.wet = if wet.is_finite() {
            wet.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
}

fn sanitise(x: f32) -> f32 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

fn scaled(len: usize, sample_rate: f32) -> usize {
    ((len as f32) * sample_rate / TUNING_RATE).round().max(1.0) as usize
}

fn valid_rate(sample_rate: f32) -> f32 {
    if sample_rate.is_finite() && sample_rate > 0.0 {
        sample_rate
    } else {
        48_000.0
    }
}

/// Freeverb-style mono reverb.
#[derive(Debug, Clone)]
pub struct Freeverb {
    network: Network,
    controls: Controls,
}

impl Freeverb {
    /// Create a reverb.
    ///
    /// * `room_size` — tail length in `[0, 1]` (NaN becomes 0.5).
    /// * `damping` — high-frequency absorption in `[0, 1]`.
    /// * `wet` — wet/dry mix in `[0, 1]`.
    ///
    /// A non-finite or non-positive `sample_rate` falls back to 48 kHz.
    pub fn new(sample_rate: f32, room_size: f32, damping: f32, wet: f32) -> Self {
        Self {
            network: Network::new(valid_rate(sample_rate), 0),
            controls: Controls::new(room_size, damping, wet),
        }
    }

    /// Set the room size in `[0, 1]` (NaN becomes 0.5).
    pub fn set_room_size(&mut self, room: f32) {
        self.controls.set_room_size(room);
    }

    /// Set the damping in `[0, 1]` (NaN becomes 0.5).
    pub fn set_damping(&mut self, damping: f32) {
        self.controls.set_damping(damping);
    }

    /// Set the wet/dry mix in `[0, 1]` (NaN becomes 0).
    pub fn set_wet(&mut self, wet: f32) {
        self.controls.set_wet(wet);
    }

    /// Clear all delay lines.
    pub fn reset(&mut self) {
        self.network.reset();
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = sanitise(x);
        let c = self.controls;
        let acc = self.network.tick(x * FIXED_GAIN, c.feedback, c.damp);
        x * (1.0 - c.wet) + acc * c.wet
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
        }
    }
}

/// Freeverb with its stereo spread: two decorrelated networks, one per
/// channel.
///
/// The left channel uses the Freeverb tunings. The right channel adds the
/// stereo spread to every delay length. Both networks receive the sum of the
/// two inputs. Room size, damping and wet mix apply to both channels.
#[derive(Debug, Clone)]
pub struct StereoFreeverb {
    left: Network,
    right: Network,
    controls: Controls,
}

impl StereoFreeverb {
    /// Create a stereo reverb. Parameters are as for [`Freeverb::new`].
    pub fn new(sample_rate: f32, room_size: f32, damping: f32, wet: f32) -> Self {
        let sr = valid_rate(sample_rate);
        Self {
            left: Network::new(sr, 0),
            right: Network::new(sr, STEREO_SPREAD),
            controls: Controls::new(room_size, damping, wet),
        }
    }

    /// Set the room size in `[0, 1]` (NaN becomes 0.5).
    pub fn set_room_size(&mut self, room: f32) {
        self.controls.set_room_size(room);
    }

    /// Set the damping in `[0, 1]` (NaN becomes 0.5).
    pub fn set_damping(&mut self, damping: f32) {
        self.controls.set_damping(damping);
    }

    /// Set the wet/dry mix in `[0, 1]` (NaN becomes 0).
    pub fn set_wet(&mut self, wet: f32) {
        self.controls.set_wet(wet);
    }

    /// Clear all delay lines in both channels.
    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    /// Process one stereo sample, returning `(left, right)`.
    pub fn tick(&mut self, left: f32, right: f32) -> (f32, f32) {
        let l = sanitise(left);
        let r = sanitise(right);
        let c = self.controls;
        let input = (l + r) * FIXED_GAIN;
        let wl = self.left.tick(input, c.feedback, c.damp);
        let wr = self.right.tick(input, c.feedback, c.damp);
        (
            l * (1.0 - c.wet) + wl * c.wet,
            r * (1.0 - c.wet) + wr * c.wet,
        )
    }

    /// Process interleaved stereo (`[l, r, l, r, …]`) in place.
    ///
    /// A trailing odd sample is ignored.
    pub fn process_interleaved(&mut self, buf: &mut [f32]) {
        for frame in buf.chunks_exact_mut(2) {
            let (l, r) = self.tick(frame[0], frame[1]);
            frame[0] = l;
            frame[1] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;

    #[test]
    fn silence_in_silence_out() {
        let mut r = Freeverb::new(SR, 0.8, 0.5, 1.0);
        for _ in 0..10_000 {
            assert_eq!(r.tick(0.0), 0.0);
        }
    }

    #[test]
    fn zero_wet_is_dry() {
        let mut r = Freeverb::new(SR, 0.8, 0.5, 0.0);
        for i in 0..1000 {
            let x = (i as f32 * 0.01).sin();
            assert_eq!(r.tick(x), x);
        }
    }

    #[test]
    fn impulse_produces_a_decaying_tail() {
        let mut r = Freeverb::new(SR, 0.9, 0.3, 1.0);
        r.tick(1.0);
        let mut early = 0.0f32;
        let mut late = 0.0f32;
        for i in 0..SR as usize {
            let y = r.tick(0.0);
            assert!(y.is_finite());
            if i < 4800 {
                early += y * y;
            } else if i >= 4800 * 3 {
                late += y * y;
            }
        }
        assert!(early > 0.0, "no tail");
        assert!(late < early, "tail does not decay");
    }

    #[test]
    fn larger_room_rings_longer() {
        let energy_late = |room: f32| {
            let mut r = Freeverb::new(SR, room, 0.2, 1.0);
            r.tick(1.0);
            let mut late = 0.0f32;
            for i in 0..SR as usize {
                let y = r.tick(0.0);
                if i > 24_000 {
                    late += y * y;
                }
            }
            late
        };
        assert!(energy_late(0.95) > energy_late(0.2));
    }

    #[test]
    fn invalid_parameters_are_sanitised() {
        let mut r = Freeverb::new(SR, f32::NAN, f32::INFINITY, f32::NAN);
        for _ in 0..1000 {
            assert!(r.tick(f32::NAN).is_finite());
        }
    }

    #[test]
    fn stereo_impulse_decorrelates_the_channels() {
        let mut r = StereoFreeverb::new(SR, 0.9, 0.3, 1.0);
        r.tick(1.0, 1.0);
        let mut differs = false;
        let mut energy = 0.0f32;
        for _ in 0..SR as usize / 2 {
            let (l, rr) = r.tick(0.0, 0.0);
            assert!(l.is_finite() && rr.is_finite());
            energy += l * l + rr * rr;
            if (l - rr).abs() > 1e-6 {
                differs = true;
            }
        }
        assert!(energy > 0.0, "no tail");
        assert!(differs, "channels should not be identical");
    }

    #[test]
    fn stereo_spread_is_the_freeverb_offset_at_44k() {
        // At 44.1 kHz the spread is exactly Freeverb's 23 samples.
        let left = Network::new(44_100.0, 0);
        let right = Network::new(44_100.0, STEREO_SPREAD);
        for (l, r) in left.combs.iter().zip(&right.combs) {
            assert_eq!(r.buf.len(), l.buf.len() + STEREO_SPREAD);
        }
        for (l, r) in left.allpasses.iter().zip(&right.allpasses) {
            assert_eq!(r.buf.len(), l.buf.len() + STEREO_SPREAD);
        }
    }

    #[test]
    fn stereo_mono_input_matches_mono_on_the_left_when_spread_is_zero() {
        // With no spread, the left network is exactly the mono network fed
        // the same sum, so the left output tracks Freeverb's output.
        let mut stereo = StereoFreeverb::new(SR, 0.7, 0.4, 0.5);
        let mut mono = Freeverb::new(SR, 0.7, 0.4, 0.5);
        for i in 0..2000 {
            let x = (i as f32 * 0.05).sin();
            let (l, _) = stereo.tick(x, x);
            // The mono reference sees the same sum (x + x), so it gets 2x.
            let m = mono.tick(2.0 * x);
            let dry_l = x * 0.5;
            let dry_m = 2.0 * x * 0.5;
            // Compare the wet parts (output minus dry contribution).
            let wet_l = l - dry_l;
            let wet_m = m - dry_m;
            assert!(
                (wet_l - wet_m).abs() < 1e-5,
                "sample {i}: {wet_l} vs {wet_m}"
            );
        }
    }

    #[test]
    fn stereo_interleaved_matches_per_sample_tick() {
        let mut a = StereoFreeverb::new(SR, 0.6, 0.5, 0.4);
        let mut b = a.clone();
        let mut buf: Vec<f32> = (0..200).map(|i| ((i as f32) * 0.07).sin()).collect();
        let reference: Vec<f32> = buf
            .chunks_exact(2)
            .flat_map(|f| {
                let (l, r) = b.tick(f[0], f[1]);
                [l, r]
            })
            .collect();
        a.process_interleaved(&mut buf);
        assert_eq!(buf, reference);
    }

    #[test]
    fn stereo_invalid_parameters_are_sanitised() {
        let mut r = StereoFreeverb::new(f32::NAN, f32::NAN, f32::INFINITY, f32::NAN);
        for _ in 0..1000 {
            let (l, rr) = r.tick(f32::NAN, f32::INFINITY);
            assert!(l.is_finite() && rr.is_finite());
        }
    }
}
