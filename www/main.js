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

// Peak output level from the worklet, in linear units (1.0 = full scale).
function showLevel(peak) {
  const meter = document.getElementById('meter');
  const val = document.getElementById('meter-val');
  meter.value = Math.min(peak, 1);
  val.textContent = peak < 1e-4 ? 'silent' : peak.toFixed(3) + ' peak';
}

function send(type, value) {
  if (node) node.port.postMessage({ type, value });
}

function sendEq(band, value) {
  if (node) node.port.postMessage({ type: 'eq', band, value });
}

// Each control registers a function that pushes its current value to the
// worklet. They are replayed once the processor is ready, because anything
// sent before then (or while no node exists) would otherwise be dropped.
const controlSyncs = [];

function syncControls() {
  controlSyncs.forEach((sync) => sync());
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
  controlSyncs.push(() => onChange(parseFloat(el.value)));
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
  const sendCurve = () => send('distortion_curve', parseInt(curve.value, 10));
  curve.addEventListener('change', sendCurve);
  controlSyncs.push(sendCurve);

  document.getElementById('reset').addEventListener('click', () => {
    if (node) node.port.postMessage({ type: 'reset' });
  });

  // The worklet passes the dry input straight through while bypass is on.
  const bypass = document.getElementById('bypass');
  const sendBypass = () => send('bypass', bypass.checked);
  bypass.addEventListener('change', sendBypass);
  controlSyncs.push(sendBypass);
}

// Shared by every start button, so a second click while the first is still
// loading waits for the same setup instead of building another context.
let audioReady = null;

function ensureAudio() {
  if (!audioReady) {
    audioReady = buildAudio().catch((e) => {
      audioReady = null; // allow the next click to retry
      throw e;
    });
  }
  return audioReady;
}

async function buildAudio() {
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  try {
    const res = await fetch(WASM_URL);
    const bytes = await res.arrayBuffer();
    const module = await WebAssembly.compile(bytes);

    await ctx.audioWorklet.addModule(WORKLET_URL);
    const workletNode = new AudioWorkletNode(ctx, 'tpt-pedalboard', {
      numberOfInputs: 1,
      numberOfOutputs: 1,
      processorOptions: { wasmModule: module },
    });
    // Resolve only once the processor has instantiated the wasm, so callers
    // never report "running" for a processor that failed to start.
    const ready = new Promise((resolve, reject) => {
      workletNode.port.onmessage = (e) => {
        const data = e.data || {};
        if (typeof data.level === 'number') showLevel(data.level);
        else if (data.error) reject(new Error(data.error));
        else if (data.ready) resolve();
      };
    });
    workletNode.connect(ctx.destination);
    node = workletNode;
    context = ctx;
    await ready;
    syncControls();
  } catch (e) {
    // Leave no half-built state behind, so the next click retries cleanly.
    if (context === ctx) context = null;
    node = null;
    ctx.close();
    throw e;
  }
}

async function run(startSource, label) {
  try {
    await startSource();
    toggleButtons(true);
    setStatus(label, true);
  } catch (e) {
    setStatus('error: ' + (e && e.message ? e.message : e));
    toggleButtons(false);
  }
}

function startMic() {
  return run(async () => {
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
  }, 'running (microphone)');
}

function startDemo() {
  return run(async () => {
    await ensureAudio();
    await context.resume();
    const osc = context.createOscillator();
    osc.type = 'sawtooth';
    osc.frequency.value = 110;
    osc.connect(node);
    osc.start();
    source = osc;
  }, 'running (demo tone)');
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
  showLevel(0);
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
