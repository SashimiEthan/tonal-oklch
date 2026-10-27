/**
 * Solvers for the hybrid OKLCh + CIE L* color space.
 *
 * Core idea: CIE Y (≡ WCAG relative luminance) is the immutable constraint.
 * At fixed (C, h), Y is a cubic in OKLCh L, so L is solved in closed form.
 * Gamut mapping reduces chroma while re-solving L at every step,
 * so tone is never "stale" — unlike Chromator's sequential approach.
 */

import { isDisplayable } from './luminance';
import { M_AB, LMS2RGB, oklchToLrgb, chromaHueToAb } from './oklab';

// ── Core solver: find OKLCh L for a target CIE Y ─────────

/** Luminance weight of each cubed cone response: Y = Σ W[i]·LMS[i]. */
const W = [0, 1, 2].map(
  (i) => 0.2126 * LMS2RGB[0][i] + 0.7152 * LMS2RGB[1][i] + 0.0722 * LMS2RGB[2][i],
);

/** Largest real root of t³ + B·t² + C·t + D = 0. */
function largestRealRoot(B: number, C: number, D: number): number {
  const p = C - (B * B) / 3;
  const q = (2 * B ** 3) / 27 - (B * C) / 3 + D;
  const shift = -B / 3;
  const disc = (q * q) / 4 + p ** 3 / 27;

  if (disc > 0) {
    const s = Math.sqrt(disc);
    // Cube-root only the larger-magnitude term and derive the other from it;
    // adding both cube roots cancels and drifts ~1e-7 at light tones.
    const u = Math.cbrt(q < 0 ? -q / 2 + s : -q / 2 - s);
    return u - p / (3 * u) + shift;
  }

  if (p === 0) return Math.cbrt(-q) + shift;

  const r = 2 * Math.sqrt(-p / 3);
  const phi = Math.acos(Math.max(-1, Math.min(1, ((3 * q) / (2 * p)) * Math.sqrt(-3 / p)))) / 3;
  return r * Math.cos(phi) + shift;
}

/**
 * Solve for the OKLCh L that produces a target CIE Y at given chroma
 * and hue. Each cone response is (L + o)³ with o fixed by (C, h), so
 * Y = Σ W·(L + o)³ is a cubic in L. Returns the largest real root,
 * clamped to [0, 1].
 *
 * @param maxIter Ignored. Kept so existing callers keep compiling.
 */
export function findLForTargetY(
  targetY: number,
  chroma: number,
  hue: number,
  maxIter = 25,
): number {
  const [a, b] = chromaHueToAb(chroma, hue);

  let A3 = 0, A2 = 0, A1 = 0, A0 = 0;
  for (let i = 0; i < 3; i++) {
    const o = M_AB[i][0] * a + M_AB[i][1] * b;
    A3 += W[i];
    A2 += 3 * W[i] * o;
    A1 += 3 * W[i] * o * o;
    A0 += W[i] * o ** 3;
  }

  const L = largestRealRoot(A2 / A3, A1 / A3, (A0 - targetY) / A3);
  return Math.min(1, Math.max(0, L));
}

// ── Gamut mapping with tone preservation ──────────────────

/** Linear-RGB tolerance for "fits in sRGB". Also the band `provablyEmpty` reasons about. */
const EPS = 1e-4;
/** Bound on |d²channel/dc²| along constant-tone paths near the gamut edge (measured max 27.7). */
const CURVATURE = 32;
/** Chroma intervals narrower than this are not searched for islands. */
const MIN_WIDTH = 1e-6;

type Sample = { c: number; r: number; g: number; b: number; fits: boolean };
type Bracket = { lo: number; hi: Sample };

/** Linear RGB of the color at chroma c on the constant-Y path, and whether it fits. */
function sample(targetY: number, hue: number, c: number): Sample {
  const L = findLForTargetY(targetY, c, hue);
  const { r, g, b } = oklchToLrgb({ l: L, c, h: hue });
  return { c, r, g, b, fits: isDisplayable(r, g, b, EPS) };
}

/**
 * True when no chroma strictly between a and b (both infeasible) can fit: some
 * channel is outside the band on the same side at both ends by more than the
 * curvature bound lets it bend back over an interval this wide.
 */
function provablyEmpty(a: Sample, b: Sample): boolean {
  const bend = (CURVATURE * (b.c - a.c) ** 2) / 8;
  for (const k of ['r', 'g', 'b'] as const) {
    if (-EPS - a[k] > bend && -EPS - b[k] > bend) return true;
    if (a[k] - 1 - EPS > bend && b[k] - 1 - EPS > bend) return true;
  }
  return false;
}

/** Bracket around the highest feasible chroma strictly inside (a, b), both infeasible, or null. */
function topInside(targetY: number, hue: number, a: Sample, b: Sample, tol: number): Bracket | null {
  // Written so a NaN width also returns null instead of recursing forever.
  if (!(b.c - a.c >= MIN_WIDTH) || provablyEmpty(a, b)) return null;
  const m = sample(targetY, hue, (a.c + b.c) / 2);
  if (m.fits) return topBetween(targetY, hue, m.c, b, tol);
  return topInside(targetY, hue, m, b, tol) ?? topInside(targetY, hue, a, m, tol);
}

/** Narrow [lo, hi) (lo feasible, hi infeasible) to width tol around the highest feasible chroma. */
function topBetween(targetY: number, hue: number, lo: number, hi: Sample, tol: number): Bracket {
  while (hi.c - lo > tol) {
    const m = sample(targetY, hue, (lo + hi.c) / 2);
    if (m.fits) { lo = m.c; continue; }
    const inner = topInside(targetY, hue, m, hi, tol);
    if (inner) return inner;
    hi = m;
  }
  return { lo, hi };
}

/**
 * Find the highest OKLCh chroma at or below maxChroma that fits in sRGB
 * while hitting a target CIE Y exactly. Bisection on chroma, re-solving L
 * at every step, but an infeasible midpoint only discards the interval
 * above it once `provablyEmpty` rules out a hidden feasible island there.
 * Near the blue primary the constant-Y path grazes a gamut face and the
 * feasible set splits in two; plain bisection would land on either half
 * depending on maxChroma.
 *
 * Typically 26–30 samples, one solve and one oklch→rgb conversion each.
 *
 * @param innerIter Ignored. Kept so existing callers keep compiling.
 */
export function gamutMapWithTonePreservation(
  targetY: number,
  hue: number,
  maxChroma: number,
  outerIter = 25,
  innerIter = 20,
): { l: number; c: number; h: number } {
  const top = sample(targetY, hue, maxChroma);
  if (top.fits) return { l: findLForTargetY(targetY, maxChroma, hue), c: maxChroma, h: hue };

  const tol = maxChroma / 2 ** outerIter;
  let lo = 0;
  let hi = top;
  let island: Bracket | null = null;
  for (let i = 0; i < outerIter; i++) {
    const m = sample(targetY, hue, (lo + hi.c) / 2);
    if (m.fits) { lo = m.c; continue; }
    island = topInside(targetY, hue, m, hi, tol);
    if (island) break;
    hi = m;
  }

  const c = island ? (island.lo + island.hi.c) / 2 : (lo + hi.c) / 2;
  return { l: findLForTargetY(targetY, c, hue), c, h: hue };
}