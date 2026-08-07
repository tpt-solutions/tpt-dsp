// SPDX-License-Identifier: MIT OR Apache-2.0
//! Web Audio integration helpers.
//!
//! These wrap the handful of `web-sys` calls the demo page needs: loading the
//! AudioWorklet module, constructing the pedalboard node, and patching a
//! microphone `MediaStream` through it to the destination. The DSP itself runs
//! inside the worklet (see `www/pedal-processor.js`), which instantiates this
//! same wasm module and calls
//! [`Pedalboard::process_internal_block`](crate::Pedalboard::process_internal_block)
//! once per 128-sample render quantum.
//!
//! There is no `ScriptProcessorNode` fallback here on purpose:
//! `ScriptProcessorNode` is deprecated and runs on the main thread, so it
//! cannot honour the allocation-free, 128-sample real-time contract. If you
//! must support a browser without `AudioWorklet`, drive
//! [`Pedalboard::process_block`](crate::Pedalboard::process_block) from an
//! `onaudioprocess` handler and accept the added latency and jitter.

use js_sys::{Object, Promise};
use wasm_bindgen::prelude::*;
use web_sys::{
    AudioContext, AudioWorkletNode, AudioWorkletNodeOptions, MediaStream,
    MediaStreamAudioSourceNode,
};

/// Processor name registered by `www/pedal-processor.js`.
pub const PROCESSOR_NAME: &str = "tpt-pedalboard";

/// The `registerProcessor` name the worklet uses.
#[wasm_bindgen]
pub fn processor_name() -> String {
    PROCESSOR_NAME.to_string()
}

/// Load the AudioWorklet module at `module_url`.
///
/// Returns the `addModule` promise; `await` it before calling
/// [`create_pedal_node`].
#[wasm_bindgen]
pub fn register_worklet(context: &AudioContext, module_url: &str) -> Result<Promise, JsValue> {
    context.audio_worklet()?.add_module(module_url)
}

/// Construct the pedalboard worklet node.
///
/// `processor_options` is forwarded verbatim to the processor constructor and
/// must carry the compiled `WebAssembly.Module` (as `wasmModule`) so the
/// worklet can instantiate this crate inside its own global scope.
#[wasm_bindgen]
pub fn create_pedal_node(
    context: &AudioContext,
    processor_options: &Object,
) -> Result<AudioWorkletNode, JsValue> {
    let options = AudioWorkletNodeOptions::new();
    options.set_number_of_inputs(1);
    options.set_number_of_outputs(1);
    options.set_processor_options(Some(processor_options));
    AudioWorkletNode::new_with_options(context, PROCESSOR_NAME, &options)
}

/// Patch `stream` (typically the microphone / guitar interface input) through
/// `node` to the context destination, returning the created source node so the
/// caller can keep it alive and disconnect it later.
#[wasm_bindgen]
pub fn connect_stream(
    context: &AudioContext,
    stream: &MediaStream,
    node: &AudioWorkletNode,
) -> Result<MediaStreamAudioSourceNode, JsValue> {
    let source = context.create_media_stream_source(stream)?;
    source.connect_with_audio_node(node)?;
    node.connect_with_audio_node(&context.destination())?;
    Ok(source)
}

/// Request microphone access with browser processing disabled (echo
/// cancellation, noise suppression and AGC all mangle guitar signal).
///
/// Requires the `async` feature, which pulls in `wasm-bindgen-futures`.
#[cfg(feature = "async")]
#[wasm_bindgen]
pub async fn open_microphone() -> Result<MediaStream, JsValue> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::MediaStreamConstraints;

    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no global `window`"))?;
    let devices = window.navigator().media_devices()?;
    let audio = Object::new();
    for flag in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
        js_sys::Reflect::set(&audio, &JsValue::from_str(flag), &JsValue::FALSE)?;
    }
    let constraints = MediaStreamConstraints::new();
    constraints.set_audio(&audio);
    constraints.set_video(&JsValue::FALSE);
    let stream = JsFuture::from(devices.get_user_media_with_constraints(&constraints)?).await?;
    stream.dyn_into::<MediaStream>()
}
