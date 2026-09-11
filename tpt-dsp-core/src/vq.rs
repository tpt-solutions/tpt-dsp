//! Vector Quantisation (VQ): nearest-vector search, encode, and decode
//! against a caller-supplied fixed-size codebook.
//!
//! Required by `tpt-media`'s Vox and Whisper codecs. The codec-specific
//! codebook *contents* belong in the codec (todo.md §8) — this module only
//! provides the generic search/encode/decode machinery, generic over
//! `f32`/`f64` via [`num_traits::Float`].
//!
//! # Convention
//!
//! A codebook is a flat array: `entries` vectors of `dim` elements each,
//! stored contiguously (`codebook.len() == entries * dim`, entry `i`
//! occupying `codebook[i*dim .. (i+1)*dim]`). All free functions here are
//! allocation-free and operate on caller-owned buffers; [`VqCodebook`] (an
//! owning convenience wrapper) requires the `alloc` feature.
//!
//! # Numerical contract (§12)
//!
//! - **Input domain**: any finite query/codebook values, any `entries ≥ 1`
//!   / `dim ≥ 1` (functions panic on a malformed flat codebook length).
//! - **Output domain**: `nearest_vector`/`nearest_vector_accelerated` return
//!   an index in `0..entries`, always defined for a non-empty codebook,
//!   with deterministic lowest-index tie-breaking; `vq_encode`/`vq_decode`
//!   are exact lookups/writes of caller data, not approximations.
//! - **Precision / acceptable error**: unlike the transforms above, VQ
//!   search is *exact* arithmetic (a finite sum of squared/absolute
//!   differences, compared with ordinary `<`) — there is no approximation
//!   tolerance to state. The one property that needs proving, not just
//!   asserting, is that the accelerated early-exit search never picks a
//!   different codeword than the linear scan: verified bit-identical
//!   across 2 metrics × 20 random queries against a 40-entry codebook
//!   (`accelerated_matches_linear_search`).

use num_traits::Float;

/// Distance metric used by nearest-vector search.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DistanceMetric {
    /// Squared Euclidean distance: `Σ (a[i]-b[i])²`. Cheaper than true
    /// Euclidean distance (no square root) and preserves the same
    /// nearest-neighbour ordering, so this is the usual default.
    SquaredEuclidean,
    /// Manhattan / L1 distance: `Σ |a[i]-b[i]|`. Cheaper per term (no
    /// multiply) and less sensitive to any single outlier dimension than
    /// squared Euclidean — a reasonable alternative for some codebook
    /// designs.
    Manhattan,
}

impl DistanceMetric {
    #[inline]
    fn distance<F: Float>(self, a: &[F], b: &[F]) -> F {
        let mut acc = F::zero();
        for (x, y) in a.iter().zip(b.iter()) {
            acc = acc
                + match self {
                    DistanceMetric::SquaredEuclidean => {
                        let d = *x - *y;
                        d * d
                    }
                    DistanceMetric::Manhattan => (*x - *y).abs(),
                };
        }
        acc
    }

    #[inline]
    fn term<F: Float>(self, x: F, y: F) -> F {
        match self {
            DistanceMetric::SquaredEuclidean => {
                let d = x - y;
                d * d
            }
            DistanceMetric::Manhattan => (x - y).abs(),
        }
    }
}

/// Number of codewords in a flat codebook.
///
/// # Panics
///
/// Panics if `dim == 0` or `codebook_len` is not a multiple of `dim`.
fn entry_count(codebook_len: usize, dim: usize) -> usize {
    assert!(dim > 0, "vector dimension must be positive");
    assert_eq!(
        codebook_len % dim,
        0,
        "codebook length must be a multiple of `dim`"
    );
    codebook_len / dim
}

/// Find the nearest codeword to `query` in `codebook` (a flat array of
/// `codebook.len()/dim` vectors of `dim` elements each), under `metric`.
///
/// Returns `(index, distance)` of the nearest codeword. Ties are broken
/// deterministically toward the lowest index (a strict `<` comparison,
/// scanned in index order), so repeated calls on the same input always
/// return the same result.
///
/// # Panics
///
/// Panics if `dim == 0`, `codebook.len()` is not a multiple of `dim`,
/// `codebook` has zero entries, or `query.len() != dim`.
pub fn nearest_vector<F: Float>(
    codebook: &[F],
    dim: usize,
    query: &[F],
    metric: DistanceMetric,
) -> (usize, F) {
    let entries = entry_count(codebook.len(), dim);
    assert!(entries > 0, "codebook must have at least one entry");
    assert_eq!(query.len(), dim, "query length must equal `dim`");

    let mut best_idx = 0;
    let mut best_dist = metric.distance(&codebook[0..dim], query);
    for i in 1..entries {
        let d = metric.distance(&codebook[i * dim..(i + 1) * dim], query);
        if d < best_dist {
            best_dist = d;
            best_idx = i;
        }
    }
    (best_idx, best_dist)
}

/// Same contract and results as [`nearest_vector`], but abandons a
/// codeword's partial distance sum as soon as it can no longer beat the
/// current best — both metrics accumulate non-negative per-dimension
/// terms, so a partial sum that has already reached the current best can
/// never end up smaller than it. Produces bit-identical output to
/// [`nearest_vector`] (see `accelerated_matches_linear_search`); how much
/// faster it is depends on how quickly a good match is found (best case:
/// an early codeword is already close; worst case: no better than the
/// plain linear scan, e.g. if the true nearest entry is last).
///
/// # Panics
///
/// Same as [`nearest_vector`].
pub fn nearest_vector_accelerated<F: Float>(
    codebook: &[F],
    dim: usize,
    query: &[F],
    metric: DistanceMetric,
) -> (usize, F) {
    let entries = entry_count(codebook.len(), dim);
    assert!(entries > 0, "codebook must have at least one entry");
    assert_eq!(query.len(), dim, "query length must equal `dim`");

    let mut best_idx = 0;
    let mut best_dist = F::infinity();
    for i in 0..entries {
        let entry = &codebook[i * dim..(i + 1) * dim];
        let mut acc = F::zero();
        let mut exceeded = false;
        for (&x, &y) in entry.iter().zip(query.iter()) {
            acc = acc + metric.term(x, y);
            if acc >= best_dist {
                exceeded = true;
                break;
            }
        }
        if !exceeded {
            // Not exceeded means every partial sum (all non-decreasing)
            // stayed below `best_dist`, so `acc < best_dist` strictly —
            // the same tie-break direction as `nearest_vector`.
            best_dist = acc;
            best_idx = i;
        }
    }
    (best_idx, best_dist)
}

/// Encode `input` (a flat sequence of vectors of `dim` elements each) as
/// nearest-codeword indices. `out.len()` must equal `input.len() / dim`.
///
/// # Panics
///
/// Panics if `dim == 0`, `input.len()` is not a multiple of `dim`,
/// `out.len() != input.len()/dim`, or `codebook` is empty/malformed.
pub fn vq_encode<F: Float>(
    codebook: &[F],
    dim: usize,
    input: &[F],
    metric: DistanceMetric,
    out: &mut [u32],
) {
    assert!(dim > 0, "vector dimension must be positive");
    assert_eq!(
        input.len() % dim,
        0,
        "input length must be a multiple of `dim`"
    );
    let n = input.len() / dim;
    assert_eq!(out.len(), n, "out length must equal input.len()/dim");
    for (i, slot) in out.iter_mut().enumerate() {
        let query = &input[i * dim..(i + 1) * dim];
        let (idx, _) = nearest_vector(codebook, dim, query, metric);
        *slot = idx as u32;
    }
}

/// Decode codebook `indices` back into reconstructed vectors.
/// `out.len()` must equal `indices.len() * dim`.
///
/// # Panics
///
/// Panics if `dim == 0`, `codebook.len()` is not a multiple of `dim`,
/// `out.len() != indices.len()*dim`, or any entry of `indices` is out of
/// range for `codebook`.
pub fn vq_decode<F: Float>(codebook: &[F], dim: usize, indices: &[u32], out: &mut [F]) {
    let entries = entry_count(codebook.len(), dim);
    assert_eq!(
        out.len(),
        indices.len() * dim,
        "out length must equal indices.len()*dim"
    );
    for (i, &idx) in indices.iter().enumerate() {
        let idx = idx as usize;
        assert!(idx < entries, "codebook index out of range");
        out[i * dim..(i + 1) * dim].copy_from_slice(&codebook[idx * dim..(idx + 1) * dim]);
    }
}

/// Owning convenience wrapper around a fixed-size codebook (requires the
/// `alloc` feature). Stores the flat codebook once and forwards to the
/// free functions above.
#[cfg(feature = "alloc")]
pub struct VqCodebook<F> {
    dim: usize,
    entries: alloc::vec::Vec<F>,
}

#[cfg(feature = "alloc")]
impl<F: Float> VqCodebook<F> {
    /// Build a codebook from flat entry storage (`entries.len()` must be a
    /// multiple of `dim`).
    ///
    /// # Panics
    ///
    /// Panics if `dim == 0` or `entries.len()` is not a multiple of `dim`.
    pub fn new(dim: usize, entries: alloc::vec::Vec<F>) -> Self {
        entry_count(entries.len(), dim); // validates dim/length, result unused
        VqCodebook { dim, entries }
    }

    /// Vector dimension.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Number of codewords.
    pub fn len(&self) -> usize {
        self.entries.len() / self.dim
    }

    /// `true` if the codebook has zero entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// See [`nearest_vector`].
    pub fn nearest(&self, query: &[F], metric: DistanceMetric) -> (usize, F) {
        nearest_vector(&self.entries, self.dim, query, metric)
    }

    /// See [`vq_encode`].
    pub fn encode(&self, input: &[F], metric: DistanceMetric, out: &mut [u32]) {
        vq_encode(&self.entries, self.dim, input, metric, out)
    }

    /// See [`vq_decode`].
    pub fn decode(&self, indices: &[u32], out: &mut [F]) {
        vq_decode(&self.entries, self.dim, indices, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4-entry, 2-dim codebook at the corners of a square, useful for
    /// hand-verifiable nearest-neighbour tests.
    const SQUARE_CODEBOOK: [f64; 8] = [
        0.0, 0.0, // entry 0
        10.0, 0.0, // entry 1
        0.0, 10.0, // entry 2
        10.0, 10.0, // entry 3
    ];

    #[test]
    fn nearest_vector_finds_closest_corner() {
        let (idx, _) = nearest_vector(
            &SQUARE_CODEBOOK,
            2,
            &[9.0, 1.0],
            DistanceMetric::SquaredEuclidean,
        );
        assert_eq!(idx, 1); // (10,0) is closest to (9,1)
    }

    #[test]
    fn ties_break_toward_lowest_index_and_are_deterministic() {
        // (5,5) is exactly equidistant from all four corners under either metric.
        let query = [5.0, 5.0];
        let (idx1, d1) = nearest_vector(
            &SQUARE_CODEBOOK,
            2,
            &query,
            DistanceMetric::SquaredEuclidean,
        );
        let (idx2, d2) = nearest_vector(
            &SQUARE_CODEBOOK,
            2,
            &query,
            DistanceMetric::SquaredEuclidean,
        );
        assert_eq!(idx1, 0, "must break the tie toward the lowest index");
        assert_eq!(
            (idx1, d1),
            (idx2, d2),
            "repeated calls on the same input must agree"
        );
    }

    #[test]
    fn metrics_can_disagree_on_ranking() {
        // (3,0) vs (0,4): squared-Euclidean prefers (3,0) (9 < 16), but
        // Manhattan is indifferent between axis-aligned points at equal L1
        // distance from the origin (3 == 3)... use a case where the two
        // metrics actually rank differently instead: (4,4) vs (0,7).
        // squared-Euclidean: 4^2+4^2=32 vs 0^2+7^2=49 -> prefers (4,4).
        // Manhattan: |4|+|4|=8 vs |0|+|7|=7 -> prefers (0,7).
        let codebook = [4.0f64, 4.0, 0.0, 7.0];
        let query = [0.0f64, 0.0];
        let (idx_sq, _) = nearest_vector(&codebook, 2, &query, DistanceMetric::SquaredEuclidean);
        let (idx_l1, _) = nearest_vector(&codebook, 2, &query, DistanceMetric::Manhattan);
        assert_eq!(idx_sq, 0, "squared-Euclidean must prefer (4,4)");
        assert_eq!(idx_l1, 1, "Manhattan must prefer (0,7)");
    }

    #[test]
    fn accelerated_matches_linear_search() {
        // A deterministic pseudo-random codebook/query set (SplitMix64,
        // same generator used in pitch.rs's tests) across both metrics,
        // checked for bit-identical agreement with the plain linear scan.
        let mut state: u64 = 0xC0FFEE_u64;
        let mut next = move || {
            state = state.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^= z >> 31;
            (z as f64 / u64::MAX as f64) - 0.5
        };
        let dim = 6;
        let entries = 40;
        let codebook: Vec<f64> = (0..entries * dim).map(|_| next()).collect();
        for metric in [DistanceMetric::SquaredEuclidean, DistanceMetric::Manhattan] {
            for _ in 0..20 {
                let query: Vec<f64> = (0..dim).map(|_| next()).collect();
                let linear = nearest_vector(&codebook, dim, &query, metric);
                let accel = nearest_vector_accelerated(&codebook, dim, &query, metric);
                assert_eq!(linear, accel, "metric={metric:?}");
            }
        }
    }

    #[test]
    fn encode_then_decode_reconstructs_codewords_exactly() {
        let dim = 2;
        // Two input vectors, each closest to a distinct corner.
        let input = [9.0f64, 1.0, 1.0, 9.0];
        let mut indices = [0u32; 2];
        vq_encode(
            &SQUARE_CODEBOOK,
            dim,
            &input,
            DistanceMetric::SquaredEuclidean,
            &mut indices,
        );
        assert_eq!(indices, [1, 2]);

        let mut reconstructed = [0.0f64; 4];
        vq_decode(&SQUARE_CODEBOOK, dim, &indices, &mut reconstructed);
        assert_eq!(reconstructed, [10.0, 0.0, 0.0, 10.0]);
    }

    #[test]
    fn re_encoding_decoded_output_is_idempotent() {
        // VQ is inherently lossy, but encoding a *decoded* codeword must
        // always map back to the same index (the codeword is, by
        // definition, its own nearest neighbour).
        let dim = 2;
        let input = [9.0f64, 1.0, 1.0, 9.0, 4.0, 4.0];
        let n = input.len() / dim;
        let mut indices = vec![0u32; n];
        vq_encode(
            &SQUARE_CODEBOOK,
            dim,
            &input,
            DistanceMetric::SquaredEuclidean,
            &mut indices,
        );

        let mut decoded = vec![0.0f64; input.len()];
        vq_decode(&SQUARE_CODEBOOK, dim, &indices, &mut decoded);

        let mut re_encoded = vec![0u32; n];
        vq_encode(
            &SQUARE_CODEBOOK,
            dim,
            &decoded,
            DistanceMetric::SquaredEuclidean,
            &mut re_encoded,
        );
        assert_eq!(indices, re_encoded);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn vq_codebook_wrapper_matches_free_functions() {
        let cb = VqCodebook::new(2, SQUARE_CODEBOOK.to_vec());
        assert_eq!(cb.dim(), 2);
        assert_eq!(cb.len(), 4);
        assert!(!cb.is_empty());

        let free = nearest_vector(
            &SQUARE_CODEBOOK,
            2,
            &[9.0, 1.0],
            DistanceMetric::SquaredEuclidean,
        );
        let wrapped = cb.nearest(&[9.0, 1.0], DistanceMetric::SquaredEuclidean);
        assert_eq!(free, wrapped);
    }
}
