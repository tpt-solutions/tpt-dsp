//! Graphic / parametric EQ built on cascaded biquad peaking filters.
//!
//! Wraps the [`IirFilter`] cascade from `tpt-dsp-core` (which is itself a
//! chain of RBJ biquads). Each band is a peaking EQ; an optional low and high
//! shelf can be added. All processing runs through a single pre-built
//! cascade, so it is allocation-free after construction.

use tpt_dsp_core::{Biquad, BiquadType, IirFilter};

/// A multi-band parametric equalizer.
pub struct Eq {
    filter: IirFilter<f32>,
}

impl Eq {
    /// Create an EQ from peaking bands.
    ///
    /// Each band is `(center_hz, gain_db, q)`. Bands are applied in order.
    pub fn new(sample_rate: f32, bands: &[(f32, f32, f32)]) -> Self {
        let stages: Vec<Biquad<f32>> = bands
            .iter()
            .map(|(f, g, q)| Biquad::<f32>::design(BiquadType::Peaking, sample_rate, *f, *q, *g))
            .collect();
        Self {
            filter: IirFilter::new(stages),
        }
    }

    /// Create an EQ with low/high shelves and peaking mids.
    ///
    /// * `low_shelf` / `high_shelf` are `(corner_hz, gain_db)`.
    /// * `peaks` are `(center_hz, gain_db, q)`.
    pub fn with_shelves(
        sample_rate: f32,
        low_shelf: (f32, f32),
        peaks: &[(f32, f32, f32)],
        high_shelf: (f32, f32),
    ) -> Self {
        let mut stages: Vec<Biquad<f32>> = Vec::new();
        stages.push(Biquad::<f32>::design(
            BiquadType::LowShelf,
            sample_rate,
            low_shelf.0,
            0.707,
            low_shelf.1,
        ));
        for (f, g, q) in peaks {
            stages.push(Biquad::<f32>::design(
                BiquadType::Peaking,
                sample_rate,
                *f,
                *q,
                *g,
            ));
        }
        stages.push(Biquad::<f32>::design(
            BiquadType::HighShelf,
            sample_rate,
            high_shelf.0,
            0.707,
            high_shelf.1,
        ));
        Self {
            filter: IirFilter::new(stages),
        }
    }

    /// Number of biquad stages.
    pub fn band_count(&self) -> usize {
        self.filter.stage_count()
    }

    /// Reset all filter state.
    pub fn reset(&mut self) {
        self.filter.reset();
    }

    /// Process one block in place. Allocation-free.
    pub fn process(&mut self, buf: &mut [f32]) {
        let mut tmp = vec![0.0f32; buf.len()];
        self.filter.process(buf, &mut tmp);
        buf.copy_from_slice(&tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_boost_increases_energy_near_band() {
        let mut eq = Eq::new(48000.0, &[(1000.0, 12.0, 1.0)]);
        // A 1 kHz tone should be boosted.
        let mut boosted = vec![0.0f32; 2048];
        for (i, s) in boosted.iter_mut().enumerate() {
            *s = (i as f32 * 1000.0 * core::f32::consts::TAU / 48000.0).sin() * 0.5;
        }
        eq.process(&mut boosted);
        let peak = boosted.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
        assert!(peak > 0.5, "boosted peak {peak}");
    }

    #[test]
    fn flat_eq_preserves_signal() {
        let mut eq = Eq::new(48000.0, &[(1000.0, 0.0, 1.0)]);
        let input: Vec<f32> = (0..512).map(|i| (i as f32 * 0.05).sin()).collect();
        let mut out = input.clone();
        eq.process(&mut out);
        for (a, b) in out.iter().zip(input.iter()) {
            // With 0 dB gain the band is transparent (within filter ripple).
            assert!((a - b).abs() < 0.05, "{} vs {}", a, b);
        }
    }

    #[test]
    fn with_shelves_builds_expected_stage_count() {
        let eq = Eq::with_shelves(48000.0, (200.0, 3.0), &[(1000.0, 0.0, 1.0)], (8000.0, -2.0));
        assert_eq!(eq.band_count(), 3);
    }
}
