// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Main-thread glue: boot the wasm module, load the AudioWorklet, and patch
// microphone -> pedalboard -> destination. All DSP happens inside the worklet.

import init, { connect_stream, create_pedal_node, register_worklet } from "../pkg/tpt_dsp_wasm.js";

const WASM_URL = new URL("../pkg/tpt_dsp_wasm_bg.wasm", import.meta.url);
const WORKLET_URL = new URL("./pedal-processor.js", import.meta.url);

const startButton = document.getElementById("start");
const stopButton = document.getElementById("stop");
const statusText = document.getElementById("status");

let context = null;
let node = null;
let source = null;
let stream = null;

function setStatus(text) {
  statusText.textContent = text;
}

function sendParam(name, value) {
  if (node) {
    node.port.postMessage({ name, value: Number(value) });
  }
}

function wireControls() {
  for (const control of document.querySelectorAll("[data-param]")) {
    const readout = control.parentElement.querySelector("output");
    const update = () => {
      if (readout) {
        readout.textContent = control.value;
      }
      sendParam(control.dataset.param, control.value);
    };
    control.addEventListener("input", update);
    update();
  }
}

async function start() {
  startButton.disabled = true;
  setStatus("loading wasm…");

  // Main-thread instance: only used for the web-sys helpers below.
  await init({ module_or_path: WASM_URL });

  context = new AudioContext({ latencyHint: "interactive" });
  setStatus("loading worklet…");
  await register_worklet(context, WORKLET_URL.href);

  // The worklet lives in its own realm and needs its own instance of the
  // module, so hand it a compiled WebAssembly.Module it can instantiate
  // synchronously in its constructor.
  const wasmModule = await WebAssembly.compileStreaming(fetch(WASM_URL));
  node = create_pedal_node(context, { wasmModule });
  node.onprocessorerror = (event) => setStatus(`worklet error: ${event}`);

  setStatus("requesting microphone…");
  stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      echoCancellation: false,
      noiseSuppression: false,
      autoGainControl: false,
    },
    video: false,
  });

  source = connect_stream(context, stream, node);
  await context.resume();

  wireControls();
  stopButton.disabled = false;
  setStatus(`running @ ${context.sampleRate} Hz, 128-sample blocks`);
}

async function stop() {
  stopButton.disabled = true;
  if (source) {
    source.disconnect();
  }
  if (node) {
    node.disconnect();
  }
  if (stream) {
    for (const track of stream.getTracks()) {
      track.stop();
    }
  }
  if (context) {
    await context.close();
  }
  source = node = stream = context = null;
  startButton.disabled = false;
  setStatus("stopped");
}

startButton.addEventListener("click", () => {
  start().catch((error) => {
    console.error(error);
    setStatus(`error: ${error.message ?? error}`);
    startButton.disabled = false;
  });
});

stopButton.addEventListener("click", () => {
  stop().catch(console.error);
});
