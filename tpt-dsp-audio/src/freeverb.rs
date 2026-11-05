//! Algorithmic reverb in the style of Freeverb.
//!
//! Eight parallel feedback comb filters (each with a one-pole low-pass in the
//! feedback path for high-frequency damping) feed four series all-pass
//! filters. The delay lengths are the Freeverb tunings, scaled from 44.1 kHz to
//! the actual sample rate. All buffers are allocated in [`Freeverb::new`], so
//! processing is allocation-free.
//!
//! This is a mono reverb. Freeverb's stereo spread is not implemented.

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
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

/// Freeverb-style mono reverb.
#[derive(Debug, Clone)]
pub struct Freeverb {
    combs: Vec<Comb>,
    allpasses: Vec<AllPass>,
    feedback: f32,
    damp: f32,
    wet: f32,
}

fn scaled(len: usize, sample_rate: f32) -> usize {
    ((len as f32) * sample_rate / TUNING_RATE).round().max(1.0) as usize
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
        let sr = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut fv = Self {
            combs: COMB_TUNING
                .iter()
                .map(|&n| Comb::new(scaled(n, sr)))
                .collect(),
            allpasses: ALLPASS_TUNING
                .iter()
                .map(|&n| AllPass::new(scaled(n, sr)))
                .collect(),
            feedback: 0.0,
            damp: 0.0,
            wet: 0.0,
        };
        fv.set_room_size(room_size);
        fv.set_damping(damping);
        fv.set_wet(wet);
        fv
    }

    /// Set the room size in `[0, 1]` (NaN becomes 0.5).
    pub fn set_room_size(&mut self, room: f32) {
        let r = if room.is_finite() {
            room.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.feedback = r * SCALE_ROOM + OFFSET_ROOM;
    }

    /// Set the damping in `[0, 1]` (NaN becomes 0.5).
    pub fn set_damping(&mut self, damping: f32) {
        let d = if damping.is_finite() {
            damping.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.damp = d * SCALE_DAMP;
    }

    /// Set the wet/dry mix in `[0, 1]` (NaN becomes 0).
    pub fn set_wet(&mut self, wet: f32) {
        self.wet = if wet.is_finite() {
            wet.clamp(0.0, 1.0)
        } else {
            0.0
        };
    }

    /// Clear all delay lines.
    pub fn reset(&mut self) {
        self.combs.iter_mut().for_each(Comb::reset);
        self.allpasses.iter_mut().for_each(AllPass::reset);
    }

    /// Process one sample.
    pub fn tick(&mut self, x: f32) -> f32 {
        let x = if x.is_finite() { x } else { 0.0 };
        let input = x * FIXED_GAIN;
        let mut acc = 0.0f32;
        for c in self.combs.iter_mut() {
            acc += c.tick(input, self.feedback, self.damp);
        }
        for a in self.allpasses.iter_mut() {
            acc = a.tick(acc);
        }
        x * (1.0 - self.wet) + acc * self.wet
    }

    /// Process a block in place.
    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.tick(*s);
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
}
