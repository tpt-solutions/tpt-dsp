//! Clean-room ALSA backend for Linux, implemented directly over the kernel
//! UAPI.
//!
//! PCM devices under `/dev/snd/pcmC{card}D{device}{p|c}` are opened with plain
//! `std::fs::File`s and driven with hand-declared `ioctl` calls — no
//! `libasound` linkage and no LGPL code is involved; the ABI constants below
//! mirror the GPL-2.0+syscall-note UAPI header
//! (`include/uapi/sound/asound.h`) and are re-declared from scratch here.
//!
//! Design notes:
//!
//! - Blocking-mode `RW_INTERLEAVED` transfers (`WRITEI_FRAMES`/`READI_FRAMES`)
//!   are used instead of mmap + `sync_ptr`. A blocking transfer sleeps in the
//!   kernel for at most one period, so the stop flag is checked between
//!   periods without extra synchronisation primitives.
//! - Sample format is negotiated in order `FLOAT_LE → S32_LE → S16_LE`; the
//!   chosen hardware format is converted to/from mono-or-stereo `f32`.
//! - Underruns/overruns (`-EPIPE`) are recovered with `PREPARE` (+`START`)
//!   rather than tearing the stream down.
//! - Devices are enumerated by scanning `/dev/snd`, named after the card id
//!   from `/proc/asound/cardN/id` plus the PCM name from
//!   `SNDRV_PCM_IOCTL_INFO`, and selected by exact `hw:C,D` spec or a
//!   case-insensitive substring of the reported name.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc::Receiver, Arc};
use std::thread;

use super::AudioError;

// ---------------------------------------------------------------- ioctl ABI

const fn ioc(dir: u32, kind: u8, nr: u8, size: usize) -> u64 {
    ((dir as u64) << 30) | ((size as u64) << 16) | ((kind as u64) << 8) | nr as u64
}
/// `_IO`
const fn io(kind: u8, nr: u8) -> u64 {
    ioc(0, kind, nr, 0)
}
/// `_IOR`
const fn ior<T>(kind: u8, nr: u8) -> u64 {
    ioc(2, kind, nr, core::mem::size_of::<T>())
}
/// `_IOWR`
const fn iowr<T>(kind: u8, nr: u8) -> u64 {
    ioc(3, kind, nr, core::mem::size_of::<T>())
}

const IOCTL_PVERSION: u64 = ior::<i32>(b'A', 0x00);
const IOCTL_INFO: u64 = ior::<PcmInfo>(b'A', 0x01);
const IOCTL_HW_PARAMS: u64 = iowr::<HwParams>(b'A', 0x11);
const IOCTL_SW_PARAMS: u64 = iowr::<SwParams>(b'A', 0x13);
const IOCTL_PREPARE: u64 = io(b'A', 0x40);
const IOCTL_START: u64 = io(b'A', 0x42);
const IOCTL_WRITEI_FRAMES: u64 = iowr::<XferI>(b'A', 0x50);
const IOCTL_READI_FRAMES: u64 = iowr::<XferI>(b'A', 0x51);

// ------------------------------------------------------------- enum values

const ACCESS_RW_INTERLEAVED: u32 = 3;
const FORMAT_S16_LE: u32 = 2;
const FORMAT_S32_LE: u32 = 10;
const FORMAT_FLOAT_LE: u32 = 14;
const STREAM_PLAYBACK: i32 = 0;
const STREAM_CAPTURE: i32 = 1;

// HW_PARAM indices (`snd_pcm_hw_param_t`).
const HW_PARAM_ACCESS: usize = 0;
const HW_PARAM_FORMAT: usize = 1;
const HW_PARAM_SUBFORMAT: usize = 2;
const HW_PARAM_SAMPLE_BITS: usize = 8;
const HW_PARAM_FRAME_BITS: usize = 9;
const HW_PARAM_CHANNELS: usize = 10;
const HW_PARAM_RATE: usize = 11;
const HW_PARAM_PERIOD_SIZE: usize = 13;
const HW_PARAM_PERIODS: usize = 15;
const HW_PARAM_BUFFER_SIZE: usize = 17;

/// libc `EPIPE`; signalled by ALSA on underrun/overrun (XRUN).
const EPIPE_RAW: i32 = 32;

extern "C" {
    fn ioctl(fd: i32, request: u64, ...) -> i32;
}

// ----------------------------------------------------------------- structs

/// `struct snd_mask`: 256-bit set (`SNDRV_MASK_MAX`).
#[repr(C)]
#[derive(Clone, Copy)]
struct SndMask {
    bits: [u32; 8],
}

impl SndMask {
    const FULL: SndMask = SndMask {
        bits: [u32::MAX; 8],
    };
    const EMPTY: SndMask = SndMask { bits: [0; 8] };

    fn set(&mut self, bit: u32) {
        self.bits[(bit / 32) as usize] |= 1 << (bit % 32);
    }

    /// The single set bit, when exactly one is set (post-refinement state).
    fn only(&self) -> Option<u32> {
        let mut found: Option<u32> = None;
        for (word, w) in self.bits.iter().enumerate() {
            let mut w = *w;
            while w != 0 {
                if found.is_some() {
                    return None;
                }
                found = Some(word as u32 * 32 + w.trailing_zeros());
                w &= w - 1;
            }
        }
        found
    }
}

/// `struct snd_interval`: closed integer range with refinement flags
/// (`openmin | openmax<<1 | integer<<2 | empty<<3`).
#[repr(C)]
#[derive(Clone, Copy)]
struct SndInterval {
    min: u32,
    max: u32,
    flags: u32,
}

impl SndInterval {
    const OPEN: SndInterval = SndInterval {
        min: 0,
        max: u32::MAX,
        flags: 0,
    };

    fn exact(value: u32) -> Self {
        SndInterval {
            min: value,
            max: value,
            flags: 0b100,
        }
    }

    fn range(min: u32, max: u32) -> Self {
        SndInterval {
            min,
            max,
            flags: 0b100,
        }
    }
}

/// `struct snd_pcm_hw_params` (608 bytes on LP64 — verified against the
/// UAPI layout: 4 + 3·32 + 5·32 + 12·12 + 9·12 + 24 + pad4 + 8 + 16 + 48).
#[repr(C)]
struct HwParams {
    flags: u32,
    masks: [SndMask; 3],
    mres: [SndMask; 5],
    intervals: [SndInterval; 12],
    ires: [SndInterval; 9],
    rmask: u32,
    cmask: u32,
    info: u32,
    msbits: u32,
    rate_num: u32,
    rate_den: u32,
    fifo_size: u64,
    sync: [u8; 16],
    reserved: [u8; 48],
}

const _: () = assert!(core::mem::size_of::<HwParams>() == 608);

impl HwParams {
    /// Equivalent of alsa-lib's `snd_pcm_hw_params_any`: the full parameter
    /// space, which the kernel then intersects with driver constraints.
    fn any() -> Self {
        HwParams {
            flags: 0,
            masks: [SndMask::FULL; 3],
            mres: [SndMask::EMPTY; 5],
            intervals: [SndInterval::OPEN; 12],
            ires: [SndInterval::OPEN; 9],
            rmask: u32::MAX,
            cmask: 0,
            info: 0,
            msbits: 0,
            rate_num: 0,
            rate_den: 0,
            fifo_size: 0,
            sync: [0; 16],
            reserved: [0; 48],
        }
    }

    fn mask_set(&mut self, param: usize, bit: u32) {
        self.masks[param].set(bit);
    }

    fn interval(&mut self, param: usize, value: SndInterval) {
        self.intervals[param] = value;
    }

    fn interval_max(&self, param: usize) -> u32 {
        self.intervals[param].max
    }
}

/// `struct snd_pcm_sw_params` (136 bytes on LP64).
#[repr(C)]
struct SwParams {
    tstamp_mode: i32,
    period_step: u32,
    sleep_min: u32,
    avail_min: u64,
    xfer_align: u64,
    start_threshold: u64,
    stop_threshold: u64,
    silence_threshold: u64,
    silence_size: u64,
    boundary: u64,
    proto: u32,
    tstamp_type: u32,
    reserved: [u8; 56],
}

const _: () = assert!(core::mem::size_of::<SwParams>() == 136);

impl Default for SwParams {
    fn default() -> Self {
        SwParams {
            tstamp_mode: 0,
            period_step: 0,
            sleep_min: 0,
            avail_min: 1,
            xfer_align: 1, // obsolete, kept at 1 as alsa-lib does
            start_threshold: 1,
            stop_threshold: 1 << 30,
            silence_threshold: 0,
            silence_size: 0,
            // The kernel tracks its own boundary; this field is not read back.
            boundary: 1 << 40,
            proto: 0,
            tstamp_type: 0,
            reserved: [0; 56],
        }
    }
}

/// `struct snd_xferi`.
#[repr(C)]
struct XferI {
    result: i64,
    buf: *mut c_void,
    frames: u64,
}

/// `struct snd_pcm_info`.
#[repr(C)]
struct PcmInfo {
    device: u32,
    subdevice: u32,
    stream: i32,
    card: i32,
    id: [u8; 64],
    name: [u8; 80],
    subname: [u8; 32],
    dev_class: i32,
    dev_subclass: i32,
    subdevices_count: u32,
    subdevices_avail: u32,
    pad1: [u8; 16],
    reserved: [u8; 64],
}

impl PcmInfo {
    fn new(stream: i32, card: u32, device: u32) -> Self {
        PcmInfo {
            device,
            subdevice: 0,
            stream,
            card: card as i32,
            id: [0; 64],
            name: [0; 80],
            subname: [0; 32],
            dev_class: 0,
            dev_subclass: 0,
            subdevices_count: 0,
            subdevices_avail: 0,
            pad1: [0; 16],
            reserved: [0; 64],
        }
    }

    fn c_field(field: &[u8]) -> String {
        let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
        String::from_utf8_lossy(&field[..end]).into_owned()
    }
}

// ------------------------------------------------------------------ helpers

fn do_ioctl(file: &File, request: u64, arg: *mut c_void) -> Result<(), AudioError> {
    // SAFETY: fd is valid; arg points to a struct matching the ioctl ABI.
    let ret = unsafe { ioctl(file.as_raw_fd(), request, arg) };
    if ret == -1 {
        Err(AudioError(format!(
            "ALSA ioctl 0x{request:X} failed: {}",
            io::Error::last_os_error()
        )))
    } else {
        Ok(())
    }
}

fn ioctl_ret(file: &File, request: u64, arg: *mut c_void) -> i32 {
    // SAFETY: as above.
    unsafe { ioctl(file.as_raw_fd(), request, arg) }
}

fn last_errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// One enumerated PCM endpoint.
struct PcmDeviceSpec {
    path: String,
    label: String,
}

fn card_name(card: u32) -> String {
    std::fs::read_to_string(format!("/proc/asound/card{card}/id"))
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|_| format!("card{card}"))
}

/// Scan `/dev/snd` for playback (`capture=false`) or capture endpoints.
fn enumerate(capture: bool) -> Result<Vec<PcmDeviceSpec>, AudioError> {
    if !Path::new("/dev/snd").is_dir() {
        return Err(AudioError(
            "/dev/snd does not exist (no ALSA sound system)".into(),
        ));
    }
    let mut entries: Vec<(u32, u32)> = Vec::new();
    let dir =
        std::fs::read_dir("/dev/snd").map_err(|e| AudioError(format!("read_dir /dev/snd: {e}")))?;
    for entry in dir {
        let name = entry
            .map_err(|e| AudioError(format!("readdir entry: {e}")))?
            .file_name()
            .to_string_lossy()
            .into_owned();
        // pcmC<card>D<device><p|c>
        let Some(rest) = name.strip_prefix("pcmC") else {
            continue;
        };
        let Some((c, d)) = rest.split_once('D') else {
            continue;
        };
        let (dev_num, suffix) = d.split_at(d.len().saturating_sub(1));
        let is_playback = suffix == "p";
        let is_capture = suffix == "c";
        if (capture && is_capture) || (!capture && is_playback) {
            if let (Ok(card), Ok(device)) = (c.parse::<u32>(), dev_num.parse::<u32>()) {
                entries.push((card, device));
            }
        }
    }
    entries.sort();
    entries.dedup();

    let mut specs = Vec::with_capacity(entries.len());
    for (card, device) in entries {
        let suffix = if capture { 'c' } else { 'p' };
        let path = format!("/dev/snd/pcmC{card}D{device}{suffix}");
        let stream = if capture {
            STREAM_CAPTURE
        } else {
            STREAM_PLAYBACK
        };
        // Devices held by PulseAudio/PipeWire fail to open — skip them.
        let Ok(file) = File::options().read(true).write(true).open(&path) else {
            continue;
        };
        let mut info = PcmInfo::new(stream, card, device);
        if do_ioctl(&file, IOCTL_INFO, &mut info as *mut _ as *mut c_void).is_err() {
            continue;
        }
        let label = format!(
            "hw:{card},{device} [{}: {}]",
            card_name(card),
            PcmInfo::c_field(&info.name)
        );
        specs.push(PcmDeviceSpec { path, label });
    }
    Ok(specs)
}

/// Resolve `device` (a `hw:C,D` spec or a case-insensitive substring of an
/// endpoint label) to a PCM node path; `None` picks the first endpoint.
fn resolve_path(capture: bool, device: Option<&str>) -> Result<String, AudioError> {
    let mut specs = enumerate(capture)?;
    if specs.is_empty() {
        return Err(AudioError("no usable ALSA PCM devices found".into()));
    }
    let Some(query) = device else {
        return Ok(specs.remove(0).path);
    };
    let needle = query.to_lowercase();
    // Accept both the reported label and a plain `hw:C,D` spec.
    let hw_spec = |path: &str| path.trim_start_matches("/dev/snd/pcm").to_lowercase();
    specs
        .iter()
        .find(|s| s.label.to_lowercase().contains(&needle) || hw_spec(&s.path).contains(&needle))
        .map(|s| s.path.clone())
        .ok_or_else(|| {
            let names: Vec<&str> = specs.iter().map(|s| s.label.as_str()).collect();
            AudioError(format!(
                "no ALSA device matching {query:?}; available: {}",
                names.join(", ")
            ))
        })
}

/// Effective negotiated configuration.
struct Config {
    channels: usize,
    format: u32,
    period_frames: u32,
    #[allow(dead_code)]
    buffer_frames: u32,
}

/// Open a PCM node, negotiate hardware parameters (`HW_PARAMS` + `SW_PARAMS`
/// + `PREPARE`) and return the file plus the effective configuration.
fn configure(
    path: &str,
    capture: bool,
    sample_rate: u32,
    block_size: u32,
) -> Result<(File, Config), AudioError> {
    let file = File::options()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| AudioError(format!("open {path}: {e}")))?;

    // Protocol sanity probe (also proves the node is a PCM device).
    let mut version: i32 = 0;
    do_ioctl(&file, IOCTL_PVERSION, &mut version as *mut _ as *mut c_void)?;

    let period = block_size.clamp(16, 4096);
    let mut hw = HwParams::any();
    hw.mask_set(HW_PARAM_ACCESS, ACCESS_RW_INTERLEAVED);
    hw.mask_set(HW_PARAM_FORMAT, FORMAT_FLOAT_LE);
    hw.mask_set(HW_PARAM_FORMAT, FORMAT_S32_LE);
    hw.mask_set(HW_PARAM_FORMAT, FORMAT_S16_LE);
    hw.mask_set(HW_PARAM_SUBFORMAT, 0); // STD
    hw.interval(HW_PARAM_SAMPLE_BITS, SndInterval::range(16, 64));
    hw.interval(HW_PARAM_FRAME_BITS, SndInterval::range(16, 1024));
    hw.interval(HW_PARAM_CHANNELS, SndInterval::range(1, 2));
    hw.interval(HW_PARAM_RATE, SndInterval::exact(sample_rate.max(1000)));
    hw.interval(HW_PARAM_PERIOD_SIZE, SndInterval::range(period, period * 4));
    hw.interval(HW_PARAM_PERIODS, SndInterval::range(2, 16));
    if ioctl_ret(&file, IOCTL_HW_PARAMS, &mut hw as *mut _ as *mut c_void) < 0 {
        return Err(AudioError(format!(
            "ALSA HW_PARAMS failed for {sample_rate} Hz on {path} ({});
             try a rate the device supports natively (e.g. 48000)",
            io::Error::last_os_error()
        )));
    }

    let format = hw.masks[HW_PARAM_FORMAT]
        .only()
        .ok_or_else(|| AudioError("ALSA left multiple formats selected".into()))?;
    let channels = hw.interval_max(HW_PARAM_CHANNELS).clamp(1, 2) as usize;
    let period_frames = hw.interval_max(HW_PARAM_PERIOD_SIZE).max(1);
    let buffer_frames = hw.interval_max(HW_PARAM_BUFFER_SIZE).max(period_frames);

    let mut sw = SwParams {
        avail_min: u64::from(period_frames),
        // Capture starts on the first readi; playback starts once the first
        // period is queued so the DMA does not start on an empty buffer.
        start_threshold: if capture { 1 } else { u64::from(period_frames) },
        stop_threshold: u64::from(buffer_frames),
        ..SwParams::default()
    };
    if ioctl_ret(&file, IOCTL_SW_PARAMS, &mut sw as *mut _ as *mut c_void) < 0 {
        return Err(AudioError(format!(
            "ALSA SW_PARAMS failed: {}",
            io::Error::last_os_error()
        )));
    }
    do_ioctl(&file, IOCTL_PREPARE, std::ptr::null_mut())?;

    Ok((
        file,
        Config {
            channels,
            format,
            period_frames,
            buffer_frames,
        },
    ))
}

/// Recover a stream from XRUN (`-EPIPE`) or suspend (`-ESTRPIPE`).
fn recover(file: &File, err: i32) -> Result<(), i32> {
    if err != -EPIPE_RAW {
        return Err(err);
    }
    if ioctl_ret(file, IOCTL_PREPARE, std::ptr::null_mut()) < 0 {
        return Err(last_errno());
    }
    // The auto-start threshold restarts playback on the next writei; the
    // explicit START is required for capture and harmless for playback.
    let _ = ioctl_ret(file, IOCTL_START, std::ptr::null_mut());
    Ok(())
}

fn to_f32(format: u32, raw: &[u8], out: &mut Vec<f32>) {
    out.clear();
    match format {
        FORMAT_FLOAT_LE => out.extend(
            raw.chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        ),
        FORMAT_S32_LE => out.extend(raw.chunks_exact(4).map(|b| {
            (i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2147483648.0).clamp(-1.0, 1.0)
        })),
        _ => out.extend(
            raw.chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0),
        ),
    }
}

const fn format_size(format: u32) -> usize {
    match format {
        FORMAT_FLOAT_LE | FORMAT_S32_LE => 4,
        _ => 2,
    }
}

fn from_f32(format: u32, samples: &[f32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(samples.len() * format_size(format));
    match format {
        FORMAT_FLOAT_LE => out.extend(samples.iter().flat_map(|s| s.to_le_bytes())),
        FORMAT_S32_LE => out.extend(
            samples
                .iter()
                .flat_map(|s| (((*s).clamp(-1.0, 1.0) * 2147483647.0) as i32).to_le_bytes()),
        ),
        _ => out.extend(
            samples
                .iter()
                .flat_map(|s| (((*s).clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()),
        ),
    }
}

fn is_stopped(stop: &StopFlag) -> bool {
    stop.load(std::sync::atomic::Ordering::Relaxed)
}

/// Shared stop flag bridging the `mpsc` stop channel and the stream thread
/// (a `Receiver` itself is not `Sync`, so it cannot be captured by `spawn`).
type StopFlag = std::sync::atomic::AtomicBool;

// ---------------------------------------------------------------- public API

pub(crate) fn list_devices(capture: bool) -> Result<Vec<String>, AudioError> {
    Ok(enumerate(capture)?.into_iter().map(|s| s.label).collect())
}

pub(crate) fn has_default_input() -> bool {
    enumerate(true).is_ok_and(|v| !v.is_empty())
}

pub(crate) fn run_output<F>(
    sample_rate: u32,
    block_size: u32,
    callback: F,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    run_output_on_device(None, sample_rate, block_size, callback, stop)
}

pub(crate) fn run_output_on_device<F>(
    device: Option<&str>,
    sample_rate: u32,
    block_size: u32,
    callback: F,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    let path = resolve_path(false, device)?;
    let (file, cfg) = configure(&path, false, sample_rate, block_size)?;
    // The stream thread owns the file and all scratch buffers; it exits at
    // the next period boundary after the stop flag is raised.
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let handle = thread::spawn(move || output_loop(file, cfg, callback, &flag));
    let _ = stop.recv();
    stopped.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = handle.join();
    Ok(())
}

fn output_loop<F>(
    file: File,
    cfg: Config,
    mut callback: F,
    stop: &StopFlag,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    let channels = cfg.channels;
    let period = cfg.period_frames as usize;
    let frame_bytes = channels * format_size(cfg.format);
    // All scratch is allocated up front; the loop below never allocates.
    let mut mono = vec![0.0f32; period];
    let mut interleaved = vec![0.0f32; period * channels];
    let mut raw: Vec<u8> = Vec::with_capacity(interleaved.len() * format_size(cfg.format));
    loop {
        if is_stopped(stop) {
            return Ok(());
        }
        callback(&mut mono[..]);
        for frame in 0..period {
            for ch in 0..channels {
                interleaved[frame * channels + ch] = mono[frame];
            }
        }
        from_f32(cfg.format, &interleaved, &mut raw);
        let mut offset = 0usize;
        while offset < raw.len() {
            let mut xfer = XferI {
                result: 0,
                buf: unsafe { raw.as_mut_ptr().add(offset).cast() },
                frames: ((raw.len() - offset) / frame_bytes) as u64,
            };
            let ret = ioctl_ret(
                &file,
                IOCTL_WRITEI_FRAMES,
                &mut xfer as *mut _ as *mut c_void,
            );
            if ret < 0 {
                let err = last_errno();
                if recover(&file, -err).is_ok() {
                    continue;
                }
                return Err(AudioError(format!("ALSA writei failed: errno {err}")));
            }
            if xfer.result <= 0 {
                return Err(AudioError(format!(
                    "ALSA writei short result {}",
                    xfer.result
                )));
            }
            offset += xfer.result as usize * frame_bytes;
        }
    }
}

pub(crate) fn run_input<C>(callback: C, stop: &Receiver<()>) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    run_input_on_device(None, callback, stop)
}

pub(crate) fn run_input_on_device<C>(
    device: Option<&str>,
    callback: C,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    let path = resolve_path(true, device)?;
    let (file, cfg) = configure(&path, true, 48_000, 512)?;
    let stopped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stopped);
    let handle = thread::spawn(move || input_loop(file, cfg, callback, &flag));
    let _ = stop.recv();
    stopped.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = handle.join();
    Ok(())
}

fn input_loop<C>(
    file: File,
    cfg: Config,
    mut callback: C,
    stop: &StopFlag,
) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    let channels = cfg.channels;
    let period = cfg.period_frames as usize;
    let frame_bytes = channels * format_size(cfg.format);
    let mut raw = vec![0u8; period * frame_bytes];
    let mut converted: Vec<f32> = Vec::with_capacity(period * channels);
    loop {
        if is_stopped(stop) {
            return Ok(());
        }
        let mut xfer = XferI {
            result: 0,
            buf: raw.as_mut_ptr().cast(),
            frames: period as u64,
        };
        let ret = ioctl_ret(
            &file,
            IOCTL_READI_FRAMES,
            &mut xfer as *mut _ as *mut c_void,
        );
        if ret < 0 {
            let err = last_errno();
            if recover(&file, -err).is_ok() {
                continue;
            }
            return Err(AudioError(format!("ALSA readi failed: errno {err}")));
        }
        if xfer.result > 0 {
            to_f32(
                cfg.format,
                &raw[..xfer.result as usize * frame_bytes],
                &mut converted,
            );
            callback(&converted, channels);
        }
    }
}
