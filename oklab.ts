/**
 * sRGB ↔ Oklab ↔ OKLCh conversions.
 * Matrices verbatim from culori 4.0.2 (src/oklab/convertOklabToLrgb.js,
 * src/oklab/convertLrgbToOklab.js), which match CSS Color 4.
 */

import { linearize, delinearize } from './luminance';

export interface Oklch {
  l: number;
  c: number;
  h: number;
}

export interface Lrgb {
  r: number;
  g: number;
  b: number;
}

// ── Matrices ──────────────────────────────────────────────

/** Oklab (a, b) → offset added to L for each cone response, rows L, M, S. */
export const M_AB: ReadonlyArray<readonly [number, number]> = [
  [0.3963377773761749, 0.2158037573099136],
  [-0.1055613458156586, -0.0638541728258133],
  [-0.0894841775298119, -1.2914855480194092],
];

/** Cubed cone responses (L, M, S) → linear sRGB, rows r, g, b. */
export const LMS2RGB: ReadonlyArray<readonly [number, number, number]> = [
  [4.0767416360759574, -3.3077115392580616, 0.2309699031821044],
  [-1.2684379732850317, 2.6097573492876887, -0.3413193760026573],
  [-0.0041960761386756, -0.7034186179359362, 1.7076146940746117],
];

/** Linear sRGB → cubed cone responses, rows L, M, S. */
const RGB2LMS: ReadonlyArray<readonly [number, number, number]> = [
  [0.412221469470763, 0.5363325372617348, 0.0514459932675022],
  [0.2119034958178252, 0.6806995506452344, 0.1073969535369406],
  [0.0883024591900564, 0.2817188391361215, 0.6299787016738222],
];

/** Cube-rooted cone responses → Oklab, rows l, a, b. */
const LMS2LAB: ReadonlyArray<readonly [number, number, number]> = [
  [0.210454268309314, 0.7936177747023054, -0.0040720430116193],
  [1.9779985324311684, -2.4285922420485799, 0.450593709617411],
  [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];

// ── OKLCh → sRGB ──────────────────────────────────────────

/** OKLCh → linear sRGB. Out-of-gamut colors come back with channels outside [0, 1]. */
export function oklchToLrgb({ l, c, h }: Oklch): Lrgb {
  const rad = (h / 180) * Math.PI;
  const a = c ? c * Math.cos(rad) : 0;
  const b = c ? c * Math.sin(rad) : 0;

  const L = Math.pow(l + M_AB[0][0] * a + M_AB[0][1] * b, 3);
  const M = Math.pow(l + M_AB[1][0] * a + M_AB[1][1] * b, 3);
  const S = Math.pow(l + M_AB[2][0] * a + M_AB[2][1] * b, 3);

  return {
    r: LMS2RGB[0][0] * L + LMS2RGB[0][1] * M + LMS2RGB[0][2] * S,
    g: LMS2RGB[1][0] * L + LMS2RGB[1][1] * M + LMS2RGB[1][2] * S,
    b: LMS2RGB[2][0] * L + LMS2RGB[2][1] * M + LMS2RGB[2][2] * S,
  };
}

/** OKLCh → gamma-encoded sRGB (unclamped). */
export function oklchToRgb(color: Oklch): Lrgb {
  const { r, g, b } = oklchToLrgb(color);
  return { r: delinearize(r), g: delinearize(g), b: delinearize(b) };
}

// ── sRGB → OKLCh ──────────────────────────────────────────

function normalizeHue(hue: number): number {
  hue = hue % 360;
  return hue < 0 ? hue + 360 : hue;
}

/** Gamma-encoded sRGB → OKLCh. Neutral inputs (r = g = b) give c = 0, h = 0 exactly. */
export function rgbToOklch(r: number, g: number, b: number): Oklch {
  const lr = linearize(r);
  const lg = linearize(g);
  const lb = linearize(b);

  const L = Math.cbrt(RGB2LMS[0][0] * lr + RGB2LMS[0][1] * lg + RGB2LMS[0][2] * lb);
  const M = Math.cbrt(RGB2LMS[1][0] * lr + RGB2LMS[1][1] * lg + RGB2LMS[1][2] * lb);
  const S = Math.cbrt(RGB2LMS[2][0] * lr + RGB2LMS[2][1] * lg + RGB2LMS[2][2] * lb);

  const l = LMS2LAB[0][0] * L + LMS2LAB[0][1] * M + LMS2LAB[0][2] * S;
  if (r === g && g === b) return { l, c: 0, h: 0 };

  const a = LMS2LAB[1][0] * L + LMS2LAB[1][1] * M + LMS2LAB[1][2] * S;
  const bb = LMS2LAB[2][0] * L + LMS2LAB[2][1] * M + LMS2LAB[2][2] * S;
  const c = Math.sqrt(a * a + bb * bb);
  return { l, c, h: c ? normalizeHue((Math.atan2(bb, a) * 180) / Math.PI) : 0 };
}
