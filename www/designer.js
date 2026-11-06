// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Filter designer page. Builds a FilterSpec in the wasm module and draws its
// magnitude response with a log-frequency axis. Every change re-runs the
// design, so a bad combination shows the reason rather than a blank plot.

import init, { FilterSpec } from './pkg/tpt_dsp_wasm.js';

const POINTS = 240;
const DB_MIN = -100;
const DB_MAX = 5;

const $ = (id) => document.getElementById(id);
const els = {
  family: $('family'), shape: $('shape'), order: $('order'), orderVal: $('order-val'),
  fs: $('fs'), f1: $('f1'), f1Label: $('f1-label'), f2: $('f2'), f2Field: $('f2-field'),
  ripple: $('ripple'), rippleVal: $('ripple-val'), rippleField: $('ripple-field'),
  atten: $('atten'), attenVal: $('atten-val'), attenField: $('atten-field'),
  error: $('error'), plot: $('plot'), meta: $('meta'), coeffBody: $('coeff-body'),
  snippet: $('snippet'), copy: $('copy'),
};

// Which controls each family and shape uses. Chebyshev II reads the attenuation
// as its stopband level; the others read the ripple. Elliptic reads both.
function usesRipple(family) {
  return family === 'chebyshev1' || family === 'elliptic';
}
function usesAtten(family) {
  return family === 'chebyshev2' || family === 'elliptic';
}
function isBand(shape) {
  return shape === 'bandpass' || shape === 'bandstop';
}

function logFrequencies(fs) {
  const lo = Math.log10(20);
  const hi = Math.log10(fs / 2);
  return Float32Array.from({ length: POINTS }, (_, i) =>
    10 ** (lo + ((hi - lo) * i) / (POINTS - 1)),
  );
}

function drawPlot(freqs, db, fs) {
  const canvas = els.plot;
  const ctx = canvas.getContext('2d');
  const { width: w, height: h } = canvas;
  const left = 56, right = 16, top = 16, bottom = 40;
  const plotW = w - left - right, plotH = h - top - bottom;
  const x = (f) =>
    left + (plotW * (Math.log10(f) - Math.log10(20))) / (Math.log10(fs / 2) - Math.log10(20));
  const y = (d) => top + (plotH * (DB_MAX - Math.max(DB_MIN, Math.min(DB_MAX, d)))) / (DB_MAX - DB_MIN);

  ctx.clearRect(0, 0, w, h);
  ctx.font = '22px system-ui, sans-serif';
  ctx.fillStyle = '#9aa0b4';
  ctx.strokeStyle = '#2c2f3e';
  ctx.lineWidth = 1;

  for (let d = DB_MAX; d >= DB_MIN; d -= 20) {
    ctx.beginPath();
    ctx.moveTo(left, y(d));
    ctx.lineTo(w - right, y(d));
    ctx.stroke();
    ctx.fillText(`${d} dB`, 6, y(d) + 7);
  }
  for (const f of [20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]) {
    if (f > fs / 2) continue;
    ctx.beginPath();
    ctx.moveTo(x(f), top);
    ctx.lineTo(x(f), h - bottom);
    ctx.stroke();
    const label = f >= 1000 ? `${f / 1000}k` : `${f}`;
    ctx.fillText(label, x(f) - 12, h - bottom + 26);
  }

  ctx.strokeStyle = '#8ab4ff';
  ctx.lineWidth = 3;
  ctx.beginPath();
  for (let i = 0; i < freqs.length; i++) {
    const px = x(freqs[i]), py = y(db[i]);
    if (i === 0) ctx.moveTo(px, py);
    else ctx.lineTo(px, py);
  }
  ctx.stroke();
}

function fmt(v) {
  return Number(v).toPrecision(9);
}

function renderCoefficients(flat) {
  els.coeffBody.textContent = '';
  const sections = flat.length / 5;
  let snippet = `// ${sections} biquad section(s), a0 = 1, y = b0·x + b1·x₋₁ + b2·x₋₂ − a1·y₋₁ − a2·y₋₂\n`;
  snippet += 'let sections = [\n';
  for (let s = 0; s < sections; s++) {
    const [b0, b1, b2, a1, a2] = flat.slice(s * 5, s * 5 + 5);
    const tr = document.createElement('tr');
    const cells = [s + 1, fmt(b0), fmt(b1), fmt(b2), fmt(a1), fmt(a2)];
    for (const c of cells) {
      const td = document.createElement(tr.childElementCount === 0 ? 'th' : 'td');
      td.textContent = c;
      tr.appendChild(td);
    }
    els.coeffBody.appendChild(tr);
    snippet += `    BiquadCoeffs { b0: ${fmt(b0)}, b1: ${fmt(b1)}, b2: ${fmt(b2)}, a1: ${fmt(a1)}, a2: ${fmt(a2)} },\n`;
  }
  snippet += '];\n';
  els.snippet.value = snippet;
}

function showError(message) {
  els.error.textContent = message;
  els.error.hidden = false;
  els.meta.textContent = '';
  els.coeffBody.textContent = '';
  els.snippet.value = '';
  const ctx = els.plot.getContext('2d');
  ctx.clearRect(0, 0, els.plot.width, els.plot.height);
}

function update() {
  const family = els.family.value;
  const shape = els.shape.value;
  const fs = Number(els.fs.value);
  els.orderVal.textContent = els.order.value;
  els.rippleVal.textContent = Number(els.ripple.value).toFixed(2);
  els.attenVal.textContent = els.atten.value;

  els.f1Label.textContent = isBand(shape) ? 'Lower edge (Hz)' : 'Cutoff (Hz)';
  els.f2Field.hidden = !isBand(shape);
  els.rippleField.hidden = !usesRipple(family);
  els.attenField.hidden = !usesAtten(family);

  const spec = new FilterSpec(family, shape, Number(els.order.value), fs);
  try {
    spec.set_edges(Number(els.f1.value), Number(els.f2.value));
    spec.set_ripple(Number(els.ripple.value), Number(els.atten.value));
    const freqs = logFrequencies(fs);
    const db = spec.response_db(freqs);
    const coeffs = spec.coefficients();
    els.error.hidden = true;
    drawPlot(freqs, db, fs);
    renderCoefficients(coeffs);
    const passDb = db[0];
    const sections = spec.section_count();
    els.meta.textContent =
      `${sections} biquad section${sections === 1 ? '' : 's'} · response at 20 Hz ${passDb.toFixed(1)} dB`;
  } catch (err) {
    showError(String(err));
  } finally {
    spec.free();
  }
}

for (const el of [els.family, els.shape, els.order, els.fs, els.f1, els.f2, els.ripple, els.atten]) {
  el.addEventListener('input', update);
  el.addEventListener('change', update);
}

els.copy.addEventListener('click', async () => {
  try {
    await navigator.clipboard.writeText(els.snippet.value);
    els.copy.textContent = 'Copied';
    setTimeout(() => (els.copy.textContent = 'Copy Rust snippet'), 1200);
  } catch {
    els.snippet.select();
  }
});

await init(new URL('./pkg/tpt_dsp_wasm_bg.wasm', import.meta.url));
update();
