// SPDX-License-Identifier: MIT OR Apache-2.0
//
// `AudioWorkletGlobalScope` does not expose `TextDecoder` / `TextEncoder`, but
// the wasm-bindgen glue constructs both at module scope, so importing it inside
// a worklet throws before any of our code runs.
//
// This module is imported *before* the glue in `pedal-processor.js`; ES module
// dependencies are evaluated in the order their import declarations appear, so
// these globals exist by the time the glue is evaluated. Nothing on the audio
// hot path uses them — they only matter for strings and panic messages — so a
// compact UTF-8 implementation is enough.

if (typeof globalThis.TextDecoder === "undefined") {
  globalThis.TextDecoder = class TextDecoder {
    constructor(encoding = "utf-8") {
      this.encoding = encoding;
      this.fatal = false;
      this.ignoreBOM = false;
    }

    decode(input) {
      if (!input) {
        return "";
      }
      const bytes =
        input instanceof Uint8Array
          ? input
          : new Uint8Array(input.buffer ?? input, input.byteOffset ?? 0, input.byteLength ?? input.length);

      let out = "";
      let i = 0;
      while (i < bytes.length) {
        const b0 = bytes[i++];
        if (b0 < 0x80) {
          out += String.fromCharCode(b0);
        } else if (b0 < 0xe0) {
          out += String.fromCharCode(((b0 & 0x1f) << 6) | (bytes[i++] & 0x3f));
        } else if (b0 < 0xf0) {
          out += String.fromCharCode(
            ((b0 & 0x0f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f),
          );
        } else {
          out += String.fromCodePoint(
            ((b0 & 0x07) << 18) |
              ((bytes[i++] & 0x3f) << 12) |
              ((bytes[i++] & 0x3f) << 6) |
              (bytes[i++] & 0x3f),
          );
        }
      }
      return out;
    }
  };
}

if (typeof globalThis.TextEncoder === "undefined") {
  globalThis.TextEncoder = class TextEncoder {
    constructor() {
      this.encoding = "utf-8";
    }

    // No `encodeInto`: the wasm-bindgen glue detects its absence and installs
    // its own shim on top of `encode`.
    encode(text) {
      const bytes = [];
      for (const character of text) {
        const cp = character.codePointAt(0);
        if (cp < 0x80) {
          bytes.push(cp);
        } else if (cp < 0x800) {
          bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
          bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3f), 0x80 | (cp & 0x3f));
        } else {
          bytes.push(
            0xf0 | (cp >> 18),
            0x80 | ((cp >> 12) & 0x3f),
            0x80 | ((cp >> 6) & 0x3f),
            0x80 | (cp & 0x3f),
          );
        }
      }
      return new Uint8Array(bytes);
    }
  };
}
