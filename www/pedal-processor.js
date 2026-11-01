// SPDX-License-Identifier: MIT OR Apache-2.0
//
// AudioWorklet processor for the tpt-dsp web pedalboard.
//
// The wasm module (compiled on the main thread and passed in as
// `processorOptions.wasmModule`) is instantiated here and driven once per
// 128-sample render quantum. The DSP crate guarantees that
// `Pedalboard::process_internal_block` performs no heap allocation, so the
// audio callback stays glitch-free.
//
// Parameter changes arrive as messages on `this.port` and are forwarded to the
// Rust setters. `set_eq_gain` can allocate (it rebuilds the biquad cascade), so
// we rebuild our linear-memory views after any message to stay safe against
// wasm memory growth.
//
// Module evaluation order matters here: static imports are evaluated in the
// order they are listed, so the TextDecoder/TextEncoder shim is installed
// before the wasm-bindgen glue runs. Dynamic `import()` is not allowed on the
// AudioWorkletGlobalScope, which is why the glue is imported statically.
import './text-codec-shim.js';
import init, { Pedalboard } from './pkg/tpt_dsp_wasm.js';

class TptPedalboardProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.board = null;
    this.inputView = null;
    this.outputView = null;
    this.peak = 0;
    this.blockCount = 0;
    this.bypass = false;

    const module = options.processorOptions && options.processorOptions.wasmModule;
    if (!module) {
      this.port.postMessage({ error: 'no wasmModule passed in processorOptions' });
      return;
    }

    init(module)
      .then((wasm) => {
        this.wasm = wasm;
        // `init` resolves to the raw wasm exports; the `Pedalboard` class lives in the glue.
        this.board = new Pedalboard();
        this.refreshViews();
        this.port.postMessage({ ready: true });
      })
      .catch((e) => this.port.postMessage({ error: String(e) }));

    this.port.onmessage = (ev) => this.onMessage(ev.data);
  }

  refreshViews() {
    const mem = this.wasm.memory.buffer;
    this.inputView = new Float32Array(mem, this.board.input_ptr(), 128);
    this.outputView = new Float32Array(mem, this.board.output_ptr(), 128);
  }

  onMessage(m) {
    const b = this.board;
    if (!b) return;
    switch (m.type) {
      case 'distortion': b.set_distortion(m.value); break;
      case 'distortion_mix': b.set_distortion_mix(m.value); break;
      case 'distortion_curve': b.set_distortion_curve(m.value); break;
      case 'delay_time': b.set_delay_time(m.value); break;
      case 'delay_feedback': b.set_delay_feedback(m.value); break;
      case 'delay_mix': b.set_delay_mix(m.value); break;
      case 'reverb_mix': b.set_reverb_mix(m.value); break;
      case 'eq': b.set_eq_gain(m.band, m.value); break;
      case 'output_gain': b.set_output_gain(m.value); break;
      case 'reset': b.reset(); break;
      case 'bypass': this.bypass = !!m.value; break;
      default: break;
    }
    // `set_eq_gain` may grow wasm memory; rebuild views defensively.
    if (m.type === 'eq') this.refreshViews();
  }

  // Track the largest absolute sample since the last report.
  trackPeak(samples) {
    for (let i = 0; i < samples.length; i++) {
      const a = Math.abs(samples[i]);
      if (a > this.peak) this.peak = a;
    }
  }

  process(inputs, outputs) {
    const output = outputs[0];
    if (!this.board || !output || output.length === 0) return true;

    const input = inputs[0];
    const inChan = input && input.length > 0 ? input[0] : null;

    // The wet chain keeps running while bypassed, so its delay and reverb
    // state stays current; bypass only chooses which signal reaches the output.
    if (inChan) {
      this.inputView.set(inChan.subarray(0, 128));
    } else {
      this.inputView.fill(0);
    }

    this.board.process_internal_block();

    for (let c = 0; c < output.length; c++) {
      if (!this.bypass) output[c].set(this.outputView);
      else if (inChan) output[c].set(inChan.subarray(0, 128));
      else output[c].fill(0);
    }

    this.trackPeak(output[0]);
    this.reportLevel();
    return true;
  }

  // Report the peak output level about every 16 blocks (~40 ms at 48 kHz) so
  // the page can show that audio is actually flowing through the chain.
  reportLevel() {
    if (++this.blockCount >= 16) {
      this.port.postMessage({ level: this.peak });
      this.peak = 0;
      this.blockCount = 0;
    }
  }
}

// Must match `PROCESSOR_NAME` in tpt-dsp-wasm/src/web.rs and the name in main.js.
// Not `processor_name()`: that is a wasm export, and `wasm` is only set inside
// `init()`, so calling it here at module evaluation throws and nothing registers.
registerProcessor('tpt-pedalboard', TptPedalboardProcessor);
