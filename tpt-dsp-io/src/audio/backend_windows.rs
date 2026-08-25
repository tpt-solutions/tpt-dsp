//! Shared-mode WASAPI backend, implemented directly in-tree.
//!
//! The COM interface vtables, GUIDs and constants below are hand-declared:
//! no `windows`/`winapi` wrapper crates are involved. Only the small surface
//! needed for event-driven shared-mode render/capture is bound.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

use std::ffi::c_void;
use std::slice;
use std::sync::mpsc::{channel, Receiver};
use std::thread;

use super::AudioError;

// ---------------------------------------------------------------- constants

// Win32 naming kept for FFI readability; the acronym lint is silenced here.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
type HRESULT = i32;

const CLSCTX_ALL: u32 = 0x17; // INPROC_SERVER | INPROC_HANDLER | LOCAL | REMOTE
const FLOW_RENDER: usize = 0;
const FLOW_CAPTURE: usize = 1;
const ROLE_CONSOLE: usize = 0;
const DEVICE_STATE_ACTIVE: usize = 1;
const SHARE_SHARED: usize = 0;
const FLAG_EVENT_CALLBACK: usize = 0x0004_0000;
const FLAG_AUTOCONVERT_PCM: usize = 0x8000_0000;
const FLAG_SRC_DEFAULT_QUALITY: usize = 0x0800_0000;
const COINIT_MULTITHREADED: u32 = 0x0000_0000;
const RPC_E_CHANGED_MODE: HRESULT = 0x8001_0106u32 as i32;
const WAIT_OBJECT_0: u32 = 0;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const AUDCLNT_BUFFERFLAGS_SILENT: u32 = 0x2;

/// Buffer requested from the audio engine, in 100 ns units (20 ms).
const BUFFER_HNS: usize = 200_000;
/// Event-wait timeout so the stop flag is checked regularly (ms).
const WAIT_SLICE_MS: u32 = 20;

// -------------------------------------------------------------------- GUIDs

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

const fn guid(d1: u32, d2: u16, d3: u16, d4: [u8; 8]) -> Guid {
    Guid { d1, d2, d3, d4 }
}

const CLSID_MM_DEVICE_ENUMERATOR: Guid = guid(
    0xBCDE0395,
    0xE52F,
    0x467C,
    [0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E],
);
const IID_IMM_DEVICE_ENUMERATOR: Guid = guid(
    0xA95664D2,
    0x9614,
    0x4F35,
    [0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6],
);
const IID_IAUDIO_CLIENT: Guid = guid(
    0x1CB9AD4C,
    0xDBFA,
    0x4C32,
    [0xB1, 0x78, 0xC2, 0xF5, 0x68, 0xA7, 0x03, 0xB2],
);
const IID_IAUDIO_RENDER_CLIENT: Guid = guid(
    0xF294ACFC,
    0x3146,
    0x4483,
    [0xA7, 0xBF, 0xAD, 0xDC, 0xA7, 0xC2, 0x60, 0xE2],
);
const IID_IAUDIO_CAPTURE_CLIENT: Guid = guid(
    0xC8ADBD64,
    0xE71E,
    0x48A0,
    [0xA4, 0xDE, 0x18, 0x5C, 0x39, 0x5C, 0xD3, 0x17],
);

// --------------------------------------------------------------- win32 fns

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, model: u32) -> HRESULT;
    fn CoCreateInstance(
        clsid: *const Guid,
        outer: *mut c_void,
        clsctx: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> HRESULT;
    fn CoTaskMemFree(p: *mut c_void);
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        attrs: *mut c_void,
        manual_reset: i32,
        initial: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn CloseHandle(h: *mut c_void) -> i32;
    fn WaitForSingleObject(h: *mut c_void, ms: u32) -> u32;
    fn SetEvent(h: *mut c_void) -> i32;
}

/// Invoke COM vtable slot `idx` with pointer-sized arguments. Every method we
/// call returns an `HRESULT`; out-values come back through pointer arguments.
macro_rules! vt_call {
    ($obj:expr, $idx:expr $(, $arg:expr)* $(,)?) => {{
        let vt: *mut usize = *($obj as *mut *mut usize);
        let slot: unsafe extern "system" fn(*mut c_void, ...) -> HRESULT =
            core::mem::transmute(*vt.add($idx));
        let hr: HRESULT = slot($obj $(, $arg)*);
        hr
    }};
}

/// Release a COM object (vtable slot 2); returns the resulting refcount.
unsafe fn release(obj: *mut c_void) -> u32 {
    if obj.is_null() {
        return 0;
    }
    let vt: *mut usize = *(obj as *mut *mut usize);
    let f: unsafe extern "system" fn(*mut c_void) -> u32 = core::mem::transmute(*vt.add(2));
    f(obj)
}

fn com_init() -> Result<(), AudioError> {
    // SAFETY: plain Win32 call; S_OK/S_FALSE and RPC_E_CHANGED_MODE all mean
    // COM is initialised on this thread.
    let hr = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_MULTITHREADED) };
    if hr < 0 && hr != RPC_E_CHANGED_MODE {
        return Err(AudioError(format!("CoInitializeEx failed: hr=0x{hr:08X}")));
    }
    Ok(())
}

fn create_enumerator() -> Result<*mut c_void, AudioError> {
    let mut out: *mut c_void = std::ptr::null_mut();
    // SAFETY: out-pointer is a valid local; GUIDs are static constants.
    let hr = unsafe {
        CoCreateInstance(
            &CLSID_MM_DEVICE_ENUMERATOR,
            std::ptr::null_mut(),
            CLSCTX_ALL,
            &IID_IMM_DEVICE_ENUMERATOR,
            &mut out,
        )
    };
    if hr < 0 || out.is_null() {
        return Err(AudioError(format!(
            "CoCreateInstance(MMDeviceEnumerator): hr=0x{hr:08X}"
        )));
    }
    Ok(out)
}

fn err(context: &str, hr: HRESULT) -> AudioError {
    AudioError(format!("{context}: hr=0x{hr:08X}"))
}

/// Endpoint identifier string for one device (`IMMDevice::GetId`, slot 5).
unsafe fn device_id(dev: *mut c_void) -> Result<String, AudioError> {
    let mut pw: *mut u16 = std::ptr::null_mut();
    let hr = vt_call!(dev, 5, &mut pw);
    if hr < 0 || pw.is_null() {
        return Err(err("IMMDevice::GetId", hr));
    }
    let mut len = 0usize;
    while *pw.add(len) != 0 {
        len += 1;
    }
    let s = String::from_utf16_lossy(slice::from_raw_parts(pw, len));
    CoTaskMemFree(pw.cast());
    Ok(s)
}

pub(crate) fn list_devices(capture: bool) -> Result<Vec<String>, AudioError> {
    com_init()?;
    let flow = if capture { FLOW_CAPTURE } else { FLOW_RENDER };
    // SAFETY: all out-pointers are valid locals; objects are released on exit.
    unsafe {
        let enumerator = create_enumerator()?;
        let mut coll: *mut c_void = std::ptr::null_mut();
        let hr = vt_call!(enumerator, 3, flow, DEVICE_STATE_ACTIVE, &mut coll);
        if hr < 0 {
            release(enumerator);
            return Err(err("IMMDeviceEnumerator::EnumAudioEndpoints", hr));
        }
        let mut count: u32 = 0;
        let hr = vt_call!(coll, 3, &mut count);
        if hr < 0 {
            release(coll);
            release(enumerator);
            return Err(err("IMMDeviceCollection::GetCount", hr));
        }
        let mut names = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let mut dev: *mut c_void = std::ptr::null_mut();
            if vt_call!(coll, 4, i, &mut dev) >= 0 && !dev.is_null() {
                match device_id(dev) {
                    Ok(id) => names.push(id),
                    Err(_) => names.push(format!("device {i}")),
                }
                release(dev);
            }
        }
        release(coll);
        release(enumerator);
        Ok(names)
    }
}

pub(crate) fn has_default_input() -> bool {
    if com_init().is_err() {
        return false;
    }
    // SAFETY: out-pointer is a valid local; the device is released.
    unsafe {
        let Ok(enumerator) = create_enumerator() else {
            return false;
        };
        let mut dev: *mut c_void = std::ptr::null_mut();
        let ok =
            vt_call!(enumerator, 4, FLOW_CAPTURE, ROLE_CONSOLE, &mut dev) >= 0 && !dev.is_null();
        if !dev.is_null() {
            release(dev);
        }
        release(enumerator);
        ok
    }
}

/// Activate the default endpoint of `flow` and return its `IAudioClient`.
unsafe fn default_audio_client(flow: usize) -> Result<*mut c_void, AudioError> {
    let enumerator = create_enumerator()?;
    let mut dev: *mut c_void = std::ptr::null_mut();
    let hr = vt_call!(enumerator, 4, flow, ROLE_CONSOLE, &mut dev); // GetDefaultAudioEndpoint
    release(enumerator);
    if hr < 0 || dev.is_null() {
        return Err(AudioError("no default audio device available".to_string()));
    }
    let mut client: *mut c_void = std::ptr::null_mut();
    let hr = vt_call!(dev, 3, &IID_IAUDIO_CLIENT, CLSCTX_ALL, 0, &mut client); // Activate
    release(dev);
    if hr < 0 || client.is_null() {
        return Err(err("IMMDevice::Activate(IAudioClient)", hr));
    }
    Ok(client)
}

// ------------------------------------------------------------------ events

struct EventHandle(*mut c_void);

impl EventHandle {
    fn create() -> Result<Self, AudioError> {
        // SAFETY: plain Win32 call; unnamed auto-reset event.
        let h = unsafe { CreateEventW(std::ptr::null_mut(), 0, 0, std::ptr::null()) };
        if h.is_null() {
            return Err(AudioError("CreateEventW failed".into()));
        }
        Ok(Self(h))
    }

    fn signal(&self) {
        // SAFETY: valid handle created above.
        unsafe {
            SetEvent(self.0);
        }
    }
}

impl Drop for EventHandle {
    fn drop(&mut self) {
        // SAFETY: valid handle created in [`EventHandle::create`].
        unsafe {
            CloseHandle(self.0);
        }
    }
}

// ------------------------------------------------------------ public entry

pub(crate) fn run_output<F>(
    sample_rate: u32,
    block_size: u32,
    callback: F,
    stop: &Receiver<()>,
) -> Result<(), AudioError>
where
    F: FnMut(&mut [f32]) + Send + 'static,
{
    com_init()?;
    let device_event = EventHandle::create()?;
    let stop_event = EventHandle::create()?;
    let (ready_tx, ready_rx) = channel::<Result<(), AudioError>>();
    let dev_ev = device_event.0 as usize;
    let stop_ev = stop_event.0 as usize;
    let handle = thread::spawn(move || {
        let result = unsafe {
            output_loop(
                sample_rate,
                block_size,
                callback,
                dev_ev as *mut c_void,
                stop_ev as *mut c_void,
            )
        };
        let _ = ready_tx.send(result);
    });

    ready_rx
        .recv()
        .map_err(|_| AudioError("audio output thread died".into()))??;

    // Block the caller until the producer asks us to stop, then tear down.
    let _ = stop.recv();
    stop_event.signal();
    let _ = handle.join();
    Ok(())
}

unsafe fn output_loop(
    sample_rate: u32,
    block_size: u32,
    mut callback: impl FnMut(&mut [f32]),
    device_event: *mut c_void,
    stop_event: *mut c_void,
) -> Result<(), AudioError> {
    let client = default_audio_client(FLOW_RENDER)?;
    let result = drive_render(
        client,
        sample_rate,
        block_size,
        &mut callback,
        device_event,
        stop_event,
    );
    release(client);
    result
}

unsafe fn drive_render(
    client: *mut c_void,
    sample_rate: u32,
    block_size: u32,
    callback: &mut dyn FnMut(&mut [f32]),
    device_event: *mut c_void,
    stop_event: *mut c_void,
) -> Result<(), AudioError> {
    let format = WaveFormatEx {
        format_tag: WAVE_FORMAT_IEEE_FLOAT,
        channels: 1,
        samples_per_sec: sample_rate,
        avg_bytes_per_sec: sample_rate * 4,
        block_align: 4,
        bits_per_sample: 32,
        extra_size: 0,
    };
    let flags = FLAG_EVENT_CALLBACK | FLAG_AUTOCONVERT_PCM | FLAG_SRC_DEFAULT_QUALITY;
    let mut hr = vt_call!(client, 3, SHARE_SHARED, flags, BUFFER_HNS, 0, &format, 0); // Initialize
    if hr < 0 {
        return Err(err("IAudioClient::Initialize(render)", hr));
    }
    let mut buffer_frames: u32 = 0;
    hr = vt_call!(client, 4, &mut buffer_frames); // GetBufferSize
    if hr < 0 {
        return Err(err("IAudioClient::GetBufferSize", hr));
    }
    let mut render: *mut c_void = std::ptr::null_mut();
    hr = vt_call!(client, 14, &IID_IAUDIO_RENDER_CLIENT, &mut render); // GetService
    if hr < 0 || render.is_null() {
        return Err(err("IAudioClient::GetService(IAudioRenderClient)", hr));
    }
    let teardown = || {
        release(render);
    };
    hr = vt_call!(client, 13, device_event); // SetEventHandle
    if hr < 0 {
        teardown();
        return Err(err("IAudioClient::SetEventHandle", hr));
    }
    hr = vt_call!(client, 10); // Start
    if hr < 0 {
        teardown();
        return Err(err("IAudioClient::Start", hr));
    }

    loop {
        let woke = WaitForSingleObject(device_event, WAIT_SLICE_MS);
        if is_stopped(stop_event) {
            break;
        }
        if woke != WAIT_OBJECT_0 {
            continue;
        }
        let mut padding: u32 = 0;
        if vt_call!(client, 6, &mut padding) < 0 {
            break; // GetCurrentPadding failed; tear down
        }
        while padding < buffer_frames {
            let chunk = (buffer_frames - padding).min(block_size.max(1));
            let mut data: *mut u8 = std::ptr::null_mut();
            if vt_call!(render, 3, chunk, &mut data) < 0 || data.is_null() {
                break; // GetBuffer failed; drop this tick
            }
            let out = slice::from_raw_parts_mut(data.cast::<f32>(), chunk as usize);
            callback(out);
            vt_call!(render, 4, chunk, 0usize); // ReleaseBuffer
            padding += chunk;
        }
    }

    vt_call!(client, 11); // Stop
    teardown();
    Ok(())
}

fn is_stopped(stop_event: *mut c_void) -> bool {
    // SAFETY: zero-timeout wait doubles as a non-blocking check; the
    // auto-reset event consumes its own signal.
    unsafe { WaitForSingleObject(stop_event, 0) == WAIT_OBJECT_0 }
}

pub(crate) fn run_input<C>(callback: C, stop: &Receiver<()>) -> Result<(), AudioError>
where
    C: FnMut(&[f32], usize) + Send + 'static,
{
    com_init()?;
    let device_event = EventHandle::create()?;
    let stop_event = EventHandle::create()?;
    let (ready_tx, ready_rx) = channel::<Result<(), AudioError>>();
    let dev_ev = device_event.0 as usize;
    let stop_ev = stop_event.0 as usize;
    let handle = thread::spawn(move || {
        let result = unsafe { input_loop(callback, dev_ev as *mut c_void, stop_ev as *mut c_void) };
        let _ = ready_tx.send(result);
    });

    ready_rx
        .recv()
        .map_err(|_| AudioError("audio input thread died".into()))??;

    let _ = stop.recv();
    stop_event.signal();
    let _ = handle.join();
    Ok(())
}

#[repr(C)]
struct WaveFormatEx {
    format_tag: u16,
    channels: u16,
    samples_per_sec: u32,
    avg_bytes_per_sec: u32,
    block_align: u16,
    bits_per_sample: u16,
    extra_size: u16,
}

unsafe fn input_loop(
    mut callback: impl FnMut(&[f32], usize),
    device_event: *mut c_void,
    stop_event: *mut c_void,
) -> Result<(), AudioError> {
    let client = default_audio_client(FLOW_CAPTURE)?;
    let mut mix: *mut WaveFormatEx = std::ptr::null_mut();
    let mut hr = vt_call!(client, 8, &mut mix); // GetMixFormat
    if hr < 0 || mix.is_null() {
        release(client);
        return Err(err("IAudioClient::GetMixFormat", hr));
    }
    let channels = (*mix).channels.max(1) as usize;
    let tag = effective_format_tag(mix);
    // The mix format is always accepted in shared mode; no conversion flags.
    hr = vt_call!(
        client,
        3,
        SHARE_SHARED,
        FLAG_EVENT_CALLBACK,
        BUFFER_HNS,
        0,
        mix,
        0
    );
    CoTaskMemFree(mix.cast());
    if hr < 0 {
        release(client);
        return Err(err("IAudioClient::Initialize(capture)", hr));
    }
    let mut capture: *mut c_void = std::ptr::null_mut();
    hr = vt_call!(client, 14, &IID_IAUDIO_CAPTURE_CLIENT, &mut capture);
    if hr < 0 || capture.is_null() {
        release(client);
        return Err(err("IAudioClient::GetService(IAudioCaptureClient)", hr));
    }
    hr = vt_call!(client, 13, device_event);
    if hr < 0 {
        release(capture);
        release(client);
        return Err(err("IAudioClient::SetEventHandle", hr));
    }
    hr = vt_call!(client, 10); // Start
    if hr < 0 {
        release(capture);
        release(client);
        return Err(err("IAudioClient::Start", hr));
    }

    let mut converted: Vec<f32> = Vec::new();
    loop {
        let woke = WaitForSingleObject(device_event, WAIT_SLICE_MS);
        if is_stopped(stop_event) {
            break;
        }
        if woke != WAIT_OBJECT_0 {
            continue;
        }
        // Drain every queued packet before waiting again.
        loop {
            let mut packet_frames: u32 = 0;
            if vt_call!(capture, 5, &mut packet_frames) < 0 {
                break; // GetNextPacketSize failed
            }
            if packet_frames == 0 {
                break;
            }
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;
            let mut pos: u64 = 0;
            let mut qpc: u64 = 0;
            if vt_call!(
                capture,
                3,
                &mut data,
                &mut frames,
                &mut flags,
                &mut pos,
                &mut qpc
            ) < 0
                || data.is_null()
            {
                break;
            }
            convert_packet(tag, data, frames as usize, channels, flags, &mut converted);
            vt_call!(capture, 4, frames, 0usize); // ReleaseBuffer
            callback(&converted, channels);
        }
    }

    vt_call!(client, 11); // Stop
    release(capture);
    release(client);
    Ok(())
}

/// Resolve the effective format code, unwrapping `WAVE_FORMAT_EXTENSIBLE` to
/// the first two bytes of its SubFormat GUID (which starts at offset 24 of the
/// extensible `WAVEFORMATEX` body).
unsafe fn effective_format_tag(fmt: *const WaveFormatEx) -> u16 {
    let tag = (*fmt).format_tag;
    if tag != WAVE_FORMAT_EXTENSIBLE {
        return tag;
    }
    let p = (fmt as *const u8).add(24);
    u16::from_le_bytes([*p, *p.add(1)])
}

fn convert_packet(
    tag: u16,
    data: *const u8,
    frames: usize,
    channels: usize,
    flags: u32,
    out: &mut Vec<f32>,
) {
    out.clear();
    let samples = frames * channels;
    if samples == 0 {
        return;
    }
    if flags & AUDCLNT_BUFFERFLAGS_SILENT != 0 {
        out.resize(samples, 0.0);
        return;
    }
    // SAFETY: WASAPI guarantees `samples` elements of the declared format at
    // `data` between GetBuffer and ReleaseBuffer.
    unsafe {
        match tag {
            WAVE_FORMAT_IEEE_FLOAT => {
                out.extend_from_slice(slice::from_raw_parts(data.cast::<f32>(), samples));
            }
            WAVE_FORMAT_PCM => {
                let p = data.cast::<i16>();
                for i in 0..samples {
                    out.push(f32::from(*p.add(i)) / 32768.0);
                }
            }
            _ => out.resize(samples, 0.0),
        }
    }
}
