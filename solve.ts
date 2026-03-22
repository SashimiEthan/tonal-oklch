/**
 * Binary search solvers for the hybrid OKLCh + CIE L* color space.
 *
 * Core idea: CIE Y (≡ WCAG relative luminance) is the immutable constraint.
 * OKLCh L is searched via bisection to hit a target Y at any (C, h).
 * Gamut mapping reduces chroma while re-solving L at every step,
 * so tone is never "stale" — unlike Chromator's sequential approach.
 */

import { useMode, modeOklch, modeRgb, modeLrgb, converter } from 'culori/fn';
import { isDisplayable } from './luminance';

// Register the culori modes we need
useMode(modeOklch);
useMode(modeRgb);
useMode(modeLrgb);

const toLrgb = converter('lrgb');

// ── Toe function for initial L estimate ───────────────────

const TOE_K1 = 0.173;
const TOE_K2 = 0.004;
const TOE_K3 = (1 + TOE_K1) / (1 + TOE_K2);

/** Inverse toe: map Lr (approx CIE Lstar / 100) to OKLCh L. Used to seed the bisection. */
function toeInv(Lr: number): number {
  return (Lr * Lr + TOE_K1 * Lr) / (TOE_K3 * (Lr + TOE_K2));
}

// ── Internal helpers ──────────────────────────────────────

/** Compute WCAG luminance (CIE Y) for an OKLCh color via linear RGB (no gamma round-trip). */
function getY(L: number, C: number, h: number): number {
  const lrgb = toLrgb({ mode: 'oklch', l: L, c: C, h });
  return 0.2126 * lrgb.r + 0.7152 * lrgb.g + 0.0722 * lrgb.b;
}

/** Check if an OKLCh color is within the sRGB gamut (checked in linear RGB). */
function inGamut(L: number, C: number, h: number): boolean {
  const lrgb = toLrgb({ mode: 'oklch', l: L, c: C, h });
  return isDisplayable(lrgb.r, lrgb.g, lrgb.b);
}

// ── Core solver: find OKLCh L for a target CIE Y ─────────

/**
 * Binary search for the OKLCh L that produces a target CIE Y
 * at given chroma and hue. Converges because Y(L) is monotonically
 * increasing for fixed (C, h).
 *
 * Uses the toe function to seed a narrow initial bracket,
 * converging in ~20 iterations instead of 30.
 */
export function findLForTargetY(
  targetY: number,
  chroma: number,
  hue: number,
  maxIter = 25,
): number {
  // Seed bracket using toe inverse: Lr ≈ tone/100, L ≈ toeInv(Lr)
  const est = toeInv(Math.cbrt(targetY) * 1.16 - 0.16);
  let lo = Math.max(0, est - 0.06);
  let hi = Math.min(1, est + 0.06);

  // Widen bracket if it doesn't contain the target
  if (getY(lo, chroma, hue) > targetY) lo = 0;
  if (getY(hi, chroma, hue) < targetY) hi = 1;

  for (let i = 0; i < maxIter; i++) {
    const mid = (lo + hi) / 2;
    if (getY(mid, chroma, hue) < targetY) lo = mid;
    else hi = mid;
  }

  return (lo + hi) / 2;
}

// ── Gamut mapping with tone preservation ──────────────────

/**
 * Find the maximum OKLCh chroma that fits in sRGB while hitting
 * a target CIE Y exactly. Binary search on chroma; inner loop
 * re-solves L for the target Y at each candidate chroma.
 *
 * Worst case: ~25 outer × ~20 inner = ~500 oklch→rgb conversions ≈ 30μs.
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