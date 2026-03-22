/**
 * Pure-math luminance and CIE L* utilities.
 * No external dependencies — all formulas from WCAG 2.1 and CIE standards.
 */

// ── sRGB linearization ────────────────────────────────────

/** Linearize a single sRGB channel (gamma-decode). */
export function linearize(c: number): number {
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

// ── WCAG relative luminance ───────────────────────────────

/** WCAG 2.1 relative luminance from sRGB 0–1 values. */
export function wcagLuminance(r: number, g: number, b: number): number {
  return 0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b);
}

/** WCAG 2.1 contrast ratio between two luminance values. */
export function wcagContrast(Y1: number, Y2: number): number {
  const lighter = Math.max(Y1, Y2);
  const darker = Math.min(Y1, Y2);
  return (lighter + 0.05) / (darker + 0.05);
}

// ── CIE L* ↔ Y ───────────────────────────────────────────

const KAPPA = 903.2963;   // (29/3)^3
const EPSILON = 216 / 24389; // (6/29)^3

/** Convert CIE L* (0–100) to CIE Y (relative luminance 0–1). */
export function cieLstarToY(Lstar: number): number {
  if (Lstar <= 8) return Lstar / KAPPA;
  const fy = (Lstar + 16) / 116;
  return fy * fy * fy;
}

/** Convert CIE Y (relative luminance 0–1) to CIE L* (0–100). */
export function yToCieLstar(Y: number): number {
  if (Y <= EPSILON) return Y * KAPPA;
  return 116 * Math.cbrt(Y) - 16;
}

// ── sRGB displayability ───────────────────────────────────

/** Check if sRGB values are within the displayable [0, 1] range. */
export function isDisplayable(r: number, g: number, b: number, epsilon = 1e-4): boolean {
  return r >= -epsilon && r <= 1 + epsilon &&
         g >= -epsilon && g <= 1 + epsilon &&
         b >= -epsilon && b <= 1 + epsilon;
}
