//! Audio output via [cpal](https://docs.rs/cpal) (pure-Rust cross-platform
//! audio I/O).
//!
//! Provides a thin, real-time-safe wrapper that drives a per-block DSP callback
//! from the host's audio thread. The callback receives a mutable slice of
//! `f32` samples to fill; it must be allocation-free.
//!
//! Enabled by the `audio` feature.

use cpal::traits::*;

/// Run an output audio stream, calling `callback` once per block to fill the
/// output buffer.
///
/// * `sample_rate` — requested sample rate in Hz.
/// * `block_size` — requested buffer size in frames (the host may adjust).
/// * `callback` — fills `data` (length = frames × 1 channel) each block.
/// * `stop` — the run blocks until a message arrives on this channel (or it is
///   dropped), at which point the stream is torn down.
///
/// # Errors
///
/// Returns an error if no output device is available or the stream cannot be
/// built / started.
pub fn run_output<F>(
    sample_rate: u32,
    block_size: u32,
    mut callback: F,
    stop: &std::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output audio device available")?;

    let config = cpal::StreamConfig {
        channels: 1,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Fixed(block_size),
    };

    let err_fn = |e: cpal::StreamError| eprintln!("tpt-dsp-io audio error: {e}");
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _info| {
            callback(data);
        },
        err_fn,
        None,
    )?;
    stream.play()?;

    // Hold the stream alive and wait for a stop signal.
    let _ = stop.recv();
    Ok(())
}

/// Enumerate the names of the available output devices (useful for logging /
/// device selection UIs).
///
/// # Errors
///
/// Returns an error if the host cannot be queried.
pub fn list_output_devices() -> Result<Vec<String>, cpal::DevicesError> {
    let host = cpal::default_host();
    let mut names = Vec::new();
    for device in host.output_devices()? {
        if let Ok(name) = device.name() {
            names.push(name);
        }
    }
    Ok(names)
}
