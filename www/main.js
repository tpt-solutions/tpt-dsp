// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Bootstrap for the tpt-dsp web pedalboard. Compiles the wasm module, loads the
// AudioWorklet processor, constructs the pedalboard node and wires the UI
// controls to parameter messages.

const WASM_URL = new URL('./pkg/tpt_dsp_wasm_bg.wasm', import.meta.url);
const WORKLET_URL = new URL('./pedal-processor.js', import.meta.url);

let context = null;
let node = null;
let source = null;
let micStream = null;

const statusEl = document.getElementById('status');
const ledEl = document.getElementById('led');

function setStatus(text, on = false) {
  ledEl.className = on ? 'led on' : 'led';
  statusEl.lastChild.textContent = text;
}

function send(type, value) {
  if (node) node.port.postMessage({ type, value });
}

function sendEq(band, value) {
  if (node) node.port.postMessage({ type: 'eq', band, value });
}

function bindRange(id, fmt, onChange) {
  const el = document.getElementById(id);
  const out = document.getElementById(id + '-val');
  const update = () => {
    const v = parseFloat(el.value);
    if (out) out.textContent = fmt ? fmt(v) : el.value;
    onChange(v);
  };
  el.addEventListener('input', update);
  update();
}

function wireControls() {
  bindRange('distortion', (v) => v.toFixed(1), (v) => send('distortion', v));
  bindRange('distortion_mix', (v) => v.toFixed(2), (v) => send('distortion_mix', v));
  bindRange('delay_time', (v) => v.toFixed(2), (v) => send('delay_time', v));
  bindRange('delay_feedback', (v) => v.toFixed(2), (v) => send('delay_feedback', v));
  bindRange('delay_mix', (v) => v.toFixed(2), (v) => send('delay_mix', v));
  bindRange('reverb_mix', (v) => v.toFixed(2), (v) => send('reverb_mix', v));
  bindRange('eq-0', (v) => v.toFixed(1), (v) => sendEq(0, v));
  bindRange('eq-1', (v) => v.toFixed(1), (v) => sendEq(1, v));
  bindRange('eq-2', (v) => v.toFixed(1), (v) => sendEq(2, v));
  bindRange('output_gain', (v) => v.toFixed(2), (v) => send('output_gain', v));

  const curve = document.getElementById('distortion_curve');
  curve.addEventListener('change', () => send('distortion_curve', parseInt(curve.value, 10)));

  document.getElementById('reset').addEventListener('click', () => {
    if (node) node.port.postMessage({ type: 'reset' });
  });

  document.getElementById('bypass').addEventListener('change', (e) => {
    // A dry signal is approximated by zeroing the wet effects; the chain still
    // runs but the UI communicates intent via a near-transparent setting.
    if (e.target.checked) {
      send('distortion_mix', 0);
      send('delay_mix', 0);
      send('reverb_mix', 0);
    }
  });
}

async function ensureAudio() {
  if (context) return;
  context = new (window.AudioContext || window.webkitAudioContext)();

  const res = await fetch(WASM_URL);
  const bytes = await res.arrayBuffer();
  const module = await WebAssembly.compile(bytes);

  await context.audioWorklet.addModule(WORKLET_URL);
  node = new AudioWorkletNode(context, 'tpt-pedalboard', {
    numberOfInputs: 1,
    numberOfOutputs: 1,
    processorOptions: { wasmModule: module },
  });
  node.port.onmessage = (e) => {
    if (e.data && e.data.error) setStatus('error: ' + e.data.error);
    if (e.data && e.data.ready) setStatus('running', true);
  };
  node.connect(context.destination);
}

async function startMic() {
  await ensureAudio();
  await context.resume();
  const constraints = {
    audio: {
      echoCancellation: false,
      noiseSuppression: false,
      autoGainControl: false,
    },
    video: false,
  };
  micStream = await navigator.mediaDevices.getUserMedia(constraints);
  const mic = context.createMediaStreamSource(micStream);
  mic.connect(node);
  source = mic;
  setStatus('running (microphone)', true);
  toggleButtons(true);
}

async function startDemo() {
  await ensureAudio();
  await context.resume();
  const osc = context.createOscillator();
  osc.type = 'sawtooth';
  osc.frequency.value = 110;
  osc.connect(node);
  osc.start();
  source = osc;
  setStatus('running (demo tone)', true);
  toggleButtons(true);
}

function stop() {
  if (source) {
    try { source.stop(); } catch (_) {}
    try { source.disconnect(); } catch (_) {}
    source = null;
  }
  if (micStream) {
    micStream.getTracks().forEach((t) => t.stop());
    micStream = null;
  }
  setStatus('stopped');
  toggleButtons(false);
}

function toggleButtons(running) {
  document.getElementById('start-mic').disabled = running;
  document.getElementById('start-demo').disabled = running;
  document.getElementById('stop').disabled = !running;
}

document.getElementById('start-mic').addEventListener('click', startMic);
document.getElementById('start-demo').addEventListener('click', startDemo);
document.getElementById('stop').addEventListener('click', stop);

wireControls();
setStatus('idle');
