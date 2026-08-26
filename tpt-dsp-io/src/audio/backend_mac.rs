//! Clean-room CoreAudio backend for macOS, implemented directly over the
//! system frameworks.
//!
//! All foreign declarations below are written from scratch against Apple's
//! public C API — no `coreaudio-sys`, no `bindgen` output and no third-party
//! wrapper crates are involved. The symbols live in `AudioToolbox`,
//! `CoreAudio` and `CoreFoundation`, linked here as system frameworks.
//!
//! Design notes:
//!
//! - **Playback** uses the default-output AudioUnit (`'auol'/'def '`): the
//!   client format is declared as mono `f32` at the requested rate on the
//!   unit's output scope and the HAL performs format conversion into the
//!   device mix. Samples are produced from an `AURenderCallback` installed on
//!   input scope element 0.
//! - **Capture** uses a HALOutput unit (`'auol'/'hal '`): input I/O is
//!   enabled on element 1, output I/O disabled on element 0, and the client
//!   pulls interleaved `f32` data through a render callback on output scope
//!   element 1 (calling [`AudioUnitRender`] with bus 1 inside). The client
//!   channel count mirrors the hardware stream so no channel remapping is
//!   attempted.
//! - Devices are enumerated via `kAudioHardwarePropertyDevices` and named
//!   through `kAudioObjectPropertyName`; selection matches an exact name or a
//!   case-insensitive substring, falling back to the default-device
//!   properties (`kAudioHardwarePropertyDefault{Output,Input}Device`).
//! - The requested `block_size` is advisory: CoreAudio owns the render thread
//!   and its buffer granularity, so every callback delivers whatever frame
//!   count the HAL provides.
//!
//! Runtime validation requires a physical Mac; the code is compile-checked
//! for `x86_64-apple-darwin`.
//!
//! # License
//!
//! Dual licensed under MIT / Apache-2.0. Copyright TPT Solutions.

// FFI struct layouts mirror Apple's C headers verbatim, including reserved
// padding fields Rust never reads.
#![allow(dead_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc::Receiver, Arc};

use super::AudioError;

// ------------------------------------------------------------------ ABI types

/// Four-character-code (`OSType`) constant.
const fn fourcc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}

type OsStatus = i32;
type AudioComponent = *mut c_void;
type AudioUnit = *mut c_void;
type AudioDeviceId = u32;
type AudioObjectId = u32;
type CfStringRef = *const c_void;

const NO_ERR: OsStatus = 0;

#[repr(C)]
#[derive(Clone, Copy)]
struct ComponentDescription {
    component_type: u32,
    component_sub_type: u32,
    component_manufacturer: u32,
    component_flags: u32,
    component_flags_mask: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct StreamBasicDescription {
    sample_rate: f64,
    format_id: u32,
    format_flags: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    bytes_per_frame: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
    reserved: u32,
}

/// `kAudioFormatLinearPCM`
const FORMAT_LPCM: u32 = fourcc(b"lpcm");
/// `kLinearPCMFormatFlagIsFloat`
const LPCM_FLAG_IS_FLOAT: u32 = 1 << 0;
/// `kLinearPCMFormatFlagIsPacked`
const LPCM_FLAG_IS_PACKED: u32 = 1 << 3;

#[repr(C)]
struct SmpteTime {
    subframes: i16,
    subframe_divisor: i16,
    counter: u32,
    smpte_type: u32,
    flags: u32,
    hours: i16,
    minutes: i16,
    seconds: i16,
    frames: i16,
}

#[repr(C)]
struct AudioTimeStamp {
    sample_time: f64,
    host_time: u64,
    rate_scalar: f64,
    word_clock_time: u64,
    smpte_time: SmpteTime,
    flags: u32,
    reserved: u32,
}

#[repr(C)]
struct AudioBuffer {
    number_channels: u32,
    data_byte_size: u32,
    data: *mut c_void,
}

/// Variable-length C array trailing member; one inline [`AudioBuffer`] is all
/// this backend ever addresses directly.
#[repr(C)]
struct AudioBufferList {
    number_buffers: u32,
    buffers: [AudioBuffer; 1],
}

type RenderCallback = unsafe extern "C" fn(
    refcon: *mut c_void,
    action_flags: *mut u32,
    time_stamp: *const AudioTimeStamp,
    bus_number: u32,
    number_frames: u32,
    io_data: *mut AudioBufferList,
) -> OsStatus;

#[repr(C)]
struct RenderCallbackStruct {
    proc_: Option<RenderCallback>,
    refcon: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ObjectPropertyAddress {
    selector: u32,
    scope: u32,
    element: u32,
}

// ------------------------------------------------------------- enum constants

const K_AUDIO_UNIT_TYPE_OUTPUT: u32 = fourcc(b"auol");
const K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT: u32 = fourcc(b"def ");
const K_AUDIO_UNIT_SUBTYPE_HAL_OUTPUT: u32 = fourcc(b"hal ");
const K_AUDIO_MANUFACTURER_APPLE: u32 = fourcc(b"appl");

const K_AUDIO_UNIT_SCOPE_OUTPUT: u32 = 0;
const K_AUDIO_UNIT_SCOPE_INPUT: u32 = 1;
const K_AUDIO_UNIT_SCOPE_GLOBAL: u32 = 0;

/// `kAudioUnitProperty_StreamFormat`
const PROP_STREAM_FORMAT: u32 = 8;
/// `kAudioUnitProperty_SetRenderCallback`
const PROP_SET_RENDER_CALLBACK: u32 = 23;
/// `kAudioOutputUnitProperty_EnableIO`
const PROP_ENABLE_IO: u32 = 200;
/// `kAudioOutputUnitProperty_CurrentDevice`
const PROP_CURRENT_DEVICE: u32 = 201;

const K_AUDIO_OBJECT_SYSTEM_OBJECT: AudioObjectId = 1;
const SCOPE_GLOBAL: u32 = fourcc(b"glob");
const SCOPE_OUTPUT: u32 = fourcc(b"out ");
const SCOPE_INPUT: u32 = fourcc(b"inpt");

const SEL_HARDWARE_DEVICES: u32 = fourcc(b"dev#");
const SEL_DEFAULT_OUTPUT_DEVICE: u32 = fourcc(b"dOut");
const SEL_DEFAULT_INPUT_DEVICE: u32 = fourcc(b"dIn ");
const SEL_OBJECT_NAME: u32 = fourcc(b"lnam");
const SEL_STREAM_CONFIGURATION: u32 = fourcc(b"stm#");

/// `kCFStringEncodingUTF8`
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// -------------------------------------------------------- foreign declarations

#[link(name = "AudioToolbox", kind = "framework")]
extern "C" {
    fn AudioComponentFindNext(
        component: AudioComponent,
        description: *const ComponentDescription,
    ) -> AudioComponent;
    fn AudioComponentInstanceNew(component: AudioComponent, instance: *mut AudioUnit) -> OsStatus;
    fn AudioComponentInstanceDispose(instance: AudioUnit) -> OsStatus;
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioUnitInitialize(unit: AudioUnit) -> OsStatus;
    fn AudioUnitUninitialize(unit: AudioUnit) -> OsStatus;
    fn AudioOutputUnitStart(unit: AudioUnit) -> OsStatus;
    fn AudioOutputUnitStop(unit: AudioUnit) -> OsStatus;
    fn AudioUnitSetProperty(
        unit: AudioUnit,
        property_id: u32,
        scope: u32,
        element: u32,
        data: *const c_void,
        data_size: u32,
    ) -> OsStatus;
    fn AudioUnitGetProperty(
        unit: AudioUnit,
        property_id: u32,
        scope: u32,
        element: u32,
        data: *mut c_void,
        data_size: *mut u32,
    ) -> OsStatus;
    fn AudioUnitRender(
        unit: AudioUnit,
        io_action_flags: *mut u32,
        time_stamp: *const AudioTimeStamp,
        bus_number: u32,
        number_frames: u32,
        io_data: *mut AudioBufferList,
    ) -> OsStatus;
    fn AudioObjectGetPropertyDataSize(
        object_id: AudioObjectId,
        address: *const ObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        out_data_size: *mut u32,
    ) -> OsStatus;
    fn AudioObjectGetPropertyData(
        object_id: AudioObjectId,
        address: *const ObjectPropertyAddress,
        qualifier_data_size: u32,
        qualifier_data: *const c_void,
        io_data_size: *mut u32,
        out_data: *mut c_void,
    ) -> OsStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringGetCString(
        string: CfStringRef,
        buffer: *mut u8,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFRelease(reference: *const c_void);
}

fn status_to_result(status: OsStatus, what: &str) -> Result<(), AudioError> {
    if status == NO_ERR {
        Ok(())
    } else {
        Err(AudioError(format!("CoreAudio {what}: OSStatus {status}")))
    }
}

// ------------------------------------------------------------ device plumbing

fn get_u32_object_property(
    object_id: AudioObjectId,
    selector: u32,
    scope: u32,
) -> Result<u32, AudioError> {
    let address = ObjectPropertyAddress {
        selector,
        scope,
        element: 0,
    };
    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object_id,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&mut value) as *mut u32 as *mut c_void,
        )
    };
    status_to_result(status, "get property")?;
    Ok(value)
}

fn cfstring_to_string(string: CfStringRef) -> String {
    if string.is_null() {
        return String::new();
    }
    let mut buf = [0u8; 256];
    let ok = unsafe {
        CFStringGetCString(
            string,
            buf.as_mut_ptr(),
            buf.len() as isize,
            CF_STRING_ENCODING_UTF8,
        )
    };
    unsafe { CFRelease(string) };
    if ok {
        std::ffi::CStr::from_bytes_until_nul(&buf)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    }
}

fn device_name(device_id: AudioDeviceId) -> Result<String, AudioError> {
    let address = ObjectPropertyAddress {
        selector: SEL_OBJECT_NAME,
        scope: SCOPE_GLOBAL,
        element: 0,
    };
    let mut name: CfStringRef = std::ptr::null();
    let mut size = std::mem::size_of::<*const c_void>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut size,
            (&mut name) as *mut CfStringRef as *mut c_void,
        )
    };
    status_to_result(status, "device name")?;
    Ok(cfstring_to_string(name))
}

/// Total channel count offered by `device_id` on `scope`, or `None` when the
/// device exposes no streams there.
fn device_channels(device_id: AudioDeviceId, scope: u32) -> Result<Option<u32>, AudioError> {
    let address = ObjectPropertyAddress {
        selector: SEL_STREAM_CONFIGURATION,
        scope,
        element: 0,
    };
    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(device_id, &address, 0, std::ptr::null(), &mut size)
    };
    if status != NO_ERR || size < std::mem::size_of::<u32>() as u32 {
        return Ok(None);
    }
    let mut raw = vec![0u8; size as usize];
    let mut read = size;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device_id,
            &address,
            0,
            std::ptr::null(),
            &mut read,
            raw.as_mut_ptr() as *mut c_void,
        )
    };
    status_to_result(status, "stream configuration")?;
    let list = unsafe { &*(raw.as_ptr() as *const AudioBufferList) };
    // Walk the variable-length buffer array manually; `number_buffers` may
    // exceed the single inline element.
    let base = unsafe {
        raw.as_ptr()
            .add(std::mem::offset_of!(AudioBufferList, buffers))
    }
    .cast::<AudioBuffer>();
    let mut channels = 0u32;
    for index in 0..list.number_buffers as usize {
        let buffer = unsafe { &*base.add(index) };
        channels += buffer.number_channels;
    }
    Ok(if channels > 0 { Some(channels) } else { None })
}

fn all_device_ids() -> Result<Vec<AudioDeviceId>, AudioError> {
    let address = ObjectPropertyAddress {
        selector: SEL_HARDWARE_DEVICES,
        scope: SCOPE_GLOBAL,
        element: 0,
    };
    let mut size: u32 = 0;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            std::ptr::null(),
            &mut size,
        )
    };
    status_to_result(status, "device list size")?;
    let count = size as usize / std::mem::size_of::<AudioDeviceId>();
    let mut ids = vec![0u32; count];
    let mut read = size;
    let status = unsafe {
        AudioObjectGetPropertyData(
            K_AUDIO_OBJECT_SYSTEM_OBJECT,
            &address,
            0,
            std::ptr::null(),
            &mut read,
            ids.as_mut_ptr() as *mut c_void,
        )
    };
    status_to_result(status, "device list")?;
    Ok(ids)
}

fn default_device_id(capture: bool) -> Result<u32, AudioError> {
    let selector = if capture {
        SEL_DEFAULT_INPUT_DEVICE
    } else {
        SEL_DEFAULT_OUTPUT_DEVICE
    };
    let id = get_u32_object_property(K_AUDIO_OBJECT_SYSTEM_OBJECT, selector, SCOPE_GLOBAL)?;
    if id == 0 {
        return Err(AudioError("no default audio device".into()));
    }
    Ok(id)
}

pub(crate) fn list_devices(capture: bool) -> Result<Vec<String>, AudioError> {
    let wanted_scope = if capture { SCOPE_INPUT } else { SCOPE_OUTPUT };
    let mut names = Vec::new();
    for id in all_device_ids()? {
        if device_channels(id, wanted_scope)?.is_some() {
            names.push(device_name(id)?);
        }
    }
    Ok(names)
}

/// Resolve `device` (exact name or case-insensitive substring) to a device
/// id, or fall back to the platform default when `None`.
fn resolve_device_id(capture: bool, device: Option<&str>) -> Result<u32, AudioError> {
    let Some(spec) = device else {
        return default_device_id(capture);
    };
    let spec_lower = spec.to_lowercase();
    let wanted_scope = if capture { SCOPE_INPUT } else { SCOPE_OUTPUT };
    for id in all_device_ids()? {
        if device_channels(id, wanted_scope)?.is_none() {
            continue;
        }
        let name = device_name(id)?;
        if name == spec || name.to_lowercase().contains(&spec_lower) {
            return Ok(id);
        }
    }
    Err(AudioError(format!("no audio device matching {spec:?}")))
}

pub(crate) fn has_default_input() -> bool {
    get_u32_object_property(
        K_AUDIO_OBJECT_SYSTEM_OBJECT,
        SEL_DEFAULT_INPUT_DEVICE,
        SCOPE_GLOBAL,
    )
    .map(|id| id != 0)
    .unwrap_or(false)
}

// --------------------------------------------------------------- unit helpers

fn new_unit(sub_type: u32) -> Result<AudioUnit, AudioError> {
    let description = ComponentDescription {
        component_type: K_AUDIO_UNIT_TYPE_OUTPUT,
        component_sub_type: sub_type,
        component_manufacturer: K_AUDIO_MANUFACTURER_APPLE,
        component_flags: 0,
        component_flags_mask: 0,
    };
    let component = unsafe { AudioComponentFindNext(std::ptr::null_mut(), &description) };
    if component.is_null() {
        return Err(AudioError(
            "CoreAudio output unit component not found".into(),
        ));
    }
    let mut unit: AudioUnit = std::ptr::null_mut();
    status_to_result(
        unsafe { AudioComponentInstanceNew(component, &mut unit) },
        "component instance new",
    )?;
    Ok(unit)
}

fn client_format(sample_rate: f64, channels: u32) -> StreamBasicDescription {
    StreamBasicDescription {
        sample_rate,
        format_id: FORMAT_LPCM,
        format_flags: LPCM_FLAG_IS_FLOAT | LPCM_FLAG_IS_PACKED,
        bytes_per_packet: channels * 4,
        frames_per_packet: 1,
        bytes_per_frame: channels * 4,
        channels_per_frame: channels,
        bits_per_channel: 32,
        reserved: 0,
    }
}

fn set_stream_format(
    unit: AudioUnit,
    scope: u32,
    element: u32,
    format: &StreamBasicDescription,
) -> Result<(), AudioError> {
    status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_STREAM_FORMAT,
                scope,
                element,
                (format as *const StreamBasicDescription) as *const c_void,
                std::mem::size_of::<StreamBasicDescription>() as u32,
            )
        },
        "set stream format",
    )
}

fn start_unit(unit: AudioUnit) -> Result<(), AudioError> {
    status_to_result(unsafe { AudioUnitInitialize(unit) }, "initialize")?;
    if let Err(err) = status_to_result(unsafe { AudioOutputUnitStart(unit) }, "start") {
        let _ = unsafe { AudioUnitUninitialize(unit) };
        return Err(err);
    }
    Ok(())
}

/// Tear the stream down from the control thread; the render callback has
/// already observed `stopped`, so no further renders reach user code.
fn teardown(unit: AudioUnit) {
    unsafe {
        AudioOutputUnitStop(unit);
        AudioUnitUninitialize(unit);
        AudioComponentInstanceDispose(unit);
    }
}

/// Block the control thread until a stop message arrives (or the sender is
/// dropped). Polling with a timeout keeps the thread responsive even if the
/// caller leaks the sender.
fn wait_for_stop(stop: &Receiver<()>) {
    loop {
        match stop.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

// ------------------------------------------------------------------- playback

struct OutputCtx<F> {
    stopped: Arc<AtomicBool>,
    callback: F,
    /// Scratch reused across renders so the hot path stays allocation-free.
    scratch: Vec<f32>,
}

unsafe extern "C" fn output_trampoline<F>(
    refcon: *mut c_void,
    _action_flags: *mut u32,
    _time_stamp: *const AudioTimeStamp,
    _bus_number: u32,
    number_frames: u32,
    io_data: *mut AudioBufferList,
) -> OsStatus
where
    F: FnMut(&mut [f32]),
{
    let ctx = unsafe { &mut *(refcon as *mut OutputCtx<F>) };
    if ctx.stopped.load(Ordering::Relaxed) {
        return 1; // no data; the unit is being torn down anyway
    }
    let list = unsafe { &mut *io_data };
    if list.number_buffers == 0 {
        return -50; // kAudioUnitErr_InvalidParameter
    }
    let buffer = &mut list.buffers[0];
    let capacity = buffer.data_byte_size as usize / std::mem::size_of::<f32>();
    let samples = capacity.min(number_frames as usize);
    let out = unsafe { std::slice::from_raw_parts_mut(buffer.data as *mut f32, capacity) };
    ctx.scratch.clear();
    ctx.scratch.resize(samples, 0.0);
    (ctx.callback)(&mut ctx.scratch[..samples]);
    out[..samples].copy_from_slice(&ctx.scratch[..samples]);
    NO_ERR
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
    let device_id = resolve_device_id(false, device)?;
    let unit = new_unit(K_AUDIO_UNIT_SUBTYPE_DEFAULT_OUTPUT)?;
    // Mono f32 client format; the default-output unit converts to the device
    // mix format and rate.
    set_stream_format(
        unit,
        K_AUDIO_UNIT_SCOPE_OUTPUT,
        0,
        &client_format(f64::from(sample_rate.max(1)), 1),
    )?;
    let stopped = Arc::new(AtomicBool::new(false));
    let ctx = Box::into_raw(Box::new(OutputCtx {
        stopped: Arc::clone(&stopped),
        callback,
        scratch: Vec::with_capacity(block_size.max(1) as usize),
    }));
    let cb = RenderCallbackStruct {
        proc_: Some(output_trampoline::<F>),
        refcon: ctx.cast(),
    };
    let install = status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_SET_RENDER_CALLBACK,
                K_AUDIO_UNIT_SCOPE_INPUT,
                0,
                (&cb as *const RenderCallbackStruct) as *const c_void,
                std::mem::size_of::<RenderCallbackStruct>() as u32,
            )
        },
        "set render callback",
    )
    .and_then(|()| {
        status_to_result(
            unsafe {
                AudioUnitSetProperty(
                    unit,
                    PROP_CURRENT_DEVICE,
                    K_AUDIO_UNIT_SCOPE_GLOBAL,
                    0,
                    (&device_id as *const AudioDeviceId) as *const c_void,
                    std::mem::size_of::<AudioDeviceId>() as u32,
                )
            },
            "set current device",
        )
    });
    if let Err(err) = install.and_then(|()| start_unit(unit)) {
        unsafe { drop(Box::from_raw(ctx)) };
        let _ = unsafe { AudioComponentInstanceDispose(unit) };
        return Err(err);
    }
    wait_for_stop(stop);
    stopped.store(true, Ordering::Relaxed);
    teardown(unit);
    unsafe { drop(Box::from_raw(ctx)) };
    Ok(())
}

// -------------------------------------------------------------------- capture

struct InputCtx<C> {
    unit: AudioUnit,
    stopped: Arc<AtomicBool>,
    callback: C,
    channels: u32,
    scratch: Vec<f32>,
}

unsafe extern "C" fn input_trampoline<C>(
    refcon: *mut c_void,
    action_flags: *mut u32,
    time_stamp: *const AudioTimeStamp,
    bus_number: u32,
    number_frames: u32,
    _io_data: *mut AudioBufferList,
) -> OsStatus
where
    C: FnMut(&[f32], usize),
{
    let ctx = unsafe { &mut *(refcon as *mut InputCtx<C>) };
    if ctx.stopped.load(Ordering::Relaxed) {
        return 1;
    }
    let mut list = AudioBufferList {
        number_buffers: 1,
        buffers: [AudioBuffer {
            number_channels: ctx.channels,
            data_byte_size: (number_frames * ctx.channels * 4) as u32,
            data: std::ptr::null_mut(),
        }],
    };
    let mut flags: u32 = unsafe { *action_flags };
    let status = unsafe {
        AudioUnitRender(
            ctx.unit,
            &mut flags,
            time_stamp,
            bus_number,
            number_frames,
            &mut list,
        )
    };
    if status != NO_ERR {
        return status;
    }
    let buffer = &list.buffers[0];
    let available = buffer.data_byte_size as usize / std::mem::size_of::<f32>();
    ctx.scratch.clear();
    ctx.scratch.extend_from_slice(unsafe {
        std::slice::from_raw_parts(buffer.data as *const f32, available)
    });
    (ctx.callback)(&ctx.scratch, ctx.channels as usize);
    NO_ERR
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
    let device_id = resolve_device_id(true, device)?;
    let unit = new_unit(K_AUDIO_UNIT_SUBTYPE_HAL_OUTPUT)?;
    // Enable input on element 1, disable playback on element 0.
    let enable: u32 = 1;
    let disable: u32 = 0;
    status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_ENABLE_IO,
                K_AUDIO_UNIT_SCOPE_INPUT,
                1,
                (&enable as *const u32) as *const c_void,
                std::mem::size_of::<u32>() as u32,
            )
        },
        "enable input io",
    )?;
    status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_ENABLE_IO,
                K_AUDIO_UNIT_SCOPE_OUTPUT,
                0,
                (&disable as *const u32) as *const c_void,
                std::mem::size_of::<u32>() as u32,
            )
        },
        "disable output io",
    )?;
    status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_CURRENT_DEVICE,
                K_AUDIO_UNIT_SCOPE_GLOBAL,
                0,
                (&device_id as *const AudioDeviceId) as *const c_void,
                std::mem::size_of::<AudioDeviceId>() as u32,
            )
        },
        "set current device",
    )?;
    // Mirror the hardware channel count and rate: read back the device-side
    // stream format on the input bus now that the device is attached.
    let mut hw_format = StreamBasicDescription {
        sample_rate: 48_000.0,
        format_id: 0,
        format_flags: 0,
        bytes_per_packet: 0,
        frames_per_packet: 0,
        bytes_per_frame: 0,
        channels_per_frame: 1,
        bits_per_channel: 0,
        reserved: 0,
    };
    let mut size = std::mem::size_of::<StreamBasicDescription>() as u32;
    let got_hw = unsafe {
        AudioUnitGetProperty(
            unit,
            PROP_STREAM_FORMAT,
            K_AUDIO_UNIT_SCOPE_INPUT,
            1,
            (&mut hw_format as *mut StreamBasicDescription) as *mut c_void,
            &mut size,
        )
    } == NO_ERR;
    let channels = if got_hw && hw_format.channels_per_frame > 0 {
        hw_format.channels_per_frame
    } else {
        1
    };
    let rate = if got_hw && hw_format.sample_rate > 0.0 {
        hw_format.sample_rate
    } else {
        48_000.0
    };
    // Client format on the output side of the input bus (element 1).
    set_stream_format(
        unit,
        K_AUDIO_UNIT_SCOPE_OUTPUT,
        1,
        &client_format(rate, channels),
    )?;

    let stopped = Arc::new(AtomicBool::new(false));
    let ctx = Box::into_raw(Box::new(InputCtx {
        unit,
        stopped: Arc::clone(&stopped),
        callback,
        channels,
        scratch: Vec::with_capacity(1024 * channels as usize),
    }));
    let cb = RenderCallbackStruct {
        proc_: Some(input_trampoline::<C>),
        refcon: ctx.cast(),
    };
    // Capture callbacks are pulled from the output side of input bus 1.
    let install = status_to_result(
        unsafe {
            AudioUnitSetProperty(
                unit,
                PROP_SET_RENDER_CALLBACK,
                K_AUDIO_UNIT_SCOPE_OUTPUT,
                1,
                (&cb as *const RenderCallbackStruct) as *const c_void,
                std::mem::size_of::<RenderCallbackStruct>() as u32,
            )
        },
        "set input render callback",
    );
    if let Err(err) = install.and_then(|()| start_unit(unit)) {
        unsafe { drop(Box::from_raw(ctx)) };
        let _ = unsafe { AudioComponentInstanceDispose(unit) };
        return Err(err);
    }
    wait_for_stop(stop);
    stopped.store(true, Ordering::Relaxed);
    teardown(unit);
    unsafe { drop(Box::from_raw(ctx)) };
    Ok(())
}
