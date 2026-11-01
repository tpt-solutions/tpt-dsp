// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Minimal UTF-8 TextDecoder / TextEncoder for the AudioWorkletGlobalScope,
// which provides neither. The wasm-bindgen glue uses them at module-evaluation
// time (for example `new TextDecoder(...)` at the top level), so this module
// must be evaluated before the glue. `pedal-processor.js` imports it first.

if (typeof globalThis.TextDecoder === 'undefined') {
  class TextDecoderShim {
    decode(view) {
      // The glue calls `decode()` with no argument at module load to prime it.
      if (view === undefined) return '';
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
