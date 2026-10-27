// Throughput on the golden grid (29 tones × 17 chromas × 45 hues).
// Run `npm run build` first; compare with `cargo run --release --example bench`.
import { tonalOklchToOklch, tonalOklchToResult } from '../dist/index.js';

const TONES = [
  0.5, 1, 2, 3, 5, 10, 15, 20, 25, 30, 35, 40, 45, 49.83, 49.84,
  50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 97, 98, 99, 99.5,
];

const all = [];
for (const tone of TONES) {
  for (let ci = 0; ci < 17; ci++) {
    for (let hi = 0; hi < 45; hi++) all.push({ tone, chroma: ci / 40, hue: hi * 8 });
  }
}

/** Median wall time of `reps` passes over `colors`, in seconds. */
function time(colors, reps, fn) {
  const runs = [];
  for (let i = 0; i < reps; i++) {
    const start = performance.now();
    for (const c of colors) fn(c);
    runs.push((performance.now() - start) / 1000);
  }
  return runs.sort((a, b) => a - b)[reps >> 1];
}

function report(label, colors, seconds) {
  const per = seconds / colors.length;
  console.log(
    `${label.padEnd(10)} ${String(colors.length).padStart(6)} colors  ${(per * 1e6).toFixed(3).padStart(7)} µs/color  ${(1 / per / 1e6).toFixed(2).padStart(6)} M colors/s`,
  );
}

const mapped = all.filter((c) => tonalOklchToOklch(c).c !== c.chroma);
const inGamut = all.filter((c) => tonalOklchToOklch(c).c === c.chroma);
console.log(
  `Golden grid: ${all.length} colors, ${mapped.length} (${((100 * mapped.length) / all.length).toFixed(0)}%) need gamut mapping. Median of 9 runs.\n`,
);

for (let i = 0; i < 3; i++) time(all, 1, tonalOklchToResult);

console.log('tonalOklchToResult');
report('all', all, time(all, 9, tonalOklchToResult));
report('in-gamut', inGamut, time(inGamut, 9, tonalOklchToResult));
report('mapped', mapped, time(mapped, 9, tonalOklchToResult));

console.log('\ntonalOklchToOklch');
report('all', all, time(all, 9, tonalOklchToOklch));
