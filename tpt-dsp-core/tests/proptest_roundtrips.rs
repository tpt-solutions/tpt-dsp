//! Property-based round trips for the transforms and the ring buffer.
//!
//! Each property is checked on many random inputs. A failing case is shrunk
//! to a minimal example by proptest.

use std::collections::VecDeque;

use proptest::prelude::*;
use tpt_dsp_core::{Complex32, FastDct2Plan, FftPlan, RingBuffer};

/// Power-of-two lengths the FFT and fast DCT plans accept.
fn pow2_len() -> impl Strategy<Value = usize> {
    (2u32..=8).prop_map(|k| 1usize << k)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// IFFT(FFT(x)) / N = x for any complex input.
    #[test]
    fn fft_round_trip_recovers_the_input(
        n in pow2_len(),
        seed in prop::collection::vec(-1.0f32..1.0, 512),
    ) {
        let x: Vec<Complex32> = (0..n)
            .map(|i| Complex32::new(seed[i], seed[(i + 256) % seed.len()]))
            .collect();
        let mut buf = x.clone();
        FftPlan::new_forward(n).process_inplace(&mut buf);
        FftPlan::new_inverse(n).process_inplace(&mut buf);
        let scale = 1.0 / n as f32;
        for (got, want) in buf.iter().zip(x.iter()) {
            prop_assert!((got.re * scale - want.re).abs() < 1e-4);
            prop_assert!((got.im * scale - want.im).abs() < 1e-4);
        }
    }

    /// The fast DCT-II pair (forward then inverse) recovers n/2 · x.
    #[test]
    fn fast_dct_round_trip_recovers_the_input(
        n in pow2_len(),
        seed in prop::collection::vec(-1.0f32..1.0, 256),
    ) {
        let x: Vec<f32> = (0..n).map(|i| seed[i % seed.len()]).collect();
        let mut plan = FastDct2Plan::new(n);
        let mut coeffs = vec![0.0f32; n];
        let mut back = vec![0.0f32; n];
        plan.forward(&x, &mut coeffs);
        plan.inverse(&coeffs, &mut back);
        let scale = 2.0 / n as f32;
        for (got, want) in back.iter().zip(x.iter()) {
            prop_assert!((got * scale - want).abs() < 1e-3, "{} vs {}", got * scale, want);
        }
    }

    /// A ring buffer behaves like a bounded FIFO model under any sequence of
    /// pushes and pops.
    #[test]
    fn ring_buffer_matches_a_bounded_fifo_model(
        capacity in 1usize..=16,
        ops in prop::collection::vec(prop::option::of(-100i32..100), 0..200),
    ) {
        // Usable capacity is storage.len() - 1 (one slot marks "full").
        let mut storage = vec![0i32; capacity + 1];
        let mut ring = RingBuffer::new(&mut storage);
        let mut model: VecDeque<i32> = VecDeque::new();
        for op in ops {
            match op {
                Some(v) => {
                    let got = ring.push(v);
                    if model.len() < capacity {
                        prop_assert!(got.is_ok());
                        model.push_back(v);
                    } else {
                        prop_assert_eq!(got, Err(v));
                    }
                }
                None => {
                    prop_assert_eq!(ring.pop(), model.pop_front());
                }
            }
            prop_assert_eq!(ring.len(), model.len());
            prop_assert_eq!(ring.front(), model.front().copied());
        }
    }
}
