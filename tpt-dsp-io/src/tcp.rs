//! Raw TCP streaming of IQ data (e.g. rtl_tcp / SDR-over-network).
//!
//! A small async server that accepts one TCP connection, reads raw interleaved
//! I/Q bytes and parses them into [`Complex32`] samples delivered to a callback.
//! Useful for SDR front-ends that stream over the network rather than USB.
//!
//! Enabled by the `tcp` feature (pulls in `tokio`).

use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;

use tpt_dsp_core::Complex32;

use crate::iq::{parse_iq, IqFormat};

/// Serve IQ frames over TCP.
///
/// Accepts a single connection on `listener`, reads raw bytes, parses them into
/// complex samples with `format`, and invokes `on_frame` with each batch.
/// Returns when the connection closes or an I/O error occurs.
///
/// # Errors
///
/// Returns the underlying async I/O error.
pub async fn serve_iq(
    listener: TcpListener,
    format: IqFormat,
    mut on_frame: impl FnMut(&[Complex32]),
) -> std::io::Result<()> {
    let (mut socket, _addr) = listener.accept().await?;
    let bps = format.bytes_per_sample();
    let mut buf = vec![0u8; 8192];
    let mut out = vec![Complex32::default(); 4096];

    loop {
        let n = socket.read(&mut buf).await?;
        if n == 0 {
            break; // clean EOF
        }
        let mut offset = 0;
        while offset + bps <= n {
            let parsed = parse_iq(format, &buf[offset..n], &mut out);
            if parsed == 0 {
                break;
            }
            if parsed > 0 {
                on_frame(&out[..parsed]);
            }
            offset += parsed * bps;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpStream;

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap()
    }

    #[test]
    fn tcp_iq_roundtrip_parses_samples() {
        runtime().block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            // (0,0), (+1,-1), (-1,+1) as U8 I/Q.
            let bytes = [128u8, 128, 255, 0, 0, 255];

            let collected: Arc<Mutex<Vec<Complex32>>> = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&collected);
            let server = tokio::spawn(async move {
                serve_iq(listener, IqFormat::U8, move |samples| {
                    sink.lock().unwrap().extend_from_slice(samples);
                })
                .await
                .ok();
            });

            let mut client = TcpStream::connect(addr).await.unwrap();
            client.write_all(&bytes).await.unwrap();
            client.flush().await.unwrap();
            // Let the server read, then close the connection.
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(client);
            let _ = server.await;

            let got = collected.lock().unwrap();
            assert_eq!(got.len(), 3, "parsed samples: {got:?}");
            // U8 maps 255→+127/128, 0→-1.0.
            assert!((got[1].re - 127.0 / 128.0).abs() < 1e-6 && (got[1].im + 1.0).abs() < 1e-6);
        });
    }
}
