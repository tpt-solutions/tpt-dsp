//! Property tests for line spectral pairs: LPC → LSP → LPC recovers the
//! predictor, and the LSPs of a stable predictor are sorted and stable.
//!
//! The predictors come from Levinson–Durbin on random signals' autocorrelation,
//! with a small white-noise term added to `r[0]`. That makes every predictor
//! minimum-phase, so the LSP conversion must succeed.

use proptest::prelude::*;
use tpt_dsp_core::{autocorrelation, levinson_durbin, lpc_to_lsp, lsp_is_stable, lsp_to_lpc};

fn predictor(signal: &[f64], order: usize) -> Vec<f64> {
    let mut r = vec![0.0f64; order + 1];
    autocorrelation(signal, order, &mut r);
    r[0] *= 1.001; // white-noise correction: keeps the predictor minimum-phase
    let mut coeffs = vec![0.0f64; order];
    let mut scratch = vec![0.0f64; order];
    levinson_durbin(&r, &mut coeffs, &mut scratch);
    coeffs
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// `lsp_to_lpc(lpc_to_lsp(a)) = a` for any stable predictor.
    #[test]
    fn lpc_lsp_round_trip_recovers_the_predictor(
        order in prop::sample::select(vec![2usize, 4, 6, 8, 10]),
        signal in prop::collection::vec(-1.0f64..1.0, 128..512),
    ) {
        let a = predictor(&signal, order);
        let mut lsp = vec![0.0f64; order];
        if !lpc_to_lsp(&a, &mut lsp, 512) {
            // A stable predictor always converts; a failure is a real problem.
            prop_assert!(false, "lpc_to_lsp failed for a stable predictor");
        }
        prop_assert!(lsp_is_stable(&lsp), "LSPs not stable: {:?}", lsp);
        let mut back = vec![0.0f64; order];
        lsp_to_lpc(&lsp, &mut back);
        for (x, y) in back.iter().zip(a.iter()) {
            prop_assert!((x - y).abs() < 1e-4, "{x} vs {y}");
        }
    }

    /// LSP frequencies are strictly increasing in (0, π).
    #[test]
    fn lsps_are_sorted_and_inside_the_band(
        order in prop::sample::select(vec![4usize, 8]),
        signal in prop::collection::vec(-1.0f64..1.0, 128..256),
    ) {
        let a = predictor(&signal, order);
        let mut lsp = vec![0.0f64; order];
        prop_assume!(lpc_to_lsp(&a, &mut lsp, 512));
        for w in lsp.windows(2) {
            prop_assert!(w[0] < w[1], "not sorted: {:?}", lsp);
        }
        prop_assert!(lsp[0] > 0.0 && lsp[order - 1] < core::f64::consts::PI);
    }
}
