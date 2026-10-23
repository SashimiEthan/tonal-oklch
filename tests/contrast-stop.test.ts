import { describe, it, expect } from 'vitest';
import { tonalOklchToResult, wcagLuminance, wcagContrast } from '../index';

// tonal-oklch.com's 4.5:1 stop. Contrast is computed from rgb8 and floored to
// two decimals, exactly as the site displays it.
const STOP = 49.84;
const HUES = Array.from({ length: 13 }, (_, i) => i * 30);

function displayedContrast(color: { tone: number; chroma: number; hue: number }): number {
  const { r, g, b } = tonalOklchToResult(color).rgb8;
  const raw = wcagContrast(wcagLuminance(r / 255, g / 255, b / 255), 1);
  return Math.floor(raw * 100) / 100;
}

describe('4.5:1 stop at tone 49.84', () => {
  it('hue ramp at chroma 0.15 passes against white', () => {
    for (const hue of HUES) {
      expect(displayedContrast({ tone: STOP, chroma: 0.15, hue }), `hue ${hue}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('vibrant palette at each reference hue\'s max chroma passes against white', () => {
    for (const ref of HUES.slice(0, -1)) {
      const chroma = tonalOklchToResult({ tone: STOP, chroma: 0.5, hue: ref }).oklch.c;
      for (const hue of HUES) {
        expect(displayedContrast({ tone: STOP, chroma, hue }), `ref ${ref} hue ${hue}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it('gray token at tone 49.83 is #767676', () => {
    const result = tonalOklchToResult({ tone: 49.83, chroma: 0, hue: 0 });
    expect(result.hex).toBe('#767676');
    expect(displayedContrast({ tone: 49.83, chroma: 0, hue: 0 })).toBeGreaterThanOrEqual(4.5);
  });
});
