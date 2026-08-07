// SPDX-License-Identifier: MIT OR Apache-2.0
//
// AudioWorkletProcessor: one Pedalboard per node, driven at exactly one
// 128-sample render quantum per `process()` call.
//
// The audio thread does no allocation: the input/output blocks are copied
// straight into the wasm linear-memory buffers that `Pedalboard` pre-allocated
// in its constructor, and `process_internal_block()` is allocation-free on the
// Rust side.

// Order matters: the polyfill module must be evaluated before the glue, which
// touches `TextDecoder`/`TextEncoder` at module scope.
import "./worklet-polyfill.js";
import initSync, { Pedalboard } from "../pkg/tpt_dsp_wasm.js";

const BLOCK = 128;

class PedalboardProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { wasmModule } = options.processorOptions ?? {};
    if (!wasmModule) {
      throw new Error("processorOptions.wasmModule is required");
    }
    this.wasm = initSync({ module: wasmModule });
    this.pedal = Pedalboard.with_sample_rate(sampleRate);
    this.refreshViews();
    this.port.onmessage = (event) => this.applyParam(event.data);
  }

  // Float32Array views detach if linear memory ever grows; re-create them
  // whenever that happens (it should not, after construction).
  refreshViews() {
    const buffer = this.wasm.memory.buffer;
    this.inputView = new Float32Array(buffer, this.pedal.input_ptr(), BLOCK);
    this.outputView = new Float32Array(buffer, this.pedal.output_ptr(), BLOCK);
  }

  applyParam({ name, value }) {
    switch (name) {
      case "distortion":
        this.pedal.set_distortion(value);
        break;
      case "distortionMix":
        this.pedal.set_distortion_mix(value);
        break;
      case "distortionCurve":
        this.pedal.set_distortion_curve(value);
        break;
      case "delayTime":
        this.pedal.set_delay_time(value);
        break;
      case "delayFeedback":
        this.pedal.set_delay_feedback(value);
        break;
      case "delayMix":
        this.pedal.set_delay_mix(value);
        break;
      case "reverbMix":
        this.pedal.set_reverb_mix(value);
        break;
      case "eq0":
      case "eq1":
      case "eq2":
        // Rebuilds the biquad cascade: control thread only, never in process().
        this.pedal.set_eq_gain(Number(name.slice(2)), value);
        break;
      case "outputGain":
        this.pedal.set_output_gain(value);
        break;
      case "reset":
        this.pedal.reset();
        break;
      default:
        break;
    }
  }

  process(inputs, outputs) {
    const output = outputs[0];
    if (!output || output.length === 0) {
      return true;
    }
    if (this.inputView.length === 0) {
      this.refreshViews();
    }

    const input = inputs[0];
    const inputChannel = input && input.length > 0 ? input[0] : null;

    if (inputChannel && inputChannel.length === BLOCK) {
      this.inputView.set(inputChannel);
    } else {
      this.inputView.fill(0);
      if (inputChannel) {
        this.inputView.set(inputChannel.subarray(0, BLOCK));
      }
    }

    this.pedal.process_internal_block();

    for (const channel of output) {
      channel.set(this.outputView.subarray(0, channel.length));
    }
    return true;
  }
}

registerProcessor("tpt-pedalboard", PedalboardProcessor);
