//! Placeholder audio backend for platforms whose native implementation has
//! not landed yet (see `todo.md`, "Long-term: clean-room audio I/O").
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::sync::mpsc::Receiver;

use super::AudioError;

const UNAVAILABLE: &str = "built-in audio is not implemented on this platform yet";

pub(crate) fn list_devices(_capture: bool) -> Result<Vec<String>, AudioError> {
    Err(AudioError(UNAVAILABLE.into()))
}

pub(crate) fn has_default_input() -> bool {
    false
}

pub(crate) fn run_output<F>(
    _sample_rate: u32,
    _block_size: u32,
    _callback: F,
    _stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    Err(AudioError(UNAVAILABLE.into()))
}

pub(crate) fn run_input<C>(_callback: C, _stop: &Receiver<()>) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    Err(AudioError(UNAVAILABLE.into()))
}
