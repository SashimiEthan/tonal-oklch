/**
 * Generates a self-contained HTML visualization of the tonal-oklch library.
 * Run: npm run viz   (builds first, then generates viz.html)
 * Open: viz.html in any browser
 */

import { tonalOklchToResult, tonalOklchToOklch, wcagLuminance, wcagContrast, cieLstarToY } from '../dist/index.js';

const TONES = [10, 20, 30, 40, 49.85, 50, 60, 70, 80, 90, 95];
const HUES = Array.from({ length: 24 }, (_, i) => i * 15); // 0–345 in 15° steps
const CHROMA = 0.15;

// Pre-compute all colors (indexed as grid[hueIdx][toneIdx])
const grid = [];
for (const hue of HUES) {
  const row = [];
  for (const tone of TONES) {
    const targetY = cieLstarToY(tone);
    const result = tonalOklchToResult({ tone, chroma: CHROMA, hue });
    const { r, g, b } = result.rgb8;
    const actualY = wcagLuminance(r / 255, g / 255, b / 255);
    const contrastVsWhite = wcagContrast(actualY, 1);
    const contrastVsBlack = wcagContrast(actualY, 0);
    const yError = Math.abs(actualY - targetY);
    row.push({
      hex: result.hex,
      tone, hue,
      oklchL: result.oklch.l.toFixed(4),
      oklchC: result.oklch.c.toFixed(4),
      contrastVsWhite: contrastVsWhite.toFixed(2),
      contrastVsBlack: contrastVsBlack.toFixed(2),
      yError: yError.toFixed(6),
      actualY: actualY.toFixed(6),
      targetY: targetY.toFixed(6),
    });
  }
  grid.push(row);
}

// Compute contrast spread per tone (max - min contrast vs white across hues)
const spreads = TONES.map((tone, ti) => {
  const ratios = grid.map(hueRow => parseFloat(hueRow[ti].contrastVsWhite));
  const min = Math.min(...ratios);
  const max = Math.max(...ratios);
  return { tone, min: min.toFixed(2), max: max.toFixed(2), spread: (max - min).toFixed(3) };
});

// Gamut mapping demo: show chroma reduction for high-chroma input
const gamutDemo = [];
for (const hue of [0, 60, 120, 180, 240, 300]) {
  const low = tonalOklchToResult({ tone: 50, chroma: 0.05, hue });
  const mid = tonalOklchToResult({ tone: 50, chroma: 0.15, hue });
  const high = tonalOklchToResult({ tone: 50, chroma: 0.35, hue });
  gamutDemo.push({
    hue,
    low: { hex: low.hex, c: low.oklch.c.toFixed(4) },
    mid: { hex: mid.hex, c: mid.oklch.c.toFixed(4) },
    high: { hex: high.hex, c: high.oklch.c.toFixed(4) },
  });
}

// Grayscale ramp: tone 100 → 0 in steps of 2
const GRAY_TONES = Array.from({ length: 51 }, (_, i) => 100 - i * 2); // 100, 98, ..., 0
const grayscale = [];
for (const tone of GRAY_TONES) {
  const result = tonalOklchToResult({ tone, chroma: 0, hue: 0 });
  const { r, g, b } = result.rgb8;
  const actualY = wcagLuminance(r / 255, g / 255, b / 255);
  const targetY = cieLstarToY(tone);
  grayscale.push({
    tone,
    hex: result.hex,
    r, g, b,
    oklchL: result.oklch.l.toFixed(4),
    contrastVsWhite: wcagContrast(actualY, 1).toFixed(2),
    contrastVsBlack: wcagContrast(actualY, 0).toFixed(2),
    actualY: actualY.toFixed(6),
    targetY: targetY.toFixed(6),
    yError: Math.abs(actualY - targetY).toFixed(6),
  });
}

// Tinted neutrals: hue 0–345 (step 15) × tone 100→0 (step 2), chroma=0.01
const TINTED_HUES = Array.from({ length: 24 }, (_, i) => i * 15);
const tintedNeutrals = [];
for (const hue of TINTED_HUES) {
  const row = [];
  for (const tone of GRAY_TONES) {
    const result = tonalOklchToResult({ tone, chroma: 0.01, hue });
    const { r, g, b } = result.rgb8;
    const actualY = wcagLuminance(r / 255, g / 255, b / 255);
    const targetY = cieLstarToY(tone);
    row.push({
      tone, hue,
      hex: result.hex,
      r, g, b,
      oklchL: result.oklch.l.toFixed(4),
      oklchC: result.oklch.c.toFixed(4),
      contrastVsWhite: wcagContrast(actualY, 1).toFixed(2),
      contrastVsBlack: wcagContrast(actualY, 0).toFixed(2),
      actualY: actualY.toFixed(6),
      targetY: targetY.toFixed(6),
      yError: Math.abs(actualY - targetY).toFixed(6),
    });
  }
  tintedNeutrals.push(row);
}

// Reference hue grids: for each reference hue, use its max chroma per tone across all hues
function computeCell(tone, hue, chroma) {
  const targetY = cieLstarToY(tone);
  const result = tonalOklchToResult({ tone, chroma, hue });
  const { r, g, b } = result.rgb8;
  const actualY = wcagLuminance(r / 255, g / 255, b / 255);
  return {
    hex: result.hex, tone, hue,
    oklchL: result.oklch.l.toFixed(4),
    oklchC: result.oklch.c.toFixed(4),
    contrastVsWhite: wcagContrast(actualY, 1).toFixed(2),
    contrastVsBlack: wcagContrast(actualY, 0).toFixed(2),
    yError: Math.abs(actualY - targetY).toFixed(6),
    actualY: actualY.toFixed(6),
    targetY: targetY.toFixed(6),
  };
}

// For each reference hue, find max chroma per tone, then compute all cells
const refHueGrids = {};
for (const refHue of HUES) {
  // Max chroma per tone at the reference hue (request high chroma, get gamut-mapped result)
  const maxChromas = TONES.map(tone => tonalOklchToOklch({ tone, chroma: 0.4, hue: refHue }).c);
  const hueGrid = [];
  for (const hue of HUES) {
    const row = TONES.map((tone, ti) => computeCell(tone, hue, maxChromas[ti]));
    hueGrid.push(row);
  }
  refHueGrids[refHue] = { grid: hueGrid, maxChromas };
}

const DATA = JSON.stringify({ grid, spreads, gamutDemo, grayscale, tintedNeutrals, refHueGrids, GRAY_TONES, TINTED_HUES, TONES, HUES, CHROMA });

const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Tonal OKLCh — Visualization</title>
<style>
  :root { --bg: #1a1a2e; --fg: #e0e0e0; --muted: #888; --card: #16213e; }
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: 'SF Mono', 'Fira Code', 'Consolas', monospace; background: var(--bg); color: var(--fg); padding: 2rem; }
  h1 { font-size: 1.5rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.1rem; margin: 2rem 0 0.75rem; color: #7ec8e3; }
  .subtitle { color: var(--muted); font-size: 0.85rem; margin-bottom: 2rem; }

  /* Tone × Hue grid */
  .grid-container { overflow-x: auto; }
  .color-grid { border-collapse: collapse; }
  .color-grid th { padding: 4px 6px; font-size: 0.7rem; color: var(--muted); font-weight: normal; }
  .color-grid td { padding: 0; }
  .swatch {
    width: 68px; height: 44px; display: flex; align-items: center; justify-content: center;
    font-size: 0.55rem; cursor: pointer; position: relative; transition: transform 0.1s;
  }
  .swatch:hover { transform: scale(1.5); z-index: 10; border-radius: 4px; box-shadow: 0 4px 20px rgba(0,0,0,0.5); }
  .swatch .contrast-label { text-shadow: 0 1px 2px rgba(0,0,0,0.6); pointer-events: none; }
  .swatch .label { opacity: 0; transition: opacity 0.1s; pointer-events: none; text-shadow: 0 1px 2px rgba(0,0,0,0.8); position: absolute; bottom: 2px; font-size: 0.45rem; }
  .swatch:hover .label { opacity: 1; }

  /* Spread table */
  .spread-table { border-collapse: collapse; font-size: 0.8rem; }
  .spread-table th, .spread-table td { padding: 6px 12px; text-align: right; }
  .spread-table th { color: var(--muted); font-weight: normal; border-bottom: 1px solid #333; }
  .spread-table td { border-bottom: 1px solid #222; }
  .spread-good { color: #4caf50; }
  .spread-ok { color: #ffb74d; }
  .spread-bad { color: #ef5350; }

  /* Gamut demo */
  .gamut-row { display: flex; gap: 1.5rem; flex-wrap: wrap; margin-bottom: 1rem; }
  .gamut-group { display: flex; flex-direction: column; align-items: center; gap: 4px; }
  .gamut-swatch { width: 60px; height: 40px; border-radius: 4px; display: flex; align-items: center; justify-content: center; font-size: 0.55rem; }
  .gamut-label { font-size: 0.65rem; color: var(--muted); }

  /* Grayscale ramp */
  .gray-ramp { display: flex; flex-wrap: nowrap; gap: 0; }
  .gray-swatch {
    flex: 1; height: 52px; display: flex; flex-direction: column; align-items: center; justify-content: center;
    font-size: 0.5rem; line-height: 1.4; border-radius: 3px; cursor: pointer; position: relative; transition: transform 0.1s;
  }
  .gray-swatch:hover { transform: scale(1.6); z-index: 10; box-shadow: 0 4px 20px rgba(0,0,0,0.5); }
  .gray-swatch .g-label { opacity: 0; transition: opacity 0.1s; pointer-events: none; text-shadow: 0 1px 2px rgba(0,0,0,0.8); }
  .gray-swatch:hover .g-label { opacity: 1; }

  /* Tinted neutrals grid */
  .tinted-grid { display: flex; flex-direction: column; gap: 0; }
  .tinted-row { display: flex; align-items: center; gap: 0; }
  .tinted-row-label { width: 40px; flex-shrink: 0; font-size: 0.65rem; color: var(--muted); text-align: right; padding-right: 8px; }
  .tinted-cells { display: flex; flex: 1; gap: 0; }
  .tinted-swatch {
    flex: 1; height: 20px; cursor: pointer; transition: transform 0.1s;
  }
  .tinted-swatch:hover { transform: scaleY(2.5); z-index: 10; box-shadow: 0 2px 12px rgba(0,0,0,0.5); }

  /* Selected swatch */
  .swatch-selected { transform: scale(1.8) !important; z-index: 20; border-radius: 4px; box-shadow: 0 0 0 2px #7ec8e3, 0 4px 20px rgba(0,0,0,0.5) !important; }
  .gray-swatch.swatch-selected { transform: scale(1.6) !important; z-index: 20; box-shadow: 0 0 0 2px #7ec8e3, 0 4px 20px rgba(0,0,0,0.5) !important; }
  .gray-swatch.swatch-selected .g-label { opacity: 1; }
  .tinted-swatch.swatch-selected { transform: scaleY(2.5) !important; z-index: 20; box-shadow: 0 0 0 2px #7ec8e3, 0 2px 12px rgba(0,0,0,0.5) !important; }

  /* Copied toast */
  .toast {
    position: fixed; bottom: 2rem; left: 50%; transform: translateX(-50%);
    background: #7ec8e3; color: #0a0e27; padding: 8px 20px; border-radius: 6px;
    font-size: 0.8rem; font-weight: bold; opacity: 0; transition: opacity 0.2s;
    pointer-events: none; z-index: 200;
  }
  .toast.show { opacity: 1; }

  /* Tooltip */
  .tooltip {
    display: none; position: fixed; background: #0a0e27; border: 1px solid #334; border-radius: 8px;
    padding: 12px 16px; font-size: 0.75rem; line-height: 1.6; z-index: 100; pointer-events: none;
    box-shadow: 0 8px 30px rgba(0,0,0,0.6);
  }
  .tooltip .hex-big { font-size: 1.1rem; font-weight: bold; margin-bottom: 4px; }
  .tooltip .row { display: flex; justify-content: space-between; gap: 1.5rem; }
  .tooltip .key { color: var(--muted); }

  /* Sections */
  section { margin-bottom: 3rem; }
</style>
</head>
<body>

<h1>Tonal OKLCh</h1>
<p class="subtitle">Visualization · colors at the same tone should share identical WCAG contrast regardless of hue</p>

<section>
  <h2>Grayscale Ramp (Tone 100 → 0, step 2)</h2>
  <p class="subtitle" style="margin-bottom:0.75rem">Neutral colors (chroma 0). Hover for tone, hex, RGB, contrast, and Y error.</p>
  <div class="gray-ramp" id="grayscale"></div>
</section>

<section>
  <h2>Tinted Neutrals (Chroma 0.01)</h2>
  <p class="subtitle" style="margin-bottom:0.75rem">Near-neutral colors across all hues. Tone 100 → 0 (left to right), hue 0°–345° (top to bottom).</p>
  <div class="tinted-grid" id="tinted"></div>
</section>

<section>
  <h2>Tone × Hue Grid (Chroma ${CHROMA})</h2>
  <div style="margin-bottom:0.75rem;display:flex;align-items:center;gap:1rem">
    <label style="font-size:0.8rem;color:var(--muted)">Reference hue:
      <select id="refHueSelect" style="background:#16213e;color:#e0e0e0;border:1px solid #334;border-radius:4px;padding:4px 8px;font-family:inherit;font-size:0.8rem">
        <option value="none" selected>None (fixed C=${CHROMA})</option>
      </select>
    </label>
    <span id="refHueInfo" style="font-size:0.75rem;color:var(--muted)"></span>
  </div>
  <div class="grid-container">
    <table class="color-grid" id="grid"></table>
  </div>
</section>

<section>
  <h2>Contrast Spread per Tone (vs White)</h2>
  <p class="subtitle" style="margin-bottom:0.75rem">Max − Min contrast ratio across all hues at same tone. Lower = better tone preservation.</p>
  <table class="spread-table" id="spread"></table>
</section>

<section>
  <h2>Gamut Mapping (Tone 50)</h2>
  <p class="subtitle" style="margin-bottom:0.75rem">Requested chroma vs actual chroma after gamut mapping. High-chroma requests get reduced to fit sRGB.</p>
  <div id="gamut"></div>
</section>

<div class="tooltip" id="tip"></div>
<div class="toast" id="toast">Copied!</div>

<script>
const DATA = ${DATA};
const { grid, spreads, gamutDemo, grayscale, tintedNeutrals, refHueGrids, GRAY_TONES, TINTED_HUES, TONES, HUES } = DATA;

// Active grid data (switches when reference hue changes)
let activeGrid = grid;

// Selection & copy
const toast = document.getElementById('toast');
let toastTimer;
function showToast(hex) {
  toast.textContent = 'Copied ' + hex;
  toast.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove('show'), 1200);
}
function clearSelection() {
  const prev = document.querySelector('.swatch-selected');
  if (prev) prev.classList.remove('swatch-selected');
  tip.style.display = 'none';
}
function selectSwatch(el, hex, tipHtml, e) {
  clearSelection();
  el.classList.add('swatch-selected');
  navigator.clipboard.writeText(hex);
  showToast(hex);
  // Pin tooltip near the click
  tip.innerHTML = tipHtml;
  tip.style.display = 'block';
  tip.style.left = (e.clientX + 16) + 'px';
  tip.style.top = (e.clientY + 16) + 'px';
}
// Click outside any swatch to deselect
document.addEventListener('click', e => {
  if (!e.target.closest('.swatch, .gray-swatch, .tinted-swatch, .gamut-swatch')) clearSelection();
});

// Shared tooltip builder
function buildTip(c, extra) {
  let h = '<div class="hex-big" style="color:' + c.hex + '">' + c.hex + '</div>';
  if (c.tone !== undefined) h += '<div class="row"><span class="key">Tone</span><span>' + c.tone + '</span></div>';
  if (c.hue !== undefined) h += '<div class="row"><span class="key">Hue</span><span>' + c.hue + '°</span></div>';
  if (c.r !== undefined) h += '<div class="row"><span class="key">RGB</span><span>' + c.r + ', ' + c.g + ', ' + c.b + '</span></div>';
  if (c.oklchL) h += '<div class="row"><span class="key">OKLCh L</span><span>' + c.oklchL + '</span></div>';
  if (c.oklchC) h += '<div class="row"><span class="key">OKLCh C</span><span>' + c.oklchC + '</span></div>';
  if (c.contrastVsBlack) h += '<div class="row"><span class="key">Contrast vs ■</span><span>' + c.contrastVsBlack + ':1</span></div>';
  if (c.contrastVsWhite) h += '<div class="row"><span class="key">Contrast vs □</span><span>' + c.contrastVsWhite + ':1</span></div>';
  if (c.targetY) h += '<div class="row"><span class="key">Target Y</span><span>' + c.targetY + '</span></div>';
  if (c.actualY) h += '<div class="row"><span class="key">Actual Y</span><span>' + c.actualY + '</span></div>';
  if (c.yError) h += '<div class="row"><span class="key">Y error</span><span>' + c.yError + '</span></div>';
  return h;
}

// Render grid (hues vertical, tones horizontal)
const table = document.getElementById('grid');
function renderGrid() {
  let html = '<tr><th>Hue \\\\ Tone</th>';
  TONES.forEach(t => html += '<th>' + t + '</th>');
  html += '</tr>';
  activeGrid.forEach((row, hi) => {
    html += '<tr><th>' + HUES[hi] + '°</th>';
    row.forEach((cell, ti) => {
      const textColor = parseFloat(cell.contrastVsWhite) > 3 ? '#fff' : '#000';
      const contrast = parseFloat(cell.contrastVsWhite) > 3 ? cell.contrastVsWhite : cell.contrastVsBlack;
      html += '<td><div class="swatch" style="background:' + cell.hex + ';color:' + textColor + '"' +
        ' data-i="' + hi + '-' + ti + '"><span class="contrast-label">' + contrast + '</span><span class="label">' + cell.hex + '</span></div></td>';
    });
    html += '</tr>';
  });
  table.innerHTML = html;
}
renderGrid();

// Reference hue selector
const refSelect = document.getElementById('refHueSelect');
const refInfo = document.getElementById('refHueInfo');
HUES.forEach(h => {
  const opt = document.createElement('option');
  opt.value = h;
  opt.textContent = h + '°';
  refSelect.appendChild(opt);
});
refSelect.addEventListener('change', () => {
  const val = refSelect.value;
  if (val === 'none') {
    activeGrid = grid;
    refInfo.textContent = '';
  } else {
    const ref = refHueGrids[val];
    activeGrid = ref.grid;
    refInfo.textContent = 'Max chroma per tone: ' + ref.maxChromas.map(c => c.toFixed(3)).join(', ');
  }
  renderGrid();
});

// Grid tooltip + click
const tip = document.getElementById('tip');
function showGridTip(swatch) {
  const [hi, ti] = swatch.dataset.i.split('-').map(Number);
  const c = activeGrid[hi][ti];
  return { c, html: buildTip(c) };
}
table.addEventListener('mouseover', e => {
  const swatch = e.target.closest('.swatch');
  if (!swatch || swatch.classList.contains('swatch-selected')) return;
  const { html } = showGridTip(swatch);
  tip.innerHTML = html;
  tip.style.display = 'block';
});
table.addEventListener('click', e => {
  const swatch = e.target.closest('.swatch');
  if (!swatch) return;
  const { c, html } = showGridTip(swatch);
  selectSwatch(swatch, c.hex, html, e);
});
document.addEventListener('mousemove', e => {
  if (tip.style.display === 'block' && !document.querySelector('.swatch-selected')) {
    tip.style.left = (e.clientX + 16) + 'px';
    tip.style.top = (e.clientY + 16) + 'px';
  }
});
table.addEventListener('mouseleave', () => {
  if (!document.querySelector('.swatch-selected')) tip.style.display = 'none';
});

// Spread table
const spreadEl = document.getElementById('spread');
spreadEl.innerHTML = '<tr><th>Tone</th><th>Min</th><th>Max</th><th>Spread</th></tr>';
spreads.forEach(s => {
  const cls = parseFloat(s.spread) < 0.05 ? 'spread-good' : parseFloat(s.spread) < 0.1 ? 'spread-ok' : 'spread-bad';
  spreadEl.innerHTML += '<tr><td>' + s.tone + '</td><td>' + s.min + '</td><td>' + s.max + '</td><td class="' + cls + '">' + s.spread + '</td></tr>';
});

// Grayscale ramp
const grayEl = document.getElementById('grayscale');
grayscale.forEach((g, i) => {
  const textColor = g.tone > 50 ? '#000' : '#fff';
  const div = document.createElement('div');
  div.className = 'gray-swatch';
  div.style.background = g.hex;
  div.style.color = textColor;
  div.dataset.gi = i;
  div.innerHTML = '<span class="g-label">' + g.tone + '</span>';
  grayEl.appendChild(div);
});

// Grayscale tooltip + click
grayEl.addEventListener('mouseover', e => {
  const swatch = e.target.closest('.gray-swatch');
  if (!swatch || swatch.classList.contains('swatch-selected')) return;
  const g = grayscale[parseInt(swatch.dataset.gi)];
  tip.innerHTML = buildTip(g);
  tip.style.display = 'block';
});
grayEl.addEventListener('click', e => {
  const swatch = e.target.closest('.gray-swatch');
  if (!swatch) return;
  const g = grayscale[parseInt(swatch.dataset.gi)];
  selectSwatch(swatch, g.hex, buildTip(g), e);
});
grayEl.addEventListener('mouseleave', () => {
  if (!document.querySelector('.swatch-selected')) tip.style.display = 'none';
});

// Tinted neutrals
const tintedEl = document.getElementById('tinted');
tintedNeutrals.forEach((row, ri) => {
  const rowDiv = document.createElement('div');
  rowDiv.className = 'tinted-row';
  rowDiv.innerHTML = '<div class="tinted-row-label">' + TINTED_HUES[ri] + '°</div><div class="tinted-cells" data-ri="' + ri + '"></div>';
  const cellsDiv = rowDiv.querySelector('.tinted-cells');
  row.forEach((cell, ci) => {
    const div = document.createElement('div');
    div.className = 'tinted-swatch';
    div.style.background = cell.hex;
    div.dataset.ti = ri + '-' + ci;
    cellsDiv.appendChild(div);
  });
  tintedEl.appendChild(rowDiv);
});

// Tinted neutrals tooltip + click
tintedEl.addEventListener('mouseover', e => {
  const swatch = e.target.closest('.tinted-swatch');
  if (!swatch || swatch.classList.contains('swatch-selected')) return;
  const [ri, ci] = swatch.dataset.ti.split('-').map(Number);
  const c = tintedNeutrals[ri][ci];
  tip.innerHTML = buildTip(c);
  tip.style.display = 'block';
});
tintedEl.addEventListener('click', e => {
  const swatch = e.target.closest('.tinted-swatch');
  if (!swatch) return;
  const [ri, ci] = swatch.dataset.ti.split('-').map(Number);
  const c = tintedNeutrals[ri][ci];
  selectSwatch(swatch, c.hex, buildTip(c), e);
});
tintedEl.addEventListener('mouseleave', () => {
  if (!document.querySelector('.swatch-selected')) tip.style.display = 'none';
});

// Gamut demo
const gamutEl = document.getElementById('gamut');
gamutDemo.forEach(g => {
  const div = document.createElement('div');
  div.className = 'gamut-row';
  div.innerHTML = '<strong style="width:50px;align-self:center">' + g.hue + '°</strong>';
  [['C=0.05', g.low], ['C=0.15', g.mid], ['C=0.35', g.high]].forEach(([label, data]) => {
    const textColor = g.hue > 180 ? '#fff' : '#000';
    div.innerHTML += '<div class="gamut-group"><div class="gamut-swatch" style="background:' + data.hex + ';color:' + textColor + '">' + data.hex +
      '</div><div class="gamut-label">' + label + ' → ' + data.c + '</div></div>';
  });
  gamutEl.appendChild(div);
});
</script>
</body>
</html>`;

import { writeFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const outPath = resolve(__dirname, '..', 'viz.html');
writeFileSync(outPath, html);
console.log(`✅ Wrote ${outPath}`);
console.log('   Open in browser to inspect.');
