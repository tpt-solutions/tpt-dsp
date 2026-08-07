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

import init, { processor_name } from './pkg/tpt_dsp_wasm.js';

class TptPedalboardProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.board = null;
    this.inputView = null;
    this.outputView = null;

    const module = options.processorOptions && options.processorOptions.wasmModule;
    if (!module) {
      this.port.postMessage({ error: 'no wasmModule passed in processorOptions' });
      return;
    }

    init(module)
      .then((wasm) => {
        this.wasm = wasm;
        this.board = new wasm.Pedalboard();
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
      default: break;
    }
    // `set_eq_gain` may grow wasm memory; rebuild views defensively.
    if (m.type === 'eq') this.refreshViews();
  }

  process(inputs, outputs) {
    const output = outputs[0];
    if (!this.board || !output || output.length === 0) return true;

    const input = inputs[0];
    const inChan = input && input.length > 0 ? input[0] : null;
    if (inChan) {
      this.inputView.set(inChan.subarray(0, 128));
    } else {
      this.inputView.fill(0);
    }

    this.board.process_internal_block();

    for (let c = 0; c < output.length; c++) {
      output[c].set(this.outputView);
    }
    return true;
  }
}

registerProcessor(processor_name(), TptPedalboardProcessor);
