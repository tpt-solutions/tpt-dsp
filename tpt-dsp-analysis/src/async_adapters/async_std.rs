//! async-std channel adapters (feature `async-std`).

use async_std::channel::{Receiver, Sender};
use futures::{Stream, StreamExt};

/// Apply `f` to every frame received on `rx`, forwarding the result to `tx`
/// until `rx` closes or `tx`'s receiver is dropped.
///
/// Each frame and its output share a length; the per-frame output buffer is
/// allocated once per frame (the DSP closure itself stays allocation-free).
pub async fn process_channel<F>(rx: Receiver<Vec<f32>>, tx: Sender<Vec<f32>>, mut f: F)
where
    F: FnMut(&[f32], &mut [f32]),
{
    while let Ok(frame) = rx.recv().await {
        let mut out = vec![0.0f32; frame.len()];
        f(&frame, &mut out);
        if tx.send(out).await.is_err() {
            break;
        }
    }
}

/// Allocation-free counterpart to [`process_channel`]: `f` rewrites each
/// frame in place and that same buffer is forwarded to `tx`.
pub async fn process_channel_in_place<F>(rx: Receiver<Vec<f32>>, tx: Sender<Vec<f32>>, mut f: F)
where
    F: FnMut(&mut [f32]),
{
    while let Ok(mut frame) = rx.recv().await {
        f(&mut frame);
        if tx.send(frame).await.is_err() {
            break;
        }
    }
}

/// Apply `f` to every frame of an async `Stream`, forwarding results to `tx`.
///
/// This is the futures-`Stream` counterpart to [`process_channel`], useful
/// when the source is a network/file reader exposing a `Stream`.
pub async fn process_stream<S, F>(mut stream: S, tx: Sender<Vec<f32>>, mut f: F)
where
    S: Stream<Item = Vec<f32>> + Unpin,
    F: FnMut(&[f32], &mut [f32]),
{
    while let Some(frame) = stream.next().await {
        let mut out = vec![0.0f32; frame.len()];
        f(&frame, &mut out);
        if tx.send(out).await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SpectrumAnalyzer;
    use async_std::channel;
    use async_std::task;
    use futures::stream;

    #[test]
    fn channel_gain_pipeline() {
        task::block_on(async {
            let (in_tx, in_rx) = channel::bounded(4);
            let (out_tx, out_rx) = channel::bounded(4);
            let handle = task::spawn(process_channel(
                in_rx,
                out_tx,
                |in_: &[f32], out: &mut [f32]| {
                    for (o, x) in out.iter_mut().zip(in_.iter()) {
                        *o = x * 2.0;
                    }
                },
            ));
            in_tx.send(vec![1.0, 2.0, 3.0]).await.unwrap();
            in_tx.send(vec![4.0, 5.0]).await.unwrap();
            drop(in_tx);
            let first = out_rx.recv().await.unwrap();
            let second = out_rx.recv().await.unwrap();
            assert_eq!(first, vec![2.0, 4.0, 6.0]);
            assert_eq!(second, vec![8.0, 10.0]);
            handle.await;
        });
    }

    #[test]
    fn channel_in_place_pipeline() {
        task::block_on(async {
            let (in_tx, in_rx) = channel::bounded(4);
            let (out_tx, out_rx) = channel::bounded(4);
            let handle = task::spawn(process_channel_in_place(
                in_rx,
                out_tx,
                |frame: &mut [f32]| {
                    for x in frame.iter_mut() {
                        *x += 1.0;
                    }
                },
            ));
            in_tx.send(vec![0.0, 1.0]).await.unwrap();
            drop(in_tx);
            assert_eq!(out_rx.recv().await.unwrap(), vec![1.0, 2.0]);
            handle.await;
        });
    }

    #[test]
    fn stream_round_trip_through_analyzer() {
        task::block_on(async {
            let src = stream::iter(vec![vec![1.0f32; 4], vec![0.0f32; 4]]);
            let (out_tx, out_rx) = channel::bounded(4);
            let mut analyzer = SpectrumAnalyzer::new(4, 0.5);
            process_stream(src, out_tx, move |in_: &[f32], out: &mut [f32]| {
                analyzer.push(in_);
                out.copy_from_slice(analyzer.spectrum());
            })
            .await;
            let first = out_rx.recv().await.unwrap();
            let second = out_rx.recv().await.unwrap();
            assert_eq!(first, vec![0.5; 4]);
            assert_eq!(second, vec![0.25; 4]);
            assert!(out_rx.recv().await.is_err());
        });
    }
}
