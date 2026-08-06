//! Async streaming adapters (tokio / futures).
//!
//! These bridge a DSP stage (`FnMut(&[f32], &mut [f32])`) into an async
//! pipeline: frames arrive on a stream or channel and the transformed frames
//! are forwarded on. They are the hook for high-throughput network / file
//! streaming without blocking the DSP math behind a runtime.

use futures::Stream;
use tokio::sync::mpsc::Sender;

/// Apply `f` to every frame received on `rx`, forwarding the result to `tx`
/// until `rx` closes or `tx`'s receiver is dropped.
///
/// Each frame and its output share a length; the per-frame output buffer is
/// allocated once per frame (the DSP closure itself stays allocation-free).
pub async fn process_channel<F>(mut rx: tokio::sync::mpsc::Receiver<Vec<f32>>, tx: Sender<Vec<f32>>, mut f: F)
where
    F: FnMut(&[f32], &mut [f32]),
{
    while let Some(frame) = rx.recv().await {
        let mut out = vec![0.0f32; frame.len()];
        f(&frame, &mut out);
        if tx.send(out).await.is_err() {
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
    use futures::StreamExt;
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
    use futures::stream;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
    }

    #[test]
    fn channel_gain_pipeline() {
        let rt = runtime();
        rt.block_on(async {
            let (in_tx, in_rx) = tokio::sync::mpsc::channel(4);
            let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(4);
            let handle = tokio::spawn(process_channel(
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
            handle.await.unwrap();
        });
    }

    #[test]
    fn stream_pipeline_terminates() {
        let rt = runtime();
        rt.block_on(async {
            let src = stream::iter(vec![vec![0.5f32, 0.5], vec![1.0, 1.0]]);
            let (out_tx, mut out_rx) = tokio::sync::mpsc::channel(4);
            process_stream(
                src,
                out_tx,
                |in_: &[f32], out: &mut [f32]| out.copy_from_slice(in_),
            )
            .await;
            let a = out_rx.recv().await.unwrap();
            let b = out_rx.recv().await.unwrap();
            assert_eq!(a, vec![0.5, 0.5]);
            assert_eq!(b, vec![1.0, 1.0]);
        });
    }
}
