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

// The AudioWorkletGlobalScope provides neither TextDecoder nor TextEncoder,
// but the wasm-bindgen glue uses them (string returns such as the processor
// name, and panic messages). Polyfill minimal UTF-8 shims *before* the glue
// is imported. Static ESM imports are hoisted above this code, so the glue
// is loaded with a dynamic `import()` after installation instead.
function installTextCodecShims() {
  if (typeof globalThis.TextDecoder !== 'undefined') return;

  class TextDecoderShim {
    decode(view) {
      const bytes =
        view instanceof Uint8Array ? view : new Uint8Array(view.buffer ?? view);
      let out = '';
      let i = 0;
      while (i < bytes.length) {
        const b = bytes[i];
        let cp;
        if (b < 0x80) {
          cp = b;
          i += 1;
        } else if ((b & 0xe0) === 0xc0 && i + 1 < bytes.length) {
          cp = ((b & 0x1f) << 6) | (bytes[i + 1] & 0x3f);
          i += 2;
        } else if ((b & 0xf0) === 0xe0 && i + 2 < bytes.length) {
          cp = ((b & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f);
          i += 3;
        } else if ((b & 0xf8) === 0xf0 && i + 3 < bytes.length) {
          cp =
            ((b & 0x07) << 18) |
            ((bytes[i + 1] & 0x3f) << 12) |
            ((bytes[i + 2] & 0x3f) << 6) |
            (bytes[i + 3] & 0x3f);
          i += 4;
        } else {
          cp = 0xfffd; // replacement character
          i += 1;
        }
        out += String.fromCodePoint(cp >= 0xd800 && cp <= 0xdfff ? 0xfffd : cp);
      }
      return out;
    }
  }

  class TextEncoderShim {
    encode(str) {
      const out = [];
      for (let i = 0; i < str.length; i++) {
        let cp = str.codePointAt(i);
        if (cp > 0xffff) i++; // consume the low surrogate of a pair
        if (cp < 0x80) {
          out.push(cp);
        } else if (cp < 0x800) {
          out.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
          out.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
        } else {
          out.push(
            0xf0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3f),
            0x80 | ((cp >> 6) & 0x3f),
            0x80 | (cp & 0x3f)
          );
        }
      }
      return new Uint8Array(out);
    }
  }

  globalThis.TextDecoder = TextDecoderShim;
  globalThis.TextEncoder = TextEncoderShim;
}

let init;
let processor_name;

// Bootstrap: install the codec shims, then load the wasm-bindgen glue. The
// `await` defers registration until the hoisted class below is initialised.
(async () => {
  installTextCodecShims();
  const glue = await import('./pkg/tpt_dsp_wasm.js');
  init = glue.default;
  processor_name = glue.processor_name;
  registerProcessor(processor_name(), TptPedalboardProcessor);
})().catch((e) => {
  // Without a registered processor the main thread only sees "node name not
  // defined", so surface the real bootstrap failure loudly here.
  console.error('tpt-pedalboard worklet bootstrap failed:', e);
});

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
// Registration happens in the async bootstrap above, after the glue module
// has been imported and `processor_name` is available.
