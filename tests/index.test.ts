import { describe, it, expect } from 'vitest';
import {
  tonalOklchToResult,
  tonalOklchToRgb,
  tonalOklchToOklch,
  rgbToTonalOklch,
} from '../index';
import { cieLstarToY, wcagLuminance, wcagContrast, yToCieLstar } from '../luminance';

// ── Edge cases ────────────────────────────────────────────

describe('edge cases', () => {
  it('tone 0 returns black', () => {
    const result = tonalOklchToResult({ tone: 0, chroma: 0.15, hue: 264 });
    expect(result.hex).toBe('#000000');
    expect(result.rgb).toEqual({ r: 0, g: 0, b: 0 });
    expect(result.tone).toBe(0);
  });

  it('tone 100 returns white', () => {
    const result = tonalOklchToResult({ tone: 100, chroma: 0.15, hue: 264 });
    expect(result.hex).toBe('#ffffff');
    expect(result.rgb).toEqual({ r: 1, g: 1, b: 1 });
    expect(result.tone).toBe(100);
  });

  it('chroma 0 produces exact neutral colors (R=G=B)', () => {
    for (const tone of [5, 20, 50, 80, 95]) {
      const result = tonalOklchToResult({ tone, chroma: 0, hue: 0 });
      const { r, g, b } = result.rgb8;
      expect(r).toBe(g);
      expect(g).toBe(b);
    }
  });

  it('a missing hue counts as 0, as 0.1.x did', () => {
    type Input = Parameters<typeof tonalOklchToResult>[0];
    const gray = { tone: 49.83, chroma: 0 } as unknown as Input;
    expect(tonalOklchToResult(gray).hex).toBe('#767676');
    const pink = { tone: 50, chroma: 0.1 } as unknown as Input;
    expect(tonalOklchToResult(pink).hex).toBe('#aa6177');
    expect(tonalOklchToResult(pink).hex).toBe(tonalOklchToResult({ tone: 50, chroma: 0.1, hue: 0 }).hex);
  });

  it('negative tone clamps to black', () => {
    const result = tonalOklchToResult({ tone: -10, chroma: 0.1, hue: 100 });
    expect(result.hex).toBe('#000000');
  });

  it('tone > 100 clamps to white', () => {
    const result = tonalOklchToResult({ tone: 110, chroma: 0.1, hue: 100 });
    expect(result.hex).toBe('#ffffff');
  });
});

// ── Tone preservation ─────────────────────────────────────

describe('tone preservation', () => {
  it('preserves tone in result metadata', () => {
    for (const tone of [10, 25, 50, 75, 90]) {
      const result = tonalOklchToResult({ tone, chroma: 0.15, hue: 264 });
      expect(result.tone).toBe(tone);
    }
  });

  it('float RGB matches target WCAG luminance', () => {
    for (const tone of [20, 40, 60, 80]) {
      const result = tonalOklchToResult({ tone, chroma: 0.12, hue: 150 });
      const { r, g, b } = result.rgb;
      const actualY = wcagLuminance(r, g, b);
      const targetY = cieLstarToY(tone);
      expect(actualY).toBeCloseTo(targetY, 3);
    }
  });

  it('float RGB holds the exact tone for gamut-mapped colors', () => {
    const tones = [0.5, 1, 2, 3, 5, 10, 20, 30, 50, 70, 90, 97, 99, 99.5];
    let maxYErr = 0;
    let minChannel = Infinity;
    let maxChannel = -Infinity;

    for (const tone of tones) {
      for (let ci = 0; ci <= 8; ci++) {
        for (let hue = 0; hue < 360; hue += 10) {
          const { r, g, b } = tonalOklchToResult({ tone, chroma: ci / 20, hue }).rgb;
          minChannel = Math.min(minChannel, r, g, b);
          maxChannel = Math.max(maxChannel, r, g, b);
          maxYErr = Math.max(maxYErr, Math.abs(wcagLuminance(r, g, b) - cieLstarToY(tone)));
        }
      }
    }

    expect(minChannel).toBeGreaterThanOrEqual(0);
    expect(maxChannel).toBeLessThanOrEqual(1);
    expect(maxYErr).toBeLessThanOrEqual(1e-9);
  });

  it('keeps the 8-bit output while fixing the float for a dark mapped color', () => {
    const result = tonalOklchToResult({ tone: 5, chroma: 0.2, hue: 264 });
    expect(result.hex).toBe('#01004e');
    expect(yToCieLstar(wcagLuminance(result.rgb.r, result.rgb.g, result.rgb.b))).toBeCloseTo(5, 9);
  });

  it('never clamps L between tone 0.01 and 99.9995, where the float tone is exact', () => {
    let minL = Infinity;
    let maxL = -Infinity;
    for (const tone of [0.01, 0.02, 0.05, 0.1, 0.5, 1, 50, 99, 99.9, 99.99, 99.999, 99.9995]) {
      for (let ci = 1; ci <= 40; ci++) {
        for (let hue = 0; hue < 360; hue += 2) {
          const { l } = tonalOklchToOklch({ tone, chroma: ci / 100, hue });
          minL = Math.min(minL, l);
          maxL = Math.max(maxL, l);
        }
      }
    }
    expect(minL).toBeGreaterThan(0);
    expect(maxL).toBeLessThan(1);
  });

  it('8-bit RGB is within ±0.01 of target Y (post-nudge)', () => {
    for (let hue = 0; hue < 360; hue += 30) {
      const result = tonalOklchToResult({ tone: 50, chroma: 0.15, hue });
      const { r, g, b } = result.rgb8;
      const actualY = wcagLuminance(r / 255, g / 255, b / 255);
      const targetY = cieLstarToY(50);
      expect(Math.abs(actualY - targetY)).toBeLessThan(0.01);
    }
  });
});

// ── WCAG contrast consistency ─────────────────────────────

describe('WCAG contrast consistency across hues', () => {
  it('same tone produces nearly identical contrast ratios', () => {
    const tone = 50;
    const whiteY = 1;
    const ratios: number[] = [];

    for (let hue = 0; hue < 360; hue += 15) {
      const result = tonalOklchToResult({ tone, chroma: 0.15, hue });
      const { r, g, b } = result.rgb8;
      const Y = wcagLuminance(r / 255, g / 255, b / 255);
      ratios.push(wcagContrast(Y, whiteY));
    }

    const min = Math.min(...ratios);
    const max = Math.max(...ratios);
    // Contrast spread should be very tight (< 0.1)
    expect(max - min).toBeLessThan(0.1);
  });

  it('different tones produce different contrast ratios', () => {
    const hue = 264;
    const r1 = tonalOklchToResult({ tone: 30, chroma: 0.15, hue });
    const r2 = tonalOklchToResult({ tone: 70, chroma: 0.15, hue });

    const Y1 = wcagLuminance(r1.rgb8.r / 255, r1.rgb8.g / 255, r1.rgb8.b / 255);
    const Y2 = wcagLuminance(r2.rgb8.r / 255, r2.rgb8.g / 255, r2.rgb8.b / 255);

    expect(wcagContrast(Y1, 1)).not.toBeCloseTo(wcagContrast(Y2, 1), 0);
  });
});

// ── Gamut mapping ─────────────────────────────────────────

describe('gamut mapping', () => {
  it('all outputs are valid sRGB hex', () => {
    for (let hue = 0; hue < 360; hue += 30) {
      for (const tone of [20, 50, 80]) {
        const result = tonalOklchToResult({ tone, chroma: 0.3, hue });
        expect(result.hex).toMatch(/^#[0-9a-f]{6}$/);
        const { r, g, b } = result.rgb8;
        expect(r).toBeGreaterThanOrEqual(0);
        expect(r).toBeLessThanOrEqual(255);
        expect(g).toBeGreaterThanOrEqual(0);
        expect(g).toBeLessThanOrEqual(255);
        expect(b).toBeGreaterThanOrEqual(0);
        expect(b).toBeLessThanOrEqual(255);
      }
    }
  });

  it('reduces chroma for out-of-gamut requests', () => {
    const oklch = tonalOklchToOklch({ tone: 50, chroma: 0.4, hue: 264 });
    expect(oklch.c).toBeLessThan(0.4);
    expect(oklch.c).toBeGreaterThan(0);
  });

  it('keeps chroma when already in gamut', () => {
    const oklch = tonalOklchToOklch({ tone: 50, chroma: 0.01, hue: 100 });
    expect(oklch.c).toBeCloseTo(0.01, 3);
  });
});

// ── Round-trip conversion ─────────────────────────────────

describe('round-trip conversion', () => {
  it('rgbToTonalOklch inverts tonalOklchToRgb for achromatic', () => {
    for (const tone of [20, 50, 80]) {
      const rgb = tonalOklchToRgb({ tone, chroma: 0, hue: 0 });
      const back = rgbToTonalOklch(rgb.r, rgb.g, rgb.b);
      expect(back.tone).toBeCloseTo(tone, 0);
      expect(back.chroma).toBeCloseTo(0, 2);
    }
  });

  it('rgbToTonalOklch preserves tone within 1 unit for chromatic colors', () => {
    for (const hue of [30, 150, 264]) {
      const original = { tone: 60, chroma: 0.12, hue };
      const rgb = tonalOklchToRgb(original);
      const back = rgbToTonalOklch(rgb.r, rgb.g, rgb.b);
      expect(Math.abs(back.tone - original.tone)).toBeLessThan(1);
    }
  });

  it('rgbToTonalOklch returns known values for sRGB primaries', () => {
    const red = rgbToTonalOklch(1, 0, 0);
    const green = rgbToTonalOklch(0, 1, 0);
    const blue = rgbToTonalOklch(0, 0, 1);

    expect(red.tone).toBeCloseTo(yToCieLstar(0.2126), 3);
    expect(green.tone).toBeCloseTo(yToCieLstar(0.7152), 3);
    expect(blue.tone).toBeCloseTo(yToCieLstar(0.0722), 3);
  });
});

// ── tonalOklchToRgb convenience wrapper ───────────────────

describe('tonalOklchToRgb', () => {
  it('returns channels in 0–1 range', () => {
    const rgb = tonalOklchToRgb({ tone: 50, chroma: 0.15, hue: 264 });
    expect(rgb.r).toBeGreaterThanOrEqual(0);
    expect(rgb.r).toBeLessThanOrEqual(1);
    expect(rgb.g).toBeGreaterThanOrEqual(0);
    expect(rgb.g).toBeLessThanOrEqual(1);
    expect(rgb.b).toBeGreaterThanOrEqual(0);
    expect(rgb.b).toBeLessThanOrEqual(1);
  });

  it('matches rgb8 from tonalOklchToResult', () => {
    const color = { tone: 65, chroma: 0.15, hue: 264 };
    const result = tonalOklchToResult(color);
    const rgb = tonalOklchToRgb(color);
    expect(rgb.r).toBeCloseTo(result.rgb8.r / 255, 10);
    expect(rgb.g).toBeCloseTo(result.rgb8.g / 255, 10);
    expect(rgb.b).toBeCloseTo(result.rgb8.b / 255, 10);
  });
});
