//! Async streaming adapters (tokio / async-std / futures).
//!
//! These bridge a DSP stage (`FnMut(&[f32], &mut [f32])`) into an async
//! pipeline: frames arrive on a stream or channel and the transformed frames
//! are forwarded on. They are the hook for high-throughput network / file
//! streaming without blocking the DSP math behind a runtime.
//!
//! # Runtimes
//!
//! The adapters in this module are runtime-agnostic: they accept any futures
//! [`Stream`] / [`Sink`] pair (e.g. `futures::channel::mpsc`). Runtime
//! specific channel adapters live in the submodules, each behind its own
//! feature:
//!
//! - `tokio` — feature `async-tokio`, enabled by the default `async`
//!   feature.
//! - `async_std` — feature `async-std`.
//!
//! # Allocation
//!
//! The out-of-place adapters allocate one output buffer per frame; the DSP
//! closure itself stays allocation-free. The `*_in_place` adapters reuse the
//! incoming buffer and allocate nothing at all.

use futures::{Sink, SinkExt, Stream, StreamExt};

#[cfg(feature = "async-std")]
pub mod async_std;
#[cfg(feature = "async-tokio")]
pub mod tokio;

/// Apply `f` to every frame of `stream`, forwarding each result to `sink`.
///
/// Runs until the stream ends or the sink refuses a frame (typically because
/// its receiver was dropped).
pub async fn process_stream_into_sink<S, K, F>(mut stream: S, mut sink: K, mut f: F)
where
    S: Stream<Item = Vec<f32>> + Unpin,
    K: Sink<Vec<f32>> + Unpin,
    F: FnMut(&[f32], &mut [f32]),
{
    while let Some(frame) = stream.next().await {
        let mut out = vec![0.0f32; frame.len()];
        f(&frame, &mut out);
        if sink.send(out).await.is_err() {
            break;
        }
    }
}

/// Allocation-free counterpart to [`process_stream_into_sink`]: `f` rewrites
/// each frame in place and that same buffer is forwarded to `sink`.
pub async fn process_stream_in_place<S, K, F>(mut stream: S, mut sink: K, mut f: F)
where
    S: Stream<Item = Vec<f32>> + Unpin,
    K: Sink<Vec<f32>> + Unpin,
    F: FnMut(&mut [f32]),
{
    while let Some(mut frame) = stream.next().await {
        f(&mut frame);
        if sink.send(frame).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::channel::mpsc;
    use futures::executor::block_on;
    use futures::stream;

    #[test]
    fn sink_pipeline_applies_gain() {
        block_on(async {
            let src = stream::iter(vec![vec![1.0f32, 2.0], vec![3.0, 4.0]]);
            let (tx, rx) = mpsc::channel(4);
            process_stream_into_sink(src, tx, |in_: &[f32], out: &mut [f32]| {
                for (o, x) in out.iter_mut().zip(in_.iter()) {
                    *o = x * 2.0;
                }
            })
            .await;
            let got: Vec<Vec<f32>> = rx.collect().await;
            assert_eq!(got, vec![vec![2.0, 4.0], vec![6.0, 8.0]]);
        });
    }

    #[test]
    fn in_place_pipeline_reuses_buffers() {
        block_on(async {
            let src = stream::iter(vec![vec![1.0f32, -2.0], vec![-3.0, 4.0]]);
            let (tx, rx) = mpsc::channel(4);
            process_stream_in_place(src, tx, |frame: &mut [f32]| {
                for x in frame.iter_mut() {
                    *x = x.abs();
                }
            })
            .await;
            let got: Vec<Vec<f32>> = rx.collect().await;
            assert_eq!(got, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
        });
    }
}
