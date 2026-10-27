// Parity and timing check for the WASM build against the golden fixture and
// the JS build. Run `npm run test:wasm`, which builds both first.
import { readFileSync } from 'node:fs';

import { rgbToTonalOklch, tonalOklchToOklch, tonalOklchToResult } from '../../../dist/index.js';
import {
  initSync,
  rgbToTonalOklchBatch,
  tonalOklchToHex,
  tonalOklchToOklchBatch,
  tonalOklchToResultBatch,
  tonalOklchToRgb8Batch,
} from '../pkg/tonal_oklch_wasm.js';

initSync({ module: readFileSync(new URL('../pkg/tonal_oklch_wasm_bg.wasm', import.meta.url)) });

let failures = 0;
function check(label, ok, detail = '') {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${label}${detail ? `  (${detail})` : ''}`);
  if (!ok) failures++;
}

const hex = (r, g, b) => '#' + [r, g, b].map((n) => n.toString(16).padStart(2, '0')).join('');

// ── Golden fixture ────────────────────────────────────────

const lines = readFileSync(new URL('../../../tests/fixtures/golden-hex.txt', import.meta.url), 'utf8')
  .trim()
  .split('\n');
const hues = lines[0].match(/\(hues ([\d,]+)\)/)[1].split(',').map(Number);
const colors = [];
const expectedHex = [];
for (const line of lines.slice(1)) {
  const [tone, chroma, ...hexes] = line.split(' ');
  hues.forEach((hue, i) => {
    colors.push({ tone: Number(tone), chroma: Number(chroma), hue });
    expectedHex.push(hexes[i]);
  });
}
const flat = Float64Array.from(colors.flatMap((c) => [c.tone, c.chroma, c.hue]));

const rgb8 = tonalOklchToRgb8Batch(flat);
let goldenMismatches = 0;
let maxChannelDiff = 0;
for (let i = 0; i < colors.length; i++) {
  const got = hex(rgb8[3 * i], rgb8[3 * i + 1], rgb8[3 * i + 2]);
  if (got !== expectedHex[i]) {
    goldenMismatches++;
    for (let ch = 0; ch < 3; ch++) {
      const want = parseInt(expectedHex[i].slice(1 + 2 * ch, 3 + 2 * ch), 16);
      maxChannelDiff = Math.max(maxChannelDiff, Math.abs(want - rgb8[3 * i + ch]));
    }
  }
}
check(
  `golden fixture: ${colors.length} hexes match tonalOklchToRgb8Batch`,
  goldenMismatches === 0,
  goldenMismatches ? `${goldenMismatches} mismatches, max channel diff ${maxChannelDiff}` : '',
);

// ── Full results vs the JS build ──────────────────────────

const results = tonalOklchToResultBatch(flat);
check('tonalOklchToResultBatch stride is 10', results.length === colors.length * 10);

const diffs = { rgb: 0, oklch: 0 };
let resultMismatch = 0;
for (let i = 0; i < colors.length; i++) {
  const js = tonalOklchToResult(colors[i]);
  const w = results.subarray(10 * i, 10 * i + 10);
  diffs.rgb = Math.max(diffs.rgb, Math.abs(w[0] - js.rgb.r), Math.abs(w[1] - js.rgb.g), Math.abs(w[2] - js.rgb.b));
  diffs.oklch = Math.max(diffs.oklch, Math.abs(w[6] - js.oklch.l), Math.abs(w[7] - js.oklch.c), Math.abs(w[8] - js.oklch.h));
  const same =
    w[3] === js.rgb8.r && w[4] === js.rgb8.g && w[5] === js.rgb8.b && w[9] === js.tone && hex(w[3], w[4], w[5]) === js.hex;
  if (!same) resultMismatch++;
}
check('rgb8, hex and tone match the JS build', resultMismatch === 0, resultMismatch ? `${resultMismatch} mismatches` : '');
check('float rgb within 1e-12 of the JS build', diffs.rgb <= 1e-12, `max diff ${diffs.rgb.toExponential(2)}`);
check('oklch within 1e-9 of the JS build', diffs.oklch <= 1e-9, `max diff ${diffs.oklch.toExponential(2)}`);

const oklch = tonalOklchToOklchBatch(flat);
let oklchDiff = 0;
for (let i = 0; i < colors.length; i++) {
  const js = tonalOklchToOklch(colors[i]);
  oklchDiff = Math.max(oklchDiff, Math.abs(oklch[3 * i] - js.l), Math.abs(oklch[3 * i + 1] - js.c), Math.abs(oklch[3 * i + 2] - js.h));
}
check('tonalOklchToOklchBatch within 1e-12 of the JS build', oklchDiff <= 1e-12, `max diff ${oklchDiff.toExponential(2)}`);

// ── Reverse conversion ────────────────────────────────────

const rgbIn = [];
for (let r = 0; r <= 255; r += 17) for (let g = 0; g <= 255; g += 17) for (let b = 0; b <= 255; b += 17) rgbIn.push(r / 255, g / 255, b / 255);
const reverse = rgbToTonalOklchBatch(Float64Array.from(rgbIn));
let reverseDiff = 0;
for (let i = 0; i < rgbIn.length / 3; i++) {
  const js = rgbToTonalOklch(rgbIn[3 * i], rgbIn[3 * i + 1], rgbIn[3 * i + 2]);
  reverseDiff = Math.max(reverseDiff, Math.abs(reverse[3 * i] - js.tone), Math.abs(reverse[3 * i + 1] - js.chroma), Math.abs(reverse[3 * i + 2] - js.hue));
}
check(`rgbToTonalOklchBatch within 1e-9 of the JS build over ${rgbIn.length / 3} colors`, reverseDiff <= 1e-9, `max diff ${reverseDiff.toExponential(2)}`);

// ── Single-color and error paths ──────────────────────────

check('tonalOklchToHex(65, 0.15, 264) is #6e9cfb', tonalOklchToHex(65, 0.15, 264) === '#6e9cfb');
let threw = false;
try {
  tonalOklchToRgb8Batch(new Float64Array([50, 0.1]));
} catch {
  threw = true;
}
check('batch input not a multiple of 3 throws', threw);

// ── Timing ────────────────────────────────────────────────

function median(reps, fn) {
  const runs = [];
  for (let i = 0; i < reps; i++) {
    const t = performance.now();
    fn();
    runs.push(performance.now() - t);
  }
  return runs.sort((a, b) => a - b)[reps >> 1];
}
for (let i = 0; i < 3; i++) tonalOklchToResultBatch(flat);
const wasmMs = median(9, () => tonalOklchToResultBatch(flat));
const jsMs = median(9, () => {
  for (const c of colors) tonalOklchToResult(c);
});
console.log(
  `\n${colors.length} colors, median of 9: wasm batch ${wasmMs.toFixed(2)} ms (${((wasmMs * 1000) / colors.length).toFixed(3)} µs/color), js ${jsMs.toFixed(2)} ms (${((jsMs * 1000) / colors.length).toFixed(3)} µs/color), ${(jsMs / wasmMs).toFixed(1)}× faster`,
);

if (failures) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
