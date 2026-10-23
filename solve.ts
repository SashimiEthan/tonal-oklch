/**
 * Solvers for the hybrid OKLCh + CIE L* color space.
 *
 * Core idea: CIE Y (≡ WCAG relative luminance) is the immutable constraint.
 * At fixed (C, h), Y is a cubic in OKLCh L, so L is solved in closed form.
 * Gamut mapping reduces chroma while re-solving L at every step,
 * so tone is never "stale" — unlike Chromator's sequential approach.
 */

import { isDisplayable } from './luminance';
import { M_AB, LMS2RGB, oklchToLrgb } from './oklab';

// ── Internal helpers ──────────────────────────────────────

/** Check if an OKLCh color is within the sRGB gamut (checked in linear RGB). */
function inGamut(L: number, C: number, h: number): boolean {
  const { r, g, b } = oklchToLrgb({ l: L, c: C, h });
  return isDisplayable(r, g, b);
}

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
  const rad = (hue / 180) * Math.PI;
  // Guarded like oklab.ts so chroma 0 with an undefined hue stays achromatic.
  const a = chroma ? chroma * Math.cos(rad) : 0;
  const b = chroma ? chroma * Math.sin(rad) : 0;

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

/**
 * Find the maximum OKLCh chroma that fits in sRGB while hitting
 * a target CIE Y exactly. Binary search on chroma; each step
 * re-solves L for the target Y at the candidate chroma.
 *
 * Worst case: ~25 steps, one solve and one oklch→rgb conversion each.
 */
export function gamutMapWithTonePreservation(
  targetY: number,
  hue: number,
  maxChroma: number,
  outerIter = 25,
  innerIter = 20,
): { l: number; c: number; h: number } {
  let lo = 0;
  let hi = maxChroma;

  for (let i = 0; i < outerIter; i++) {
    const mid = (lo + hi) / 2;
    const L = findLForTargetY(targetY, mid, hue, innerIter);
    if (inGamut(L, mid, hue)) lo = mid;
    else hi = mid;
  }

  const finalC = (lo + hi) / 2;
  const finalL = findLForTargetY(targetY, finalC, hue);
  return { l: finalL, c: finalC, h: hue };
}