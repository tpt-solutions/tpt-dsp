//! RF and multirate building blocks: Goertzel tone detector, numerically
//! controlled oscillator (NCO / complex mixer), and a CIC decimator.
//!
//! [`Goertzel`] and [`Nco`] are allocation-free and `no_std`. [`CicDecimator`]
//! keeps its integrator and comb state in a `Vec`, so it needs `alloc`.

// `no_std` builds have no inherent float math; see the note in `windows.rs`.
#[allow(unused_imports)]
use num_traits::Float;

/// Goertzel algorithm: the power of one DFT bin, computed recursively.
///
/// Cheaper than a full FFT when only a few frequencies matter (DTMF, pilot
/// tones, tone-detection telemetry).
#[derive(Debug, Clone, Copy)]
pub struct Goertzel {
    coeff: f32,
    s1: f32,
    s2: f32,
}

impl Goertzel {
    /// Detector for `freq` Hz at `sample_rate` Hz.
    pub fn new(sample_rate: f32, freq: f32) -> Self {
        Self {
            coeff: 2.0 * (core::f32::consts::TAU * freq / sample_rate).cos(),
            s1: 0.0,
            s2: 0.0,
        }
    }

    /// Clear the recursion state (call between blocks).
    pub fn reset(&mut self) {
        self.s1 = 0.0;
        self.s2 = 0.0;
    }

    /// Feed one sample.
    pub fn tick(&mut self, x: f32) {
        let s0 = x + self.coeff * self.s1 - self.s2;
        self.s2 = self.s1;
        self.s1 = s0;
    }

    /// Squared magnitude of the bin after the samples fed so far.
    pub fn power(&self) -> f32 {
        self.s1 * self.s1 + self.s2 * self.s2 - self.coeff * self.s1 * self.s2
    }

    /// Magnitude of the bin, `sqrt(power)`.
    pub fn magnitude(&self) -> f32 {
        self.power().max(0.0).sqrt()
    }
}

/// Numerically controlled oscillator with a 32-bit phase accumulator.
///
/// The phase wraps in integer arithmetic, so there is no drift or accumulation
/// error over long runs. Frequency resolution is `sample_rate / 2³²`.
#[derive(Debug, Clone, Copy)]
pub struct Nco {
    phase: u32,
    step: u32,
    sample_rate: f32,
}

impl Nco {
    /// Create an NCO at `freq` Hz. Negative or non-finite frequencies are
    /// treated as 0; an invalid sample rate falls back to 48 kHz.
    pub fn new(sample_rate: f32, freq: f32) -> Self {
        let sample_rate = if sample_rate.is_finite() && sample_rate > 0.0 {
            sample_rate
        } else {
            48_000.0
        };
        let mut n = Self {
            phase: 0,
            step: 0,
            sample_rate,
        };
        n.set_frequency(freq);
        n
    }

    /// Set the frequency in Hz. Values outside `[0, sample_rate / 2)` are
    /// accepted (they alias), but non-finite or negative values become 0.
    pub fn set_frequency(&mut self, freq: f32) {
        let f = if freq.is_finite() && freq >= 0.0 {
            freq
        } else {
            0.0
        };
        // Wrap modulo one full cycle before converting, so large frequencies
        // alias instead of saturating.
        // Computed in f64 and rounded: f32 here drifts by about 3 units per
        // sample at 1 kHz / 48 kHz, which accumulates into audible phase error.
        let cycles = (f64::from(f) / f64::from(self.sample_rate)).fract();
        self.step = (cycles * 4_294_967_296.0).round() as u32;
    }

    /// Reset the phase to zero.
    pub fn reset(&mut self) {
        self.phase = 0;
    }

    /// Advance one sample and return `(cos θ, sin θ)` for the current phase.
    pub fn tick(&mut self) -> (f32, f32) {
        let theta = core::f32::consts::TAU * (self.phase as f32 / 4_294_967_296.0);
        self.phase = self.phase.wrapping_add(self.step);
        (theta.cos(), theta.sin())
    }

    /// Mix `x` down by the oscillator, returning `(I, Q)` = `(x·cos, −x·sin)`.
    pub fn mix(&mut self, x: f32) -> (f32, f32) {
        let (c, s) = self.tick();
        (x * c, -x * s)
    }
}

/// Cascaded-integrator-comb (CIC) decimator by an integer factor `R`.
///
/// `order` integrator/comb pairs give a sinc^order low-pass with nulls at the
/// output rate. The output is normalised to unity DC gain. CIC filters are
/// multiplier-free in hardware, but this float version is simpler to verify.
#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub struct CicDecimator {
    decimation: usize,
    order: usize,
    integrators: alloc::vec::Vec<f64>,
    comb_delays: alloc::vec::Vec<f64>,
    phase: usize,
    gain: f64,
}

#[cfg(feature = "alloc")]
impl CicDecimator {
    /// Create a decimator. `decimation` (`R ≥ 1`) and `order` (`N ≥ 1`) are
    /// clamped to at least 1.
    pub fn new(decimation: usize, order: usize) -> Self {
        let r = decimation.max(1);
        let n = order.max(1);
        Self {
            decimation: r,
            order: n,
            integrators: alloc::vec![0.0; n],
            comb_delays: alloc::vec![0.0; n],
            phase: 0,
            gain: (r as f64).powi(n as i32),
        }
    }

    /// The decimation factor.
    pub fn decimation(&self) -> usize {
        self.decimation
    }

    /// Clear all state.
    pub fn reset(&mut self) {
        for v in self
            .integrators
            .iter_mut()
            .chain(self.comb_delays.iter_mut())
        {
            *v = 0.0;
        }
        self.phase = 0;
    }

    /// Feed one input sample; returns `Some(output)` once every `R` inputs.
    pub fn push(&mut self, x: f32) -> Option<f32> {
        let mut v = f64::from(if x.is_finite() { x } else { 0.0 });
        for acc in self.integrators.iter_mut() {
            *acc += v;
            v = *acc;
        }
        self.phase += 1;
        if self.phase < self.decimation {
            return None;
        }
        self.phase = 0;
        let mut y = v;
        for delay in self.comb_delays.iter_mut() {
            let next = y - *delay;
            *delay = y;
            y = next;
        }
        Some((y / self.gain) as f32)
    }

    /// Decimate a block, appending outputs to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut alloc::vec::Vec<f32>) {
        for &x in input {
            if let Some(y) = self.push(x) {
                out.push(y);
            }
        }
    }

    /// Number of integrator/comb stages.
    pub fn order(&self) -> usize {
        self.order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: f32 = 48_000.0;

    #[test]
    fn goertzel_peaks_at_its_own_frequency() {
        let tone = |f: f32| -> Vec<f32> {
            (0..960)
                .map(|i| (i as f32 * f * core::f32::consts::TAU / FS).sin())
                .collect()
        };
        let x = tone(1000.0);
        let mut on = Goertzel::new(FS, 1000.0);
        let mut off = Goertzel::new(FS, 2500.0);
        for &s in &x {
            on.tick(s);
            off.tick(s);
        }
        assert!(on.magnitude() > 100.0 * off.magnitude().max(1e-6));
        // Amplitude-1 sine over N samples has bin magnitude ≈ N/2.
        assert!((on.magnitude() - 480.0).abs() < 5.0, "{}", on.magnitude());
    }

    #[test]
    fn nco_mix_of_matching_tone_is_dc() {
        let mut nco = Nco::new(FS, 3000.0);
        let mut sum_i = 0.0f64;
        let n = 48_000;
        for i in 0..n {
            let x = (i as f32 * 3000.0 * core::f32::consts::TAU / FS).cos();
            let (iq_i, _q) = nco.mix(x);
            sum_i += f64::from(iq_i);
        }
        // The I branch of a matching mix has mean 1/2 (cos² averages to ½).
        assert!(
            (sum_i / n as f64 - 0.5).abs() < 0.01,
            "{}",
            sum_i / n as f64
        );
    }

    #[test]
    fn nco_frequency_is_exact_over_long_runs() {
        // 1 kHz at 48 kHz repeats every 48 samples exactly.
        let mut nco = Nco::new(FS, 1000.0);
        let first: Vec<(f32, f32)> = (0..48).map(|_| nco.tick()).collect();
        for _ in 0..1000 {
            for expected in &first {
                let (c, s) = nco.tick();
                assert!((c - expected.0).abs() < 1e-4 && (s - expected.1).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn nco_rejects_invalid_frequency() {
        let mut nco = Nco::new(FS, f32::NAN);
        let (c, s) = nco.tick();
        assert!((c - 1.0).abs() < 1e-6 && s.abs() < 1e-6);
    }

    #[test]
    fn cic_has_unity_dc_gain() {
        let mut cic = CicDecimator::new(8, 3);
        let mut out = Vec::new();
        cic.process(&vec![0.75f32; 8 * 200], &mut out);
        let last = *out.last().unwrap();
        assert!((last - 0.75).abs() < 1e-5, "dc {last}");
        assert_eq!(out.len(), 200);
    }

    #[test]
    fn cic_attenuates_frequencies_near_the_output_rate() {
        // A tone at the output sample rate (FS/8) sits on a CIC null.
        let mut cic = CicDecimator::new(8, 4);
        let mut out = Vec::new();
        let input: Vec<f32> = (0..8 * 400)
            .map(|i| (i as f32 * (FS / 8.0) * core::f32::consts::TAU / FS).sin())
            .collect();
        cic.process(&input, &mut out);
        let peak = out[100..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak < 1e-3, "null leaked {peak}");
    }
}
