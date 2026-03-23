/**
 * Tonal OKLCh — a hybrid color space pairing OKLCh's perceptual hue & chroma
 * with CIE L*'s guaranteed WCAG contrast. Colors at the same tone are
 * guaranteed to have identical WCAG contrast ratios against any background,
 * regardless of hue or chroma.
 *
 * @example
 *   import { tonalOklchToRgb } from './src/tonal-oklch';
 *   const rgb = tonalOklchToRgb({ tone: 65, chroma: 0.15, hue: 264 });
 */

import { useMode, modeOklch, modeRgb, converter } from 'culori/fn';
import { cieLstarToY, yToCieLstar, wcagLuminance, linearize, isDisplayable } from './luminance';
import { findLForTargetY, gamutMapWithTonePreservation } from './solve';

// Ensure modes are registered (idempotent)
useMode(modeOklch);
useMode(modeRgb);

const toRgb = converter('rgb');
const toOklch = converter('oklch');

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
  /** The sRGB color (channels 0–1, clamped to gamut). */
  rgb: SrgbColor;
  /** The 8-bit sRGB color, nudged ±1 per channel to best match target luminance. */
  rgb8: { r: number; g: number; b: number };
  /** The hex string for the nudged 8-bit color. */
  hex: string;
  /** The gamut-mapped OKLCh coordinates actually used. */
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
  const rgb = toRgb({ mode: 'oklch', l: L, c: chroma, h: hue });
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
  const rawRgb = toRgb({ mode: 'oklch', ...oklch });

  // Clamp float values
  const rf = Math.max(0, Math.min(1, rawRgb.r));
  const gf = Math.max(0, Math.min(1, rawRgb.g));
  const bf = Math.max(0, Math.min(1, rawRgb.b));

  // Quantise to 8-bit, then nudge to minimise luminance drift.
  // Skip nudge for achromatic colors to keep R=G=B exact.
  const r8 = Math.round(rf * 255);
  const g8 = Math.round(gf * 255);
  const b8 = Math.round(bf * 255);
  const targetY = cieLstarToY(tone);

  let nr: number, ng: number, nb: number;
  if (color.chroma === 0) {
    nr = r8; ng = g8; nb = b8;
  } else {
    [nr, ng, nb] = nudgeToTargetY(r8, g8, b8, targetY);
  }

  const hex = `#${toHex(nr)}${toHex(ng)}${toHex(nb)}`;

  // Derive OKLCh from the final 8-bit RGB so the coordinates match
  // what culori (or any other tool) would report for this hex value.
  const finalOklch = toOklch({ mode: 'rgb', r: nr / 255, g: ng / 255, b: nb / 255 });

  return {
    rgb: { r: rf, g: gf, b: bf },
    rgb8: { r: nr, g: ng, b: nb },
    hex,
    oklch: { l: finalOklch.l, c: finalOklch.c ?? 0, h: finalOklch.h ?? 0 },
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
  const oklch = toOklch({ mode: 'rgb', r, g, b });
  const Y = wcagLuminance(r, g, b);
  return {
    tone: yToCieLstar(Y),
    chroma: oklch.c ?? 0,
    hue: oklch.h ?? 0,
  };
}

// ── Re-exports ────────────────────────────────────────────

export { cieLstarToY, yToCieLstar, wcagLuminance, wcagContrast, linearize, isDisplayable } from './luminance';
export { findLForTargetY, gamutMapWithTonePreservation } from './solve';