import { describe, it, expect } from 'vitest';
import { oklchToLrgb, oklchToRgb, rgbToOklch } from '../oklab';
import { delinearize, linearize } from '../luminance';

describe('sRGB ↔ OKLCh', () => {
  it('matches the CSS Color 4 sample conversions for the sRGB primaries', () => {
    const red = rgbToOklch(1, 0, 0);
    expect(red.l).toBeCloseTo(0.627955, 5);
    expect(red.c).toBeCloseTo(0.257683, 5);
    expect(red.h).toBeCloseTo(29.2339, 3);

    const green = rgbToOklch(0, 1, 0);
    expect(green.l).toBeCloseTo(0.86644, 5);
    expect(green.c).toBeCloseTo(0.294827, 5);
    expect(green.h).toBeCloseTo(142.4953, 3);

    const blue = rgbToOklch(0, 0, 1);
    expect(blue.l).toBeCloseTo(0.452014, 5);
    expect(blue.c).toBeCloseTo(0.313214, 5);
    expect(blue.h).toBeCloseTo(264.052, 3);
  });

  it('maps white and black exactly enough', () => {
    const white = oklchToLrgb({ l: 1, c: 0, h: 0 });
    expect(white.r).toBeCloseTo(1, 12);
    expect(white.g).toBeCloseTo(1, 12);
    expect(white.b).toBeCloseTo(1, 12);
    expect(oklchToLrgb({ l: 0, c: 0, h: 0 })).toEqual({ r: 0, g: 0, b: 0 });
  });

  it('neutral inputs give chroma 0 and hue 0 exactly', () => {
    for (let v = 0; v < 256; v++) {
      const { c, h } = rgbToOklch(v / 255, v / 255, v / 255);
      expect(c).toBe(0);
      expect(h).toBe(0);
    }
  });

  it('round-trips 8-bit sRGB through OKLCh', () => {
    let maxErr = 0;
    for (let r = 0; r < 256; r += 15) {
      for (let g = 0; g < 256; g += 15) {
        for (let b = 0; b < 256; b += 15) {
          const back = oklchToRgb(rgbToOklch(r / 255, g / 255, b / 255));
          maxErr = Math.max(maxErr, Math.abs(back.r - r / 255), Math.abs(back.g - g / 255), Math.abs(back.b - b / 255));
        }
      }
    }
    expect(maxErr).toBeLessThan(1e-9);
  });

  it('round-trips out-of-gamut sRGB, mirroring negative channels as CSS Color 4 does', () => {
    const wide = rgbToOklch(-0.2, 0.3, 0.9);
    expect(wide.c).toBeCloseTo(0.2509, 4);
    expect(wide.h).toBeCloseTo(255.88, 2);
    const back = oklchToRgb(wide);
    expect(back.r).toBeCloseTo(-0.2, 9);
    expect(back.g).toBeCloseTo(0.3, 9);
    expect(back.b).toBeCloseTo(0.9, 9);
  });
});

describe('delinearize', () => {
  it('inverts linearize on [0, 1]', () => {
    for (let v = 0; v <= 1; v += 1 / 255) {
      expect(delinearize(linearize(v))).toBeCloseTo(v, 12);
    }
  });

  it('is mirrored around zero and unclamped above one', () => {
    expect(delinearize(-0.01)).toBe(-delinearize(0.01));
    expect(delinearize(-0.001)).toBe(-delinearize(0.001));
    expect(delinearize(1.2)).toBeGreaterThan(1);
  });
});
