//! Built-in, dependency-free real-time audio I/O.
//!
//! This module replaces the former `cpal` dependency (Apache-2.0-only) so the
//! whole tree stays MIT/Apache-2.0 pure even with the `audio` feature
//! enabled. The platform backends are implemented directly in-tree:
//!
//! - **Windows** (`wasapi` submodule): shared-mode WASAPI driven through
//!   hand-written COM vtable declarations. No wrapper crates are used; the
//!   GUIDs, flags and interface layouts are declared here. Requires Windows 10+
//!   (`AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM`). Device friendly names come from
//!   the MMDevice property store.
//! - **Linux** (`alsa` submodule): the kernel ALSA UAPI driven through raw
//!   `ioctl`s on `/dev/snd/pcmC*D*p|c` — no `libasound` linkage. Blocking
//!   interleaved transfers, format negotiation (`FLOAT_LE`/`S32_LE`/`S16_LE`)
//!   and XRUN recovery.
//! - **macOS** (`backend_mac.rs`): CoreAudio AudioUnits (default-output unit
//!   for playback, HALOutput for capture) through hand-declared `extern "C"`
//!   bindings to the system `AudioToolbox`/`CoreAudio`/`CoreFoundation`
//!   frameworks — no `coreaudio-sys`, no wrapper crates.
//! - **Other platforms**: documented stubs that return an error until their
//!   native backends land
//!   (see `todo.md`, "Long-term: clean-room audio I/O").
//!
//! All callbacks run on a dedicated OS thread created by [`run_output`] /
//! [`run_input`]; the callback itself must stay allocation-free per the
//! framework-wide real-time contract.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::fmt;
use std::sync::mpsc::Receiver;

#[cfg(target_os = "windows")]
#[path = "backend_windows.rs"]
mod backend;
#[cfg(target_os = "linux")]
#[path = "backend_linux.rs"]
mod backend;
#[cfg(target_os = "macos")]
#[path = "backend_mac.rs"]
mod backend;
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
#[path = "backend_stub.rs"]
mod backend;

/// Errors reported by the built-in audio backends.
#[derive(Debug, Clone)]
pub struct AudioError(pub(crate) String);

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "audio: {}", self.0)
    }
}

impl std::error::Error for AudioError {}

/// Run a mono `f32` output stream on the default output device, calling
/// `callback` once per block to fill the output buffer. Blocks the calling
/// thread until a message arrives on `stop` (or it is dropped), then tears the
/// stream down.
///
/// * `sample_rate` — requested sample rate in Hz (the host resamples if the
///   device mix rate differs).
/// * `block_size` — requested buffer size in frames (the host may adjust).
///
/// # Errors
/// Returns [`AudioError`] when no output device is available or the stream
/// cannot be built / started.
pub fn run_output<F>(
    sample_rate: u32,
    block_size: u32,
    callback: F,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    backend::run_output(sample_rate, block_size, callback, stop)
}

/// Run an input capture stream from the default input device, invoking
/// `callback(interleaved_samples, channel_count)` once per captured packet.
/// Samples are always delivered as `f32` in `[-1, 1]` regardless of the
/// device's native format. Blocks until a message arrives on `stop`.
///
/// # Errors
/// Returns [`AudioError`] when no input device is available or the stream
/// cannot be built / started.
pub fn run_input<C>(callback: C, stop: &Receiver<()>) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    backend::run_input(callback, stop)
}

/// Like [`run_output`], but targeting a specific device.
///
/// * On Windows, `device` is matched against endpoint identifiers (exact
///   `{...}` form) and against friendly names (case-insensitive substring) as
///   reported by [`list_output_devices`].
/// * On Linux, `device` is a plain ALSA `hw:C,D` spec or a case-insensitive
///   substring of a label reported by [`list_output_devices`].
/// * On macOS, `device` is matched against device names as reported by
///   [`list_output_devices`] (exact or case-insensitive substring).
///
/// # Errors
/// Returns [`AudioError`] when no device matches or the stream cannot be
/// built / started.
pub fn run_output_on_device<F>(
    device: &str,
    sample_rate: u32,
    block_size: u32,
    callback: F,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    backend::run_output_on_device(Some(device), sample_rate, block_size, callback, stop)
}

/// Like [`run_input`], but capturing from a specific device; see
/// [`run_output_on_device`] for how `device` is interpreted on each platform.
///
/// # Errors
/// Returns [`AudioError`] when no device matches or the stream cannot be
/// built / started.
pub fn run_input_on_device<C>(
    device: &str,
    callback: C,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    backend::run_input_on_device(Some(device), callback, stop)
}

/// True when a default input capture device exists (used by callers that want
/// to fall back to synthetic sources without spinning up a stream).
pub fn has_default_input() -> bool {
    backend::has_default_input()
}

/// Names of the available output devices: human-readable friendly names
/// (Windows: via the MMDevice property store; Linux: card id plus PCM name),
/// with endpoint identifiers / `hw:C,D` specs as fallback.
///
/// # Errors
/// Returns [`AudioError`] if devices cannot be enumerated.
pub fn list_output_devices() -> Result<Vec<String>, AudioError> {
    backend::list_devices(false)
}

/// Names of the available input (capture) devices; see [`list_output_devices`]
/// for naming caveats.
///
/// # Errors
/// Returns [`AudioError`] if devices cannot be enumerated.
pub fn list_input_devices() -> Result<Vec<String>, AudioError> {
    backend::list_devices(true)
}
