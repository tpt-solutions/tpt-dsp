//! Graphic / parametric EQ built on cascaded biquad peaking filters.
//!
//! Wraps the [`IirFilter`] cascade from `tpt-dsp-core` (which is itself a
//! chain of RBJ biquads). Each band is a peaking EQ; an optional low and high
//! shelf can be added. Scratch space is reserved at construction time, so
//! [`Eq::process`] is allocation-free for any block up to the constructor's
//! `max_block` length.

use tpt_dsp_core::{Biquad, BiquadCoeffs, BiquadType, FilterDesignError, IirFilter};

/// Block length whose scratch space [`Eq::new`] reserves up front.
pub const DEFAULT_MAX_BLOCK: usize = 1024;

/// The design parameters of one band, kept so the cascade can be rebuilt
/// after a sample-rate change.
#[derive(Debug, Clone, Copy)]
struct BandSpec {
    kind: BiquadType,
    f0: f32,
    q: f32,
    gain_db: f32,
}

/// A multi-band parametric equalizer.
pub struct Eq {
    sample_rate: f32,
    specs: Vec<BandSpec>,
    filter: IirFilter<f32>,
    scratch: Vec<f32>,
    max_block: usize,
}

impl Eq {
    /// Create an EQ from peaking bands, reserving scratch for blocks of up to
    /// `DEFAULT_MAX_BLOCK` samples.
    ///
    /// Each band is `(center_hz, gain_db, q)`. Bands are applied in order.
    ///
    /// # Panics
    ///
    /// Panics on a band [`Eq::try_new`] rejects (e.g. centre above Nyquist).
    pub fn new(sample_rate: f32, bands: &[(f32, f32, f32)]) -> Self {
        Self::with_max_block(sample_rate, bands, DEFAULT_MAX_BLOCK)
    }

    /// Fallible version of [`Eq::new`]: returns an error instead of panicking
    /// for a non-finite or out-of-range band.
    pub fn try_new(sample_rate: f32, bands: &[(f32, f32, f32)]) -> Result<Self, FilterDesignError> {
        for &(f0, gain_db, q) in bands {
            BiquadCoeffs::<f32>::try_design(BiquadType::Peaking, sample_rate, f0, q, gain_db)?;
        }
        Ok(Self::new(sample_rate, bands))
    }

    /// Like [`Eq::new`], but reserves scratch for blocks of up to `max_block`
    /// samples. Longer blocks still work, at the cost of one allocation.
    pub fn with_max_block(sample_rate: f32, bands: &[(f32, f32, f32)], max_block: usize) -> Self {
        let specs = bands
            .iter()
            .map(|&(f0, gain_db, q)| BandSpec {
                kind: BiquadType::Peaking,
                f0,
                q,
                gain_db,
            })
            .collect();
        Self::from_specs(sample_rate, specs, max_block)
    }

    /// Create an EQ with low/high shelves and peaking mids.
    ///
    /// * `low_shelf` / `high_shelf` are `(corner_hz, gain_db)`.
    /// * `peaks` are `(center_hz, gain_db, q)`.
    ///
    /// # Panics
    ///
    /// Panics on a band that [`Eq::try_with_shelves`] rejects.
    pub fn with_shelves(
        sample_rate: f32,
        low_shelf: (f32, f32),
        peaks: &[(f32, f32, f32)],
        high_shelf: (f32, f32),
    ) -> Self {
        let mut specs = Vec::with_capacity(peaks.len() + 2);
        specs.push(BandSpec {
            kind: BiquadType::LowShelf,
            f0: low_shelf.0,
            q: 0.707,
            gain_db: low_shelf.1,
        });
        specs.extend(peaks.iter().map(|&(f0, gain_db, q)| BandSpec {
            kind: BiquadType::Peaking,
            f0,
            q,
            gain_db,
        }));
        specs.push(BandSpec {
            kind: BiquadType::HighShelf,
            f0: high_shelf.0,
            q: 0.707,
            gain_db: high_shelf.1,
        });
        Self::from_specs(sample_rate, specs, DEFAULT_MAX_BLOCK)
    }

    fn from_specs(sample_rate: f32, specs: Vec<BandSpec>, max_block: usize) -> Self {
        let filter = build_cascade(sample_rate, &specs, max_block);
        Self {
            sample_rate,
            specs,
            filter,
            scratch: vec![0.0; max_block],
            max_block,
        }
    }

    /// Set the sample rate in Hz and redesign every band.
    ///
    /// This rebuilds the filter cascade and is therefore **not** for the
    /// audio callback; call it from the control thread. Non-finite or
    /// non-positive rates are ignored.
    ///
    /// The change is ignored if it is invalid, or if any band's frequency
    /// would lie above the new Nyquist limit.
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        if sample_rate == self.sample_rate {
            return;
        }
        let valid = self.specs.iter().all(|s| {
            BiquadCoeffs::<f32>::try_design(s.kind, sample_rate, s.f0, s.q, s.gain_db).is_ok()
        });
        if valid {
            self.sample_rate = sample_rate;
            self.filter = build_cascade(sample_rate, &self.specs, self.max_block);
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

    /// Process one block in place.
    ///
    /// Allocation-free for blocks up to the constructor's `max_block`; a
    /// longer block grows the scratch buffer once.
    pub fn process(&mut self, buf: &mut [f32]) {
        if self.scratch.len() < buf.len() {
            self.scratch.resize(buf.len(), 0.0);
        }
        self.filter.process(buf, &mut self.scratch[..buf.len()]);
        buf.copy_from_slice(&self.scratch[..buf.len()]);
    }
}

fn build_cascade(sample_rate: f32, specs: &[BandSpec], max_block: usize) -> IirFilter<f32> {
    let stages: Vec<Biquad<f32>> = specs
        .iter()
        .map(|s| Biquad::<f32>::design(s.kind, sample_rate, s.f0, s.q, s.gain_db))
        .collect();
    IirFilter::with_max_block(stages, max_block)
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

    #[test]
    fn process_does_not_grow_scratch_within_max_block() {
        let mut eq = Eq::with_max_block(48000.0, &[(1000.0, 6.0, 1.0)], 128);
        let cap_before = eq.scratch.capacity();
        let mut block = [0.1f32; 128];
        for _ in 0..10 {
            eq.process(&mut block);
        }
        assert_eq!(eq.scratch.capacity(), cap_before);
        assert_eq!(eq.scratch.len(), 128);
    }

    fn impulse_response(eq: &mut Eq, len: usize) -> Vec<f32> {
        let mut buf = vec![0.0f32; len];
        buf[0] = 1.0;
        eq.process(&mut buf);
        buf
    }

    #[test]
    fn set_sample_rate_matches_fresh_design() {
        let mut a = Eq::new(48000.0, &[(1000.0, 12.0, 1.0)]);
        a.set_sample_rate(44100.0);
        let mut b = Eq::new(44100.0, &[(1000.0, 12.0, 1.0)]);
        assert_eq!(impulse_response(&mut a, 64), impulse_response(&mut b, 64));
    }

    #[test]
    fn invalid_sample_rate_is_ignored() {
        let mut eq = Eq::new(48000.0, &[(1000.0, 6.0, 1.0)]);
        let mut reference = Eq::new(48000.0, &[(1000.0, 6.0, 1.0)]);
        eq.set_sample_rate(f32::NAN);
        eq.set_sample_rate(0.0);
        assert_eq!(
            impulse_response(&mut eq, 64),
            impulse_response(&mut reference, 64)
        );
    }
}

#[cfg(test)]
mod try_tests {
    use super::*;

    #[test]
    fn try_new_rejects_band_above_nyquist() {
        assert!(matches!(
            Eq::try_new(48000.0, &[(30_000.0, 3.0, 1.0)]),
            Err(FilterDesignError::FrequencyOutOfRange)
        ));
        assert!(matches!(
            Eq::try_new(48000.0, &[(1000.0, 3.0, 0.0)]),
            Err(FilterDesignError::Q)
        ));
        assert!(Eq::try_new(48000.0, &[(1000.0, 3.0, 1.0)]).is_ok());
    }

    #[test]
    fn set_sample_rate_ignores_change_that_would_break_a_band() {
        // A 20 kHz band is valid at 48 kHz but above the 16 kHz Nyquist of an
        // 32 kHz rate, so the rate change must be refused, not panic.
        let mut eq = Eq::new(48000.0, &[(20_000.0, 3.0, 1.0)]);
        eq.set_sample_rate(32_000.0);
        assert_eq!(eq.sample_rate, 48000.0);
    }
}

impl Eq {
    /// Fallible version of [`Eq::with_shelves`]: returns an error instead of
    /// panicking for a non-finite or out-of-range band.
    pub fn try_with_shelves(
        sample_rate: f32,
        low_shelf: (f32, f32),
        peaks: &[(f32, f32, f32)],
        high_shelf: (f32, f32),
    ) -> Result<Self, FilterDesignError> {
        BiquadCoeffs::<f32>::try_design(
            BiquadType::LowShelf,
            sample_rate,
            low_shelf.0,
            0.707,
            low_shelf.1,
        )?;
        for &(f0, gain_db, q) in peaks {
            BiquadCoeffs::<f32>::try_design(BiquadType::Peaking, sample_rate, f0, q, gain_db)?;
        }
        BiquadCoeffs::<f32>::try_design(
            BiquadType::HighShelf,
            sample_rate,
            high_shelf.0,
            0.707,
            high_shelf.1,
        )?;
        Ok(Self::with_shelves(
            sample_rate,
            low_shelf,
            peaks,
            high_shelf,
        ))
    }
}

#[cfg(test)]
mod shelf_try_tests {
    use super::*;

    #[test]
    fn try_with_shelves_validates_every_band() {
        assert!(matches!(
            Eq::try_with_shelves(
                48000.0,
                (200.0, 3.0),
                &[(1000.0, 0.0, 1.0)],
                (30_000.0, -2.0)
            ),
            Err(FilterDesignError::FrequencyOutOfRange)
        ));
        assert!(matches!(
            Eq::try_with_shelves(48000.0, (200.0, 3.0), &[(1000.0, 0.0, 0.0)], (8000.0, -2.0)),
            Err(FilterDesignError::Q)
        ));
        assert!(
            Eq::try_with_shelves(48000.0, (200.0, 3.0), &[(1000.0, 0.0, 1.0)], (8000.0, -2.0))
                .is_ok()
        );
    }
}
