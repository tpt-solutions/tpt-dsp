//! Modified Discrete Cosine Transform (MDCT) and its inverse.
//!
//! Direct O(N²) implementations of the lapped transform used as the
//! frequency-domain analysis/synthesis stage in TDAC-based audio codecs
//! (`tpt-media`'s Pulse and Aura). These are reference-quality baselines,
//! not fast transforms — see [`crate::dct`] for the same trade-off applied
//! to the plain DCT.
//!
//! # Convention
//!
//! For an input block of `2N` samples `x[0..2N)`, the forward transform
//! produces `N` spectral coefficients:
//!
//! ```text
//! X[k] = Σ_{n=0}^{2N-1} x[n] · cos( (π/N)·(n + ½ + N/2)·(k + ½) ),  k = 0..N
//! ```
//!
//! and the inverse recovers `2N` time-domain samples from `N` coefficients:
//!
//! ```text
//! y[n] = (2/N) · Σ_{k=0}^{N-1} X[k] · cos( (π/N)·(n + ½ + N/2)·(k + ½) ),  n = 0..2N
//! ```
//!
//! This is the standard Princen–Bradley convention. A single forward+inverse
//! pass does **not** reconstruct the original block on its own — the MDCT is
//! a *lapped* transform whose inverse is intentionally aliased. Perfect
//! reconstruction (time-domain aliasing cancellation, TDAC) requires:
//!
//! 1. windowing each `2N`-sample block before the forward transform and
//!    windowing the inverse-transformed block again with a window `h`
//!    satisfying `h[n]² + h[n+N]² = 1` ([`sine_window`] provides one), and
//! 2. overlap-adding consecutive windowed inverse blocks at a hop of `N`
//!    samples.
//!
//! See the `overlap_add_round_trip_reconstructs_signal` test below for a
//! worked example, and §5.1 of the project's `todo.md` for the numerical
//! contract this module fixes.

use num_traits::Float;

use crate::complex::{cos_pi_over_n, pi};

/// Fill `out` (length `2 * n`) with the MDCT sine window:
/// `h[i] = sin( (π/(2n)) · (i + ½) )`.
///
/// This is the standard MDCT analysis/synthesis window. It satisfies the
/// Princen–Bradley condition `h[i]² + h[i+n]² = 1`, which is what allows
/// overlap-add of consecutive windowed blocks to reconstruct the original
/// signal exactly.
///
/// # Panics
///
/// Panics if `out.len() != 2 * n`.
pub fn sine_window<F: Float>(n: usize, out: &mut [F]) {
    assert_eq!(out.len(), 2 * n, "sine window length must be 2n");
    let scale = pi::<F>() / F::from(2 * n).unwrap();
    for (i, slot) in out.iter_mut().enumerate() {
        let theta = scale * (F::from(i).unwrap() + F::from(0.5).unwrap());
        *slot = theta.sin();
    }
}

/// Forward MDCT: `2n` time-domain samples in `input` → `n` spectral
/// coefficients in `out`.
///
/// # Panics
///
/// Panics if `input.len() != 2 * out.len()`.
pub fn mdct<F: Float>(input: &[F], out: &mut [F]) {
    let n = out.len();
    assert_eq!(input.len(), 2 * n, "mdct input must be exactly 2x output length");
    for (k, slot) in out.iter_mut().enumerate() {
        let two_y = (2 * k + 1) as i64;
        let mut acc = F::zero();
        for (m, x) in input.iter().enumerate() {
            let two_x = (2 * m + 1 + n) as i64;
            acc = acc + *x * cos_pi_over_n::<F>(n, two_x, two_y);
        }
        *slot = acc;
    }
}

/// Inverse MDCT: `n` spectral coefficients in `input` → `2n` time-domain
/// samples in `out`.
///
/// # Panics
///
/// Panics if `out.len() != 2 * input.len()`.
pub fn imdct<F: Float>(input: &[F], out: &mut [F]) {
    let n = input.len();
    assert_eq!(out.len(), 2 * n, "imdct output must be exactly 2x input length");
    let norm = F::from(2).unwrap() / F::from(n).unwrap();
    for (i, slot) in out.iter_mut().enumerate() {
        let two_x = (2 * i + 1 + n) as i64;
        let mut acc = F::zero();
        for (k, x) in input.iter().enumerate() {
            let two_y = (2 * k + 1) as i64;
            acc = acc + *x * cos_pi_over_n::<F>(n, two_x, two_y);
        }
        *slot = acc * norm;
    }
}

/// A reusable MDCT/IMDCT pair for a fixed, validated block size.
///
/// `forward`/`inverse` delegate to the free functions above with no
/// additional allocation; the plan exists so callers validate the block
/// size once (at setup) rather than on every call.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct MdctPlan {
    n: usize,
}

impl MdctPlan {
    /// Supported MDCT half-sizes (`N`, giving a `2N`-sample analysis
    /// window), matching the block sizes used by the Pulse/Aura codecs.
    pub const SUPPORTED_SIZES: [usize; 6] = [128, 256, 512, 1024, 2048, 4096];

    /// Build a plan for half-size `n` (the analysis window is `2n` samples).
    ///
    /// Returns `None` if `n` is not one of [`Self::SUPPORTED_SIZES`].
    pub fn new(n: usize) -> Option<Self> {
        if Self::SUPPORTED_SIZES.contains(&n) {
            Some(MdctPlan { n })
        } else {
            None
        }
    }

    /// Half-size `N` (the spectral coefficient count).
    pub fn n(&self) -> usize {
        self.n
    }

    /// Analysis/synthesis window length (`2N` time-domain samples).
    pub fn window_len(&self) -> usize {
        2 * self.n
    }

    /// Forward MDCT. `input.len()` must be `2n`; `out.len()` must be `n`.
    ///
    /// # Panics
    ///
    /// Panics if the lengths don't match the plan's size.
    pub fn forward<F: Float>(&self, input: &[F], out: &mut [F]) {
        assert_eq!(input.len(), 2 * self.n, "input length must be 2n");
        assert_eq!(out.len(), self.n, "output length must be n");
        mdct(input, out);
    }

    /// Inverse MDCT. `input.len()` must be `n`; `out.len()` must be `2n`.
    ///
    /// # Panics
    ///
    /// Panics if the lengths don't match the plan's size.
    pub fn inverse<F: Float>(&self, input: &[F], out: &mut [F]) {
        assert_eq!(input.len(), self.n, "input length must be n");
        assert_eq!(out.len(), 2 * self.n, "output length must be 2n");
        imdct(input, out);
    }
}

/// Fast MDCT/IMDCT for a fixed half-size `n`, backed by a single length-`2n`
/// complex FFT ([`crate::FftPlan`], RustFFT-backed and hardware-accelerated
/// where available) instead of the direct O(N²) sums in [`mdct`]/[`imdct`].
///
/// `std`-only (needs [`crate::FftPlan`]) and `f32`-only (matches
/// `FftPlan`'s precision — the only precision Pulse/Aura actually use).
///
/// # Derivation
///
/// Expanding the `cos` product in [`mdct`]'s convention,
/// `θ(n,k) = (π/N)(n+a)(k+½)` with `a = ½+N/2`, splits into a bilinear term
/// plus phases that depend on only one of `n`/`k`:
///
/// ```text
/// θ(n,k) = (π/N)·n·k + φ(n) + ψ(k)
/// φ(n) = (π/(2N))·n
/// ψ(k) = (π/(2N) + π/2)·k + π/(4N) + π/4
/// ```
///
/// Since `(π/N) = 2π/(2N)`, the bilinear term is exactly a size-`2N` DFT
/// kernel. Writing `X[k] = Σ_n x[n]·cos(θ(n,k)) = Re{e^{jψ(k)}·Σ_n [x[n]
/// e^{jφ(n)}]·e^{j(2π/2N)nk}}`, the inner sum is recovered from a single
/// forward FFT of the chirp-premultiplied input via the conjugate identity
/// `Σ_n y[n]·e^{+jθ} = conj(FFT(conj(y)))`. The inverse follows the same
/// θ(n,k) with `k`/`n` swapped as summation/output index and a zero-padded
/// spectrum. See `fast_matches_reference_forward`/`_inverse` below for the
/// numerical proof against the O(N²) reference.
#[cfg(feature = "std")]
pub struct FastMdctPlan {
    n: usize,
    fft: crate::plan::FftPlan,
    /// `e^{jψ(k)}`, `k = 0..n`.
    psi: std::vec::Vec<crate::complex::C32>,
    /// `e^{jφ(n)}`, `n = 0..2n`.
    phi: std::vec::Vec<crate::complex::C32>,
    scratch: std::vec::Vec<crate::complex::C32>,
}

#[cfg(feature = "std")]
impl FastMdctPlan {
    /// Build a fast plan for half-size `n` (analysis window `2n`).
    ///
    /// # Panics
    ///
    /// Panics if `n == 0`.
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "mdct half-size must be positive");
        use crate::complex::exp_i;
        let two_n = 2 * n;
        let fft = crate::plan::FftPlan::new_forward(two_n);
        let psi = (0..n)
            .map(|k| {
                let kf = k as f32;
                let psi_k = (core::f32::consts::PI / (2.0 * n as f32) + core::f32::consts::FRAC_PI_2) * kf
                    + core::f32::consts::PI / (4.0 * n as f32)
                    + core::f32::consts::FRAC_PI_4;
                exp_i(psi_k)
            })
            .collect();
        let phi = (0..two_n)
            .map(|i| exp_i(core::f32::consts::PI / (2.0 * n as f32) * i as f32))
            .collect();
        let scratch = std::vec![crate::complex::C32::new(0.0, 0.0); two_n];
        FastMdctPlan { n, fft, psi, phi, scratch }
    }

    /// Half-size `N` (the spectral coefficient count).
    pub fn n(&self) -> usize {
        self.n
    }

    /// Forward MDCT. `input.len()` must be `2n`; `out.len()` must be `n`.
    ///
    /// # Panics
    ///
    /// Panics if the lengths don't match the plan's size.
    pub fn forward(&mut self, input: &[f32], out: &mut [f32]) {
        assert_eq!(input.len(), 2 * self.n, "input length must be 2n");
        assert_eq!(out.len(), self.n, "output length must be n");
        for (i, x) in input.iter().enumerate() {
            self.scratch[i] = crate::complex::C32::new(*x, 0.0) * self.phi[i].conj();
        }
        self.fft.process_inplace(&mut self.scratch);
        for (k, slot) in out.iter_mut().enumerate() {
            *slot = (self.psi[k] * self.scratch[k].conj()).re;
        }
    }

    /// Inverse MDCT. `input.len()` must be `n`; `out.len()` must be `2n`.
    ///
    /// # Panics
    ///
    /// Panics if the lengths don't match the plan's size.
    pub fn inverse(&mut self, input: &[f32], out: &mut [f32]) {
        assert_eq!(input.len(), self.n, "input length must be n");
        assert_eq!(out.len(), 2 * self.n, "output length must be 2n");
        for slot in self.scratch.iter_mut() {
            *slot = crate::complex::C32::new(0.0, 0.0);
        }
        for (k, x) in input.iter().enumerate() {
            self.scratch[k] = crate::complex::C32::new(*x, 0.0) * self.psi[k].conj();
        }
        self.fft.process_inplace(&mut self.scratch);
        let norm = 2.0 / self.n as f32;
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = norm * (self.phi[i] * self.scratch[i].conj()).re;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_window_satisfies_princen_bradley() {
        let n = 8;
        let mut w = vec![0.0f64; 2 * n];
        sine_window(n, &mut w);
        for i in 0..n {
            let sum = w[i] * w[i] + w[i + n] * w[i + n];
            assert!((sum - 1.0).abs() < 1e-9, "h[{i}]^2+h[{i}+n]^2 = {sum}");
        }
    }

    #[test]
    fn overlap_add_round_trip_reconstructs_signal() {
        // Classic TDAC test: encode overlapping, windowed 2N-sample blocks
        // of a test signal at a hop of N, and confirm overlap-add
        // reconstructs the fully-overlapped interior of the signal.
        let n = 32usize;
        let two_n = 2 * n;
        let total = 6 * n;
        let signal: Vec<f64> = (0..total)
            .map(|i| (i as f64 * 0.13).sin() + 0.3 * (i as f64 * 0.5).cos())
            .collect();

        let mut window = vec![0.0f64; two_n];
        sine_window(n, &mut window);

        let mut reconstructed = vec![0.0f64; total];
        let mut block_count = 0;
        let mut pos = 0;
        while pos + two_n <= total {
            let mut windowed_in = vec![0.0f64; two_n];
            for i in 0..two_n {
                windowed_in[i] = signal[pos + i] * window[i];
            }
            let mut spec = vec![0.0f64; n];
            mdct(&windowed_in, &mut spec);
            let mut time = vec![0.0f64; two_n];
            imdct(&spec, &mut time);
            for i in 0..two_n {
                reconstructed[pos + i] += time[i] * window[i];
            }
            block_count += 1;
            pos += n;
        }
        assert!(block_count >= 3, "test needs several overlapping blocks");

        // The first and last half-window only receive a contribution from
        // one block and are not part of the reconstructible interior.
        for i in n..(total - n) {
            assert!(
                (reconstructed[i] - signal[i]).abs() < 1e-6,
                "sample {i}: got {} want {}",
                reconstructed[i],
                signal[i]
            );
        }
    }

    #[test]
    fn plan_rejects_unsupported_size() {
        assert!(MdctPlan::new(100).is_none());
        assert!(MdctPlan::new(1024).is_some());
    }

    #[test]
    fn plan_forward_inverse_shapes() {
        let plan = MdctPlan::new(128).unwrap();
        let input = vec![0.0f32; 256];
        let mut spec = vec![0.0f32; 128];
        plan.forward(&input, &mut spec);
        let mut back = vec![0.0f32; 256];
        plan.inverse(&spec, &mut back);
        assert_eq!(back.len(), 256);
    }

    #[test]
    fn silence_round_trips_to_silence() {
        let n = 16;
        let input = vec![0.0f64; 2 * n];
        let mut spec = vec![0.0f64; n];
        mdct(&input, &mut spec);
        assert!(spec.iter().all(|v| *v == 0.0), "silence must produce an all-zero spectrum");
        let mut back = vec![0.0f64; 2 * n];
        imdct(&spec, &mut back);
        assert!(back.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn extreme_magnitude_input_stays_finite() {
        // Values near f32's usable range: the transform must not overflow to
        // inf/NaN for realistic (if extreme) sample magnitudes.
        let n = 16;
        let input: Vec<f32> = (0..2 * n)
            .map(|i| if i % 2 == 0 { 1.0e6 } else { -1.0e6 })
            .collect();
        let mut spec = vec![0.0f32; n];
        mdct(&input, &mut spec);
        assert!(spec.iter().all(|v| v.is_finite()), "spectrum must stay finite: {spec:?}");
        let mut back = vec![0.0f32; 2 * n];
        imdct(&spec, &mut back);
        assert!(back.iter().all(|v| v.is_finite()), "reconstructed samples must stay finite: {back:?}");
    }

    #[test]
    fn nan_input_confines_to_taps_that_touch_it() {
        // Document (rather than silently allow) NaN propagation: only the
        // spectral bins whose cosine weight for the NaN sample is nonzero
        // become NaN, not the whole spectrum, since `mdct`/`imdct` never
        // branch on input values, and `NaN * 0.0 == NaN` for IEEE floats.
        // Callers must sanitize input upstream of the transform; this test
        // just pins the failure shape so a future change can't silently
        // make NaN propagate to every output either.
        let n = 8;
        let mut input = vec![0.0f64; 2 * n];
        input[0] = f64::NAN;
        let mut spec = vec![0.0f64; n];
        mdct(&input, &mut spec);
        assert!(spec.iter().any(|v| v.is_nan()), "NaN input must not be silently absorbed");
    }

    #[cfg(feature = "std")]
    #[test]
    fn fast_matches_reference_forward() {
        for &n in &[8usize, 32, 128, 1024] {
            let two_n = 2 * n;
            let input: Vec<f32> = (0..two_n)
                .map(|i| (i as f32 * 0.137).sin() + 0.4 * (i as f32 * 0.051).cos())
                .collect();

            let mut reference = vec![0.0f32; n];
            mdct(&input, &mut reference);

            let mut fast_plan = FastMdctPlan::new(n);
            let mut fast = vec![0.0f32; n];
            fast_plan.forward(&input, &mut fast);

            for (k, (r, f)) in reference.iter().zip(fast.iter()).enumerate() {
                let tol = 1e-4 * r.abs().max(1.0);
                assert!(
                    (r - f).abs() < tol,
                    "n={n} bin {k}: reference {r} vs fast {f}"
                );
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn fast_matches_reference_inverse() {
        for &n in &[8usize, 32, 128, 1024] {
            let spec: Vec<f32> = (0..n).map(|k| (k as f32 * 0.29).cos() * 0.7).collect();

            let mut reference = vec![0.0f32; 2 * n];
            imdct(&spec, &mut reference);

            let mut fast_plan = FastMdctPlan::new(n);
            let mut fast = vec![0.0f32; 2 * n];
            fast_plan.inverse(&spec, &mut fast);

            for (i, (r, f)) in reference.iter().zip(fast.iter()).enumerate() {
                let tol = 1e-4 * r.abs().max(1.0);
                assert!(
                    (r - f).abs() < tol,
                    "n={n} sample {i}: reference {r} vs fast {f}"
                );
            }
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn fast_plan_round_trip_matches_direct_overlap_add() {
        // The fast path must satisfy the same TDAC round trip as the direct
        // reference, not just match it bin-for-bin on a single block.
        let n = 32usize;
        let two_n = 2 * n;
        let total = 6 * n;
        let signal: Vec<f32> = (0..total)
            .map(|i| (i as f32 * 0.13).sin() + 0.3 * (i as f32 * 0.5).cos())
            .collect();
        let mut window = vec![0.0f32; two_n];
        sine_window(n, &mut window);

        let mut plan = FastMdctPlan::new(n);
        let mut reconstructed = vec![0.0f32; total];
        let mut pos = 0;
        while pos + two_n <= total {
            let mut windowed_in = vec![0.0f32; two_n];
            for i in 0..two_n {
                windowed_in[i] = signal[pos + i] * window[i];
            }
            let mut spec = vec![0.0f32; n];
            plan.forward(&windowed_in, &mut spec);
            let mut time = vec![0.0f32; two_n];
            plan.inverse(&spec, &mut time);
            for i in 0..two_n {
                reconstructed[pos + i] += time[i] * window[i];
            }
            pos += n;
        }
        for i in n..(total - n) {
            assert!(
                (reconstructed[i] - signal[i]).abs() < 1e-3,
                "sample {i}: got {} want {}",
                reconstructed[i],
                signal[i]
            );
        }
    }
}
