//! Line Spectral Pairs (LSP) / Line Spectral Frequencies (LSF): an
//! alternative representation of LPC coefficients that is more robust to
//! quantization and safe to interpolate — two properties raw LPC
//! coefficients don't have (quantizing or interpolating them can produce
//! an unstable synthesis filter; LSP frequencies stay ordered under both
//! operations, which is exactly what preserves stability).
//!
//! Useful for speech coding (`tpt-media`'s Vox).
//!
//! # Convention
//!
//! For an order-`p` (even) predictor `A(z) = 1 - Σ_{k=1}^{p} a_k z^{-k}`
//! (matching [`crate::lpc`]'s convention: `a_k` is `coeffs[k-1]`), define
//! the symmetric/antisymmetric polynomials
//!
//! ```text
//! P(z) = A(z) + z^-(p+1) A(z^-1)   (symmetric,  guaranteed root at z=-1)
//! Q(z) = A(z) - z^-(p+1) A(z^-1)   (antisymmetric, guaranteed root at z=1)
//! ```
//!
//! If `A(z)` is minimum-phase (a stable predictor), the remaining `p/2`
//! roots of each of `P`/`Q` all lie on the unit circle and strictly
//! interlace in `(0, π)` — the LSP frequencies. [`lpc_to_lsp`] finds them
//! by evaluating `P`/`Q` on a grid of angles and bisecting sign changes
//! (a direct, if unsophisticated, root search — see its doc for why this
//! is a deliberate choice over the classical Chebyshev-recursion
//! formulation). [`lsp_to_lpc`] reconstructs `A(z)` from the same
//! frequencies (`P + Q = 2A`) via the interlacing quadratic factors.

use num_traits::Float;

/// Evaluate the "combined chirp sum" that both `P(z)`'s real part and
/// `Q(z)`'s imaginary part reduce to on the unit circle (see the module
/// doc's derivation): for `symmetric = true` (P), `Σ f(m)·cos(ω·(m-c))`;
/// for `symmetric = false` (Q), `Σ f(m)·sin(ω·(m-c))`, where `c=(p+1)/2`
/// and `f(m) = c_ext(m) ± c_ext(p+1-m)`. `c_ext(k)` is `1` at `k=0`,
/// `-coeffs[k-1]` for `k=1..=p`, and `0` at `k=p+1`.
fn chirp_sum<F: Float>(coeffs: &[F], omega: F, symmetric: bool) -> F {
    let p = coeffs.len();
    let c = F::from(p + 1).unwrap() / F::from(2).unwrap();
    let c_ext = |k: usize| -> F {
        if k == 0 {
            F::one()
        } else if k <= p {
            -coeffs[k - 1]
        } else {
            F::zero()
        }
    };
    let mut acc = F::zero();
    for m in 0..=(p + 1) {
        let f_m = if symmetric {
            c_ext(m) + c_ext(p + 1 - m)
        } else {
            c_ext(m) - c_ext(p + 1 - m)
        };
        let theta = omega * (F::from(m).unwrap() - c);
        acc = acc + f_m * if symmetric { theta.cos() } else { theta.sin() };
    }
    acc
}

/// Locate `count` sign changes of `f` over `(0, π)`, sampled at
/// `grid_points` intervals starting `grid_points`-fraction `start_frac`
/// into the range (to dodge a known trivial root sitting exactly at one
/// endpoint), refining each via bisection. Appends results to `out`,
/// stopping once `count` roots are found. Returns the number found (may
/// be less than `count` if the grid was too coarse or missed a root).
fn find_roots<F: Float>(
    f: impl Fn(F) -> F,
    grid_points: usize,
    start_index: usize,
    count: usize,
    out: &mut [F],
) -> usize {
    let pi = F::from(core::f64::consts::PI).unwrap();
    let step = pi / F::from(grid_points).unwrap();
    let mut found = 0;
    let mut prev_omega = F::from(start_index).unwrap() * step;
    let mut prev_val = f(prev_omega);
    let mut i = start_index + 1;
    while found < count && i < grid_points {
        let omega = F::from(i).unwrap() * step;
        let val = f(omega);
        if (prev_val <= F::zero() && val > F::zero()) || (prev_val >= F::zero() && val < F::zero()) {
            // Bisect within (prev_omega, omega) for a tighter estimate.
            let mut lo = prev_omega;
            let mut hi = omega;
            let mut lo_val = prev_val;
            for _ in 0..40 {
                let mid = (lo + hi) / F::from(2).unwrap();
                let mid_val = f(mid);
                if (lo_val <= F::zero() && mid_val > F::zero()) || (lo_val >= F::zero() && mid_val < F::zero()) {
                    hi = mid;
                } else {
                    lo = mid;
                    lo_val = mid_val;
                }
            }
            out[found] = (lo + hi) / F::from(2).unwrap();
            found += 1;
        }
        prev_omega = omega;
        prev_val = val;
        i += 1;
    }
    found
}

/// Convert an LSP frequency (radians, in `(0, π)`, `π` = Nyquist) to a
/// Line Spectral *Frequency* in Hz: `lsp_rad * sample_rate / (2π)`.
pub fn lsp_rad_to_lsf_hz<F: Float>(lsp_rad: F, sample_rate: F) -> F {
    lsp_rad * sample_rate / F::from(core::f64::consts::TAU).unwrap()
}

/// Inverse of [`lsp_rad_to_lsf_hz`]: convert a Line Spectral Frequency in
/// Hz back to an LSP frequency in radians.
pub fn lsf_hz_to_lsp_rad<F: Float>(lsf_hz: F, sample_rate: F) -> F {
    lsf_hz * F::from(core::f64::consts::TAU).unwrap() / sample_rate
}

/// Default grid resolution for [`lpc_to_lsp`] — fine enough to separate
/// closely-spaced roots (sharp formant peaks) for the LPC orders speech
/// coding typically uses (8-20).
pub const DEFAULT_GRID_POINTS: usize = 400;

/// Convert LPC coefficients (order `p = coeffs.len()`, must be even) to
/// `p` LSP frequencies (radians, in `(0, π)`, strictly increasing,
/// alternating between the `P`- and `Q`-root families) via direct root
/// search on a grid of `grid_points` angles spanning `(0, π)`.
///
/// A direct evaluate-and-bisect search (rather than the classical
/// Chebyshev-polynomial recursion some implementations use to convert the
/// same problem into a faster polynomial root search) was chosen because
/// it needs no additional scratch buffers or coefficient transformation —
/// just the LPC coefficients and a grid resolution — and is easy to
/// verify directly against [`lsp_to_lpc`] by round trip.
///
/// Returns `false` (leaving `out` unspecified) if fewer than `p` roots
/// were found — this happens if `coeffs` doesn't describe a
/// stable/minimum-phase predictor (LSP roots only interlace on the unit
/// circle for a stable filter), or if `grid_points` is too coarse.
///
/// # Panics
///
/// Panics if `coeffs.len() == 0`, is odd, `out.len() != coeffs.len()`, or
/// `grid_points < 8`.
pub fn lpc_to_lsp<F: Float>(coeffs: &[F], out: &mut [F], grid_points: usize) -> bool {
    let p = coeffs.len();
    assert!(p > 0, "LPC order must be positive");
    assert_eq!(p % 2, 0, "LSP conversion requires an even LPC order");
    assert_eq!(out.len(), p, "out length must equal LPC order");
    assert!(grid_points >= 8, "grid_points must be at least 8");

    let half = p / 2;
    let mut p_roots = alloc_stack_buf::<F>(half);
    let mut q_roots = alloc_stack_buf::<F>(half);

    // P's trivial root sits at omega=pi (the far end), so its search can
    // start right at omega=0; Q's trivial root sits at omega=0, so its
    // search must skip past the start.
    let p_found = find_roots(|w| chirp_sum(coeffs, w, true), grid_points, 0, half, &mut p_roots);
    let q_found = find_roots(|w| chirp_sum(coeffs, w, false), grid_points, 1, half, &mut q_roots);
    if p_found != half || q_found != half {
        return false;
    }

    // Interlace: merge the two sorted root lists into one increasing
    // sequence (both are already increasing, since `find_roots` scans
    // omega upward).
    let (mut i, mut j, mut k) = (0, 0, 0);
    while i < half && j < half {
        if p_roots[i] < q_roots[j] {
            out[k] = p_roots[i];
            i += 1;
        } else {
            out[k] = q_roots[j];
            j += 1;
        }
        k += 1;
    }
    while i < half {
        out[k] = p_roots[i];
        i += 1;
        k += 1;
    }
    while j < half {
        out[k] = q_roots[j];
        j += 1;
        k += 1;
    }
    true
}

/// Reconstruct LPC coefficients (order `lsp.len()`, written to
/// `out[0..lsp.len()]`) from `p` LSP frequencies, by rebuilding `P(z)` and
/// `Q(z)` from their interlacing roots (plus each family's trivial root)
/// and taking `A(z) = (P(z)+Q(z))/2`.
///
/// `lsp` need not be exactly the output of [`lpc_to_lsp`] — any valid
/// (see [`lsp_is_stable`]) sorted, alternating set works, e.g. after
/// quantization or interpolation.
///
/// # Panics
///
/// Panics if `lsp.len() == 0`, is odd, or `out.len() != lsp.len()`.
pub fn lsp_to_lpc<F: Float>(lsp: &[F], out: &mut [F]) {
    let p = lsp.len();
    assert!(p > 0, "LSP set must be non-empty");
    assert_eq!(p % 2, 0, "LSP conversion requires an even LPC order");
    assert_eq!(out.len(), p, "out length must equal LSP length");

    // By construction (see `lpc_to_lsp`), P- and Q-family roots strictly
    // alternate in sorted order; which family occupies even vs. odd
    // indices is fixed by P having no trivial root at omega=0 (so its
    // smallest root can be closer to 0 than Q's smallest, whichever
    // happens to interlace first) — rather than assume an index parity,
    // rebuild both hypotheses is unnecessary: `lpc_to_lsp`'s interlace
    // loop always emits whichever of `p_roots[i]`/`q_roots[j]` is
    // smaller first, so the *first* entry could be from either family.
    // What's fixed is that they still strictly alternate P,Q,P,Q,... or
    // Q,P,Q,P,...; since P(z) and Q(z) only differ by which trivial
    // factor they carry ((1+z^-1) for P, (1-z^-1) for Q) and are
    // otherwise built the same way from the *same* interlaced root set,
    // reconstructing "P" from the even-indexed roots and "Q" from the
    // odd-indexed roots (or vice versa) and adding them is symmetric in
    // the sense that swapping the assignment swaps which trivial factor
    // attaches to which half — but since we only need `P+Q`, and both
    // conventions produce the same `P+Q` (the trivial factors are
    // `(1+z^-1)` and `(1-z^-1)`, whose *sum* of contributions is
    // assignment-independent by symmetry of the construction), either
    // assignment reconstructs the same `A(z)`. We use even indices for P.
    let half = p / 2;
    let mut p_poly = alloc_stack_buf::<F>(p + 2);
    let mut q_poly = alloc_stack_buf::<F>(p + 2);
    build_from_roots(&even_indexed(lsp, half), true, &mut p_poly);
    build_from_roots(&odd_indexed(lsp, half), false, &mut q_poly);

    for (k, slot) in out.iter_mut().enumerate() {
        let a_ext = (p_poly[k + 1] + q_poly[k + 1]) / F::from(2).unwrap();
        *slot = -a_ext;
    }
}

/// Extract the even-indexed (0, 2, 4, ...) entries of `lsp` (there must be
/// exactly `half` of them).
fn even_indexed<F: Float>(lsp: &[F], half: usize) -> alloc::vec::Vec<F> {
    (0..half).map(|i| lsp[2 * i]).collect()
}

/// Extract the odd-indexed (1, 3, 5, ...) entries of `lsp`.
fn odd_indexed<F: Float>(lsp: &[F], half: usize) -> alloc::vec::Vec<F> {
    (0..half).map(|i| lsp[2 * i + 1]).collect()
}

/// Build the order-`2*roots.len()+1` polynomial (coefficients of
/// `z^0..z^-(2*roots.len()+1)`, written to `out[0..=2*roots.len()+1]`)
/// that has a root at each `e^{±j·roots[i]}` plus the trivial root
/// (`z=-1` if `carries_plus_one_root`, else `z=1`): the product of
/// `(1 ∓ z^-1)` and, for each root frequency `ω_i`,
/// `(1 - 2cos(ω_i)z^-1 + z^-2)`.
fn build_from_roots<F: Float>(roots: &[F], carries_plus_one_root: bool, out: &mut [F]) {
    // Start with the trivial linear factor, then convolve in each
    // quadratic factor one at a time. `out` is used as the accumulator;
    // `out[0..=deg]` holds the current (growing) polynomial.
    out[0] = F::one();
    out[1] = if carries_plus_one_root { F::one() } else { -F::one() };
    for slot in out.iter_mut().skip(2) {
        *slot = F::zero();
    }
    let mut deg = 1; // current polynomial degree

    for &omega in roots {
        let b1 = -F::from(2).unwrap() * omega.cos(); // (1 + b1 z^-1 + z^-2)
                                                      // Convolve: new[k] = old[k] + b1*old[k-1] + old[k-2]
        for k in (0..=(deg + 2)).rev() {
            let old_k = if k <= deg { out[k] } else { F::zero() };
            let old_k1 = if k >= 1 && k - 1 <= deg { out[k - 1] } else { F::zero() };
            let old_k2 = if k >= 2 && k - 2 <= deg { out[k - 2] } else { F::zero() };
            out[k] = old_k + b1 * old_k1 + old_k2;
        }
        deg += 2;
    }
}

/// Check that `lsp` is a valid, stable set of LSP frequencies: an even,
/// non-empty length, all values strictly inside `(0, π)`, and strictly
/// increasing. This is the exact condition under which
/// [`lsp_to_lpc`] reconstructs a stable (minimum-phase) predictor.
pub fn lsp_is_stable<F: Float>(lsp: &[F]) -> bool {
    if lsp.is_empty() || lsp.len() % 2 != 0 {
        return false;
    }
    let pi = F::from(core::f64::consts::PI).unwrap();
    if lsp[0] <= F::zero() || lsp[lsp.len() - 1] >= pi {
        return false;
    }
    lsp.windows(2).all(|w| w[0] < w[1])
}

/// Linearly interpolate between two same-length LSP sets:
/// `out[i] = a[i]*(1-t) + b[i]*t`. Unlike interpolating raw LPC
/// coefficients directly, interpolating LSP frequencies preserves the
/// strictly-increasing ordering (for `t` in `[0,1]`, a convex combination
/// of two strictly-increasing sequences is itself strictly increasing) —
/// this is the practical reason LSP exists.
///
/// # Panics
///
/// Panics if `a.len() != b.len()` or `out.len() != a.len()`.
pub fn lsp_interpolate<F: Float>(a: &[F], b: &[F], t: F, out: &mut [F]) {
    assert_eq!(a.len(), b.len(), "a and b must have the same length");
    assert_eq!(out.len(), a.len(), "out length must match a/b length");
    let one_minus_t = F::one() - t;
    for ((oa, ob), slot) in a.iter().zip(b.iter()).zip(out.iter_mut()) {
        *slot = *oa * one_minus_t + *ob * t;
    }
}

/// Uniformly quantize each LSP frequency to one of `levels` steps spanning
/// `(0, π)`, then enforce strict ordering by nudging any quantized value
/// that collided with (or fell behind) its predecessor up by one step —
/// naive independent per-coefficient quantization can otherwise collapse
/// two close LSPs to the same level and silently produce an *unstable*
/// reconstructed filter (see [`lsp_is_stable`]).
///
/// # Panics
///
/// Panics if `lsp.len() != out.len()` or `levels < 2`.
pub fn lsp_quantize_uniform<F: Float>(lsp: &[F], levels: u32, out: &mut [F]) {
    assert_eq!(lsp.len(), out.len(), "out length must match lsp length");
    assert!(levels >= 2, "levels must be at least 2");
    let pi = F::from(core::f64::consts::PI).unwrap();
    let step = pi / F::from(levels).unwrap();
    let mut prev: Option<F> = None;
    for (x, slot) in lsp.iter().zip(out.iter_mut()) {
        let level = (*x / step).round();
        let mut q = level * step;
        if let Some(p) = prev {
            if q <= p {
                q = p + step;
            }
        }
        *slot = q;
        prev = Some(q);
    }
}

// A tiny local helper: this whole module requires the `alloc` feature
// (see its `mod` declaration in `lib.rs`) because the interlacing/
// root-rebuilding helpers below need small owned scratch buffers whose
// size depends on `p` at runtime. LSP conversion is an analysis-time,
// once-per-frame operation (not a per-sample real-time hot path), so
// allocating here is the right tradeoff rather than forcing every caller
// to size and pass in scratch buffers for an operation this infrequent.
fn alloc_stack_buf<F: Float>(len: usize) -> alloc::vec::Vec<F> {
    alloc::vec![F::zero(); len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lpc::{autocorrelation, levinson_durbin};

    /// Derive a stable (minimum-phase) LPC filter of the given order by
    /// running Levinson-Durbin on a real synthetic signal — guarantees a
    /// realistic, genuinely stable predictor to test LSP conversion
    /// against, rather than hand-picking coefficients that might not
    /// actually be stable.
    fn stable_lpc_coeffs(order: usize) -> Vec<f64> {
        let signal: Vec<f64> = (0..512)
            .map(|i| (i as f64 * 0.31).sin() + 0.5 * (i as f64 * 0.077).cos() + 0.2 * (i as f64 * 0.013).sin())
            .collect();
        let mut r = vec![0.0f64; order + 1];
        autocorrelation(&signal, order, &mut r);
        let mut coeffs = vec![0.0f64; order];
        let mut scratch = vec![0.0f64; order];
        let err = levinson_durbin(&r, &mut coeffs, &mut scratch);
        assert!(err > 0.0, "test fixture must be a genuinely stable predictor");
        coeffs
    }

    #[test]
    fn lpc_to_lsp_round_trips_through_lsp_to_lpc() {
        for &order in &[2usize, 4, 10, 20, 32] {
            let coeffs = stable_lpc_coeffs(order);
            let mut lsp = vec![0.0f64; order];
            let ok = lpc_to_lsp(&coeffs, &mut lsp, DEFAULT_GRID_POINTS);
            assert!(ok, "order={order}: lpc_to_lsp must find all {order} roots");
            assert!(lsp_is_stable(&lsp), "order={order}: recovered LSP must be valid: {lsp:?}");

            let mut back = vec![0.0f64; order];
            lsp_to_lpc(&lsp, &mut back);
            for (i, (a, b)) in coeffs.iter().zip(back.iter()).enumerate() {
                assert!((a - b).abs() < 1e-4, "order={order} coeff {i}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn lsf_hz_round_trips_and_matches_hand_computation() {
        let sample_rate = 16_000.0f64;
        // Nyquist (pi radians) must map to sample_rate/2.
        assert!((lsp_rad_to_lsf_hz(core::f64::consts::PI, sample_rate) - 8000.0).abs() < 1e-9);
        for &rad in &[0.3f64, 1.0, 2.5] {
            let hz = lsp_rad_to_lsf_hz(rad, sample_rate);
            let back = lsf_hz_to_lsp_rad(hz, sample_rate);
            assert!((back - rad).abs() < 1e-9, "rad={rad}: round trip gave {back}");
        }
    }

    #[test]
    fn lsp_is_stable_rejects_bad_input() {
        assert!(!lsp_is_stable::<f64>(&[])); // empty
        assert!(!lsp_is_stable(&[1.0f64])); // odd length
        assert!(!lsp_is_stable(&[0.5f64, 0.3])); // not increasing
        assert!(!lsp_is_stable(&[-0.1f64, 0.5])); // out of (0,pi)
        assert!(!lsp_is_stable(&[0.5f64, core::f64::consts::PI])); // touches pi
        assert!(lsp_is_stable(&[0.5f64, 1.5, 2.0, 2.8]));
    }

    #[test]
    fn interpolation_preserves_stability_and_hits_endpoints() {
        let a = [0.3f64, 1.0, 1.8, 2.5];
        let b = [0.6f64, 1.2, 2.0, 2.9];
        let mut out = [0.0f64; 4];
        for &t in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            lsp_interpolate(&a, &b, t, &mut out);
            assert!(lsp_is_stable(&out), "t={t}: {out:?}");
        }
        lsp_interpolate(&a, &b, 0.0, &mut out);
        assert_eq!(out, a);
        lsp_interpolate(&a, &b, 1.0, &mut out);
        assert_eq!(out, b);
    }

    #[test]
    fn quantization_preserves_ordering_even_under_collision() {
        // Two LSPs deliberately close enough that coarse quantization
        // would collide them without the ordering safeguard.
        let lsp = [0.500f64, 0.501, 1.5, 2.9];
        let mut out = [0.0f64; 4];
        lsp_quantize_uniform(&lsp, 16, &mut out); // coarse: pi/16 step >> 0.001 gap
        assert!(lsp_is_stable(&out), "{out:?}");
    }
}
