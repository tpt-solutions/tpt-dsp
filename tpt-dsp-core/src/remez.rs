//! Parks–McClellan equiripple linear-phase FIR design (the Remez exchange
//! algorithm).
//!
//! Given a set of frequency bands, each with a desired response and a weight,
//! the design finds the odd-length filter whose weighted error is equiripple:
//! the peak error is as small as it can be for the tap count. It is the
//! optimal FIR for a given transition band, and it is what most DSP texts mean
//! by "the Parks–McClellan filter".
//!
//! The algorithm alternates two steps. It solves for the error and the
//! interpolating polynomial on a set of `M + 2` extremal frequencies, then
//! moves the extremal set to the peaks of the new error, until the peaks are
//! equal. The polynomial is evaluated with barycentric Lagrange interpolation
//! in `x = cos(2πf)`.
//!
//! The design runs in `f64` and allocates its working set. It is an offline
//! design step, not a per-sample process.

use alloc::vec::Vec;

// `alloc`-only builds have no inherent float math; see `windows.rs`.
#[allow(unused_imports)]
use num_traits::Float;

use crate::filters::FilterDesignError;

/// One frequency band for [`remez_fir`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemezBand {
    /// Lower edge, in cycles per sample (`0.0..0.5`).
    pub start: f32,
    /// Upper edge, in cycles per sample (`start..0.5`).
    pub end: f32,
    /// Desired amplitude across the band (for example `1.0` in a passband).
    pub desired: f32,
    /// Relative weight of the error in this band (`> 0`). A larger weight
    /// makes this band's ripple smaller at the expense of the others.
    pub weight: f32,
}

/// Grid points per extremal-set degree of freedom. Denser grids find the true
/// peaks more reliably, at higher cost.
const GRID_DENSITY: usize = 16;
const MAX_ITERATIONS: usize = 80;
const RELATIVE_TOLERANCE: f64 = 1e-7;

/// Design an odd-length, linear-phase FIR by the Parks–McClellan method.
///
/// Returns `taps` coefficients, symmetric about the centre.
///
/// # Errors
/// Returns an error for an even or zero tap count, an empty or unsorted band
/// list, bands outside `[0, 0.5)`, or a non-positive weight.
pub fn remez_fir(taps: usize, bands: &[RemezBand]) -> Result<Vec<f32>, FilterDesignError> {
    if taps == 0 {
        return Err(FilterDesignError::EmptyTaps);
    }
    if taps % 2 == 0 {
        return Err(FilterDesignError::EvenTapCount);
    }
    validate_bands(bands)?;
    if taps == 1 {
        // A single tap is an impulse; its level is the desired gain of the
        // first band.
        return Ok(alloc::vec![bands[0].desired]);
    }

    let m = (taps - 1) / 2;
    let nfreq = m + 2;

    // Dense grid over every band.
    let mut gf: Vec<f64> = Vec::new();
    let mut gd: Vec<f64> = Vec::new();
    let mut gw: Vec<f64> = Vec::new();
    for b in bands {
        let (lo, hi) = (f64::from(b.start), f64::from(b.end));
        let span = hi - lo;
        let pts = ((GRID_DENSITY * (m + 1)) as f64 * span / 0.5)
            .ceil()
            .max(2.0) as usize;
        for j in 0..pts {
            gf.push(lo + span * j as f64 / (pts - 1) as f64);
            gd.push(f64::from(b.desired));
            gw.push(f64::from(b.weight));
        }
    }
    let g = gf.len();
    if g < nfreq {
        return Err(FilterDesignError::FrequencyOutOfRange);
    }
    let gx: Vec<f64> = gf
        .iter()
        .map(|&f| (core::f64::consts::TAU * f).cos())
        .collect();

    // Initial extremal set: spread evenly over the grid.
    let mut ext: Vec<usize> = (0..nfreq).map(|k| k * (g - 1) / (nfreq - 1)).collect();

    let mut last: Option<Solution> = None;
    for _ in 0..MAX_ITERATIONS {
        let sol = solve(&ext, &gx, &gd, &gw);
        // Error on the whole grid: E = W·(D − H).
        let err: Vec<f64> = (0..g)
            .map(|i| gw[i] * (gd[i] - eval(gx[i], &sol)))
            .collect();
        let max_err = err.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        let converged =
            max_err > 0.0 && (max_err - sol.delta.abs()) <= RELATIVE_TOLERANCE * max_err;
        last = Some(sol);
        if converged {
            break;
        }

        // New extremal set: the peak of each run of same-signed error.
        let mut peaks: Vec<usize> = Vec::new();
        let mut run_sign = 0.0f64;
        let mut run_best: Option<usize> = None;
        for (i, &e) in err.iter().enumerate() {
            let s = if e >= 0.0 { 1.0 } else { -1.0 };
            if run_best.is_none() || s != run_sign {
                if let Some(b) = run_best {
                    peaks.push(b);
                }
                run_sign = s;
                run_best = Some(i);
            } else if let Some(b) = run_best {
                if e.abs() > err[b].abs() {
                    run_best = Some(i);
                }
            }
        }
        if let Some(b) = run_best {
            peaks.push(b);
        }
        // Too many peaks: drop the smallest from whichever end is weaker.
        while peaks.len() > nfreq {
            let first = err[peaks[0]].abs();
            let last_e = err[*peaks.last().unwrap()].abs();
            if first < last_e {
                peaks.remove(0);
            } else {
                peaks.pop();
            }
        }
        if peaks.len() < nfreq {
            // The exchange cannot make progress; keep the current solution.
            break;
        }
        ext = peaks;
    }

    let sol = last.expect("at least one iteration runs");
    // Frequency-sample the amplitude response at f = k/N and invert the DFT.
    let n = taps as f64;
    let amp: Vec<f64> = (0..=m)
        .map(|k| eval((core::f64::consts::TAU * k as f64 / n).cos(), &sol))
        .collect();
    let mut h = Vec::with_capacity(taps);
    for i in 0..taps {
        let shift = i as f64 - m as f64;
        let mut acc = amp[0];
        for (k, a) in amp.iter().enumerate().skip(1) {
            acc += 2.0 * a * (core::f64::consts::TAU * k as f64 * shift / n).cos();
        }
        h.push((acc / n) as f32);
    }
    Ok(h)
}

fn validate_bands(bands: &[RemezBand]) -> Result<(), FilterDesignError> {
    if bands.is_empty() {
        return Err(FilterDesignError::FrequencyOutOfRange);
    }
    let mut prev_end = 0.0f32;
    for b in bands {
        let ok = b.start.is_finite()
            && b.end.is_finite()
            && b.desired.is_finite()
            && b.weight.is_finite()
            && b.weight > 0.0
            && b.start >= 0.0
            && b.start < b.end
            && b.end <= 0.5
            && b.start >= prev_end;
        if !ok {
            return Err(FilterDesignError::FrequencyOutOfRange);
        }
        prev_end = b.end;
    }
    Ok(())
}

/// The interpolating rational function for one extremal set.
struct Solution {
    xk: Vec<f64>,
    bw: Vec<f64>,
    ck: Vec<f64>,
    delta: f64,
}

fn solve(ext: &[usize], gx: &[f64], gd: &[f64], gw: &[f64]) -> Solution {
    let k = ext.len();
    let xk: Vec<f64> = ext.iter().map(|&i| gx[i]).collect();
    let dk: Vec<f64> = ext.iter().map(|&i| gd[i]).collect();
    let wk: Vec<f64> = ext.iter().map(|&i| gw[i]).collect();

    // Barycentric weights bw_j = 1 / Π_{i≠j}(x_j − x_i).
    let bw: Vec<f64> = (0..k)
        .map(|j| {
            let mut prod = 1.0f64;
            for i in 0..k {
                if i != j {
                    prod *= xk[j] - xk[i];
                }
            }
            1.0 / prod
        })
        .collect();

    // δ = Σ bw·D / Σ bw·(−1)^j / W_j.
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for j in 0..k {
        let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
        num += bw[j] * dk[j];
        den += bw[j] * sign / wk[j];
    }
    let delta = num / den;

    let ck: Vec<f64> = (0..k)
        .map(|j| {
            let sign = if j % 2 == 0 { 1.0 } else { -1.0 };
            dk[j] - sign * delta / wk[j]
        })
        .collect();

    Solution { xk, bw, ck, delta }
}

/// Evaluate the interpolant at `x` (barycentric form).
fn eval(x: f64, s: &Solution) -> f64 {
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for j in 0..s.xk.len() {
        let d = x - s.xk[j];
        if d.abs() < 1e-14 {
            return s.ck[j];
        }
        let t = s.bw[j] / d;
        num += t * s.ck[j];
        den += t;
    }
    num / den
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lowpass(taps: usize) -> Vec<f32> {
        remez_fir(
            taps,
            &[
                RemezBand {
                    start: 0.0,
                    end: 0.2,
                    desired: 1.0,
                    weight: 1.0,
                },
                RemezBand {
                    start: 0.3,
                    end: 0.5,
                    desired: 0.0,
                    weight: 1.0,
                },
            ],
        )
        .unwrap()
    }

    fn response(h: &[f32], f: f64) -> f64 {
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (n, &c) in h.iter().enumerate() {
            let w = core::f64::consts::TAU * f * n as f64;
            re += f64::from(c) * w.cos();
            im -= f64::from(c) * w.sin();
        }
        (re * re + im * im).sqrt()
    }

    #[test]
    fn lowpass_is_symmetric_and_has_unity_passband() {
        let h = lowpass(31);
        assert_eq!(h.len(), 31);
        for i in 0..15 {
            assert!((h[i] - h[30 - i]).abs() < 1e-6, "asymmetric at {i}");
        }
        let dc = response(&h, 0.0);
        assert!((dc - 1.0).abs() < 0.02, "dc {dc}");
    }

    #[test]
    fn equiripple_stopband_is_flat_and_deep() {
        let h = lowpass(31);
        let mut peak = 0.0f64;
        for k in 0..200 {
            let f = 0.3 + 0.2 * k as f64 / 199.0;
            peak = peak.max(response(&h, f));
        }
        // A 31-tap design with this transition band reaches about −40 dB.
        assert!(peak < 0.02, "stopband peak {peak}");
    }

    #[test]
    fn rejects_bad_specifications() {
        let band = RemezBand {
            start: 0.0,
            end: 0.2,
            desired: 1.0,
            weight: 1.0,
        };
        assert!(matches!(
            remez_fir(0, &[band]),
            Err(FilterDesignError::EmptyTaps)
        ));
        assert!(matches!(
            remez_fir(30, &[band]),
            Err(FilterDesignError::EvenTapCount)
        ));
        assert!(remez_fir(31, &[]).is_err());
        let bad = RemezBand { end: 0.6, ..band };
        assert!(remez_fir(31, &[bad]).is_err());
        let zero_weight = RemezBand {
            weight: 0.0,
            ..band
        };
        assert!(remez_fir(31, &[zero_weight]).is_err());
    }
}
