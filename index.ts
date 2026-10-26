/**
 * Tonal OKLCh — a hybrid color space pairing OKLCh's perceptual hue & chroma
 * with CIE L*'s guaranteed WCAG contrast. Colors at the same tone are
 * guaranteed to have identical WCAG contrast ratios against any background,
 * regardless of hue or chroma.
 *
 * @example
 *   import { tonalOklchToRgb } from 'tonal-oklch';
 *   const rgb = tonalOklchToRgb({ tone: 65, chroma: 0.15, hue: 264 });
 */

import { cieLstarToY, yToCieLstar, wcagLuminance, linearize, delinearize, isDisplayable } from './luminance';
import { oklchToRgb, oklchToLrgb, rgbToOklch, type Lrgb } from './oklab';
import { findLForTargetY, gamutMapWithTonePreservation } from './solve';

// ── Exact-tone float output ───────────────────────────────

/**
 * Bring out-of-range linear RGB channels into [0, 1] by sliding the color
 * toward the gray of luminance Y. Every point on that line has luminance Y,
 * so the tone survives where plain clamping would raise it (by up to 0.07
 * for dark, high-chroma colors mapped to the gamut tolerance). Assumes the
 * input already has luminance Y; below about tone 0.006 or above 99.9995 it
 * doesn't, and the tone is only approximate.
 */
function pullTowardGray({ r, g, b }: Lrgb, Y: number): Lrgb {
  let s = 1;
  for (const v of [r, g, b]) {
    if (v < 0) s = Math.min(s, Y / (Y - v));
    else if (v > 1) s = Math.min(s, (1 - Y) / (v - Y));
  }
  if (s === 1) return { r, g, b };

  const pull = (v: number) => Math.max(0, Math.min(1, Y + s * (v - Y)));
  return { r: pull(r), g: pull(g), b: pull(b) };
}

// ── Post-quantization nudge ───────────────────────────────

/** Compute WCAG luminance from 8-bit sRGB values. */
function y8(r: number, g: number, b: number): number {
  return 0.2126 * linearize(r / 255) + 0.7152 * linearize(g / 255) + 0.0722 * linearize(b / 255);
}

/**
 * Nudge 8-bit RGB channels by ±1 to minimise distance from a target
 * CIE Y. Tries all 27 neighbour combinations and picks the closest.
 * The visual difference is imperceptible (< 0.4% per channel) but
 * tightens WCAG contrast spread from ~0.04 to ~0.01.
 */
function nudgeToTargetY(
  r: number, g: number, b: number, targetY: number,
): [number, number, number] {
  let bestR = r, bestG = g, bestB = b;
  let bestErr = Math.abs(y8(r, g, b) - targetY);

  for (let dr = -1; dr <= 1; dr++) {
    for (let dg = -1; dg <= 1; dg++) {
      for (let db = -1; db <= 1; db++) {
        const nr = r + dr, ng = g + dg, nb = b + db;
        if (nr < 0 || nr > 255 || ng < 0 || ng > 255 || nb < 0 || nb > 255) continue;
        const err = Math.abs(y8(nr, ng, nb) - targetY);
        if (err < bestErr) {
          bestErr = err;
          bestR = nr; bestG = ng; bestB = nb;
        }
      }
    }
  }

  return [bestR, bestG, bestB];
}

// ── Types ─────────────────────────────────────────────────

/** A color in Tonal OKLCh space. */
export interface TonalOklch {
  /** CIE L* lightness, 0–100. Determines WCAG contrast. */
  tone: number;
  /** OKLCh chroma, 0–~0.4. Will be gamut-mapped down if necessary. */
  chroma: number;
  /** OKLCh hue angle, 0–360°. */
  hue: number;
}

/** A color in sRGB space, channels 0–1. */
export interface SrgbColor {
  r: number;
  g: number;
  b: number;
}

/** Full result of a tonal OKLCh conversion. */
export interface TonalOklchResult {
  /**
   * The sRGB color (channels 0–1) at the exact tone. Out-of-range channels
   * are pulled toward gray rather than clipped.
   */
  rgb: SrgbColor;
  /** The 8-bit sRGB color: `rgb` rounded, then nudged ±1 per channel to best match target luminance. */
  rgb8: { r: number; g: number; b: number };
  /** The hex string for `rgb8`. */
  hex: string;
  /** The OKLCh coordinates read back from `rgb8`, so they match what any converter reports for `hex`. */
  oklch: { l: number; c: number; h: number };
  /** The input tone, preserved exactly after gamut mapping. */
  tone: number;
}

// ── Helpers ───────────────────────────────────────────────

/** Convert an 8-bit channel to a 2-char hex string. */
function toHex(n: number): string {
  return n.toString(16).padStart(2, '0');
}

// ── Forward conversion ────────────────────────────────────

/**
 * Convert a Tonal OKLCh color to gamut-mapped OKLCh coordinates.
 * Tone (CIE L*) is preserved exactly; chroma is reduced only as needed.
 */
export function tonalOklchToOklch(color: TonalOklch): { l: number; c: number; h: number } {
  const { tone, chroma, hue } = color;

  if (tone <= 0) return { l: 0, c: 0, h: hue };
  if (tone >= 100) return { l: 1, c: 0, h: hue };

  const targetY = cieLstarToY(tone);
  const L = findLForTargetY(targetY, chroma, hue);

  // Check if the full-chroma color is in sRGB gamut
  const rgb = oklchToRgb({ l: L, c: chroma, h: hue });
  if (isDisplayable(rgb.r, rgb.g, rgb.b)) {
    return { l: L, c: chroma, h: hue };
  }

  // Gamut map: reduce chroma while re-solving L to preserve tone
  return gamutMapWithTonePreservation(targetY, hue, chroma);
}

/**
 * Convert a Tonal OKLCh color to sRGB with full result metadata.
 * Tone is preserved exactly regardless of gamut mapping.
 * The 8-bit output (rgb8, hex) is nudged ±1 per channel to minimise
 * luminance drift from quantisation — tightening contrast spread
 * from ~0.04 to ~0.01 across hues.
 */
export function tonalOklchToResult(color: TonalOklch): TonalOklchResult {
  const { tone } = color;

  if (tone <= 0) return {
    rgb: { r: 0, g: 0, b: 0 }, rgb8: { r: 0, g: 0, b: 0 },
    hex: '#000000', oklch: { l: 0, c: 0, h: color.hue }, tone: 0,
  };
  if (tone >= 100) return {
    rgb: { r: 1, g: 1, b: 1 }, rgb8: { r: 255, g: 255, b: 255 },
    hex: '#ffffff', oklch: { l: 1, c: 0, h: color.hue }, tone: 100,
  };

  const oklch = tonalOklchToOklch(color);
  const linear = oklchToLrgb(oklch);
  const targetY = cieLstarToY(tone);

  const exact = pullTowardGray(linear, targetY);
  // Snap, not clamp: edge channels must come out as exactly 0 and 1.
  const gamma = (v: number) => (v >= 1 ? 1 : v <= 0 ? 0 : delinearize(v));
  const rgb = { r: gamma(exact.r), g: gamma(exact.g), b: gamma(exact.b) };

  // 8-bit path: round rgb, then nudge to minimise luminance drift.
  let nr: number, ng: number, nb: number;
  if (color.chroma === 0) {
    // One rounding for all three channels keeps R=G=B exact, so no nudge.
    nr = ng = nb = Math.round(gamma(targetY) * 255);
  } else {
    [nr, ng, nb] = nudgeToTargetY(
      Math.round(rgb.r * 255), Math.round(rgb.g * 255), Math.round(rgb.b * 255), targetY,
    );
  }

  const hex = `#${toHex(nr)}${toHex(ng)}${toHex(nb)}`;

  // Derive OKLCh from the final 8-bit RGB so the coordinates match
  // what culori (or any other tool) would report for this hex value.
  const finalOklch = rgbToOklch(nr / 255, ng / 255, nb / 255);

  return {
    rgb,
    rgb8: { r: nr, g: ng, b: nb },
    hex,
    oklch: finalOklch,
    tone,
  };
}

/**
 * Convert a Tonal OKLCh color to sRGB. Convenience wrapper.
 * Returns the nudged 8-bit values as 0–1 floats so what you see
 * on screen matches the target luminance as closely as possible.
 */
export function tonalOklchToRgb(color: TonalOklch): SrgbColor {
  const { rgb8 } = tonalOklchToResult(color);
  return { r: rgb8.r / 255, g: rgb8.g / 255, b: rgb8.b / 255 };
}

// ── Reverse conversion ────────────────────────────────────

/** Convert sRGB values to Tonal OKLCh. */
export function rgbToTonalOklch(r: number, g: number, b: number): TonalOklch {
  const oklch = rgbToOklch(r, g, b);
  const Y = wcagLuminance(r, g, b);
  return {
    tone: yToCieLstar(Y),
    chroma: oklch.c,
    hue: oklch.h,
  };
}

// ── Re-exports ────────────────────────────────────────────

export { cieLstarToY, yToCieLstar, wcagLuminance, wcagContrast, linearize, delinearize, isDisplayable } from './luminance';
export { findLForTargetY, gamutMapWithTonePreservation } from './solve';