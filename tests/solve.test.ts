import { describe, it, expect } from 'vitest';
import { findLForTargetY, gamutMapWithTonePreservation } from '../solve';
import { cieLstarToY, isDisplayable } from '../luminance';
import { oklchToLrgb } from '../oklab';

function luminanceOf(L: number, C: number, h: number): number {
  const { r, g, b } = oklchToLrgb({ l: L, c: C, h });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

describe('findLForTargetY', () => {
  it('round-trips to the target Y wherever the root is inside (0, 1)', () => {
    const tones = [0.01, 0.1, 0.5, ...Array.from({ length: 99 }, (_, i) => i + 1)];
    let maxErr = 0;
    let minL = Infinity;
    let maxL = -Infinity;
    let inside = 0;
    let total = 0;

    for (const tone of tones) {
      const targetY = cieLstarToY(tone);
      for (let ci = 0; ci <= 20; ci++) {
        const chroma = ci / 50;
        for (let hue = 0; hue < 360; hue += 5) {
          const L = findLForTargetY(targetY, chroma, hue);
          total++;
          minL = Math.min(minL, L);
          maxL = Math.max(maxL, L);
          if (L > 0 && L < 1) {
            inside++;
            maxErr = Math.max(maxErr, Math.abs(luminanceOf(L, chroma, hue) - targetY));
          }
        }
      }
    }

    expect(minL).toBeGreaterThanOrEqual(0);
    expect(maxL).toBeLessThanOrEqual(1);
    expect(inside).toBeGreaterThan(total * 0.95);
    expect(maxErr).toBeLessThanOrEqual(1e-12);
  });

  it('returns known roots', () => {
    expect(findLForTargetY(cieLstarToY(50), 0.1, 264)).toBeCloseTo(0.5711138, 7);
    // Impossible inputs: the cubic has three real roots; the largest is returned.
    expect(findLForTargetY(cieLstarToY(3), 0.4, 65)).toBeCloseTo(0.1702, 4);
    // Raw root is negative (−0.304); clamped.
    expect(findLForTargetY(cieLstarToY(1), 0.4, 99)).toBe(0);
  });

  it('returns small L for small Y', () => {
    const L = findLForTargetY(0.001, 0, 0);
    expect(L).toBeGreaterThan(0);
    expect(L).toBeLessThan(0.2);
  });

  it('returns L≈1 for Y≈1', () => {
    const L = findLForTargetY(0.999, 0, 0);
    expect(L).toBeCloseTo(1, 1);
  });

  it('finds accurate L for achromatic colors (chroma=0)', () => {
    for (const tone of [10, 25, 50, 75, 90]) {
      const targetY = cieLstarToY(tone);
      const L = findLForTargetY(targetY, 0, 0);
      expect(L).toBeGreaterThan(0);
      expect(L).toBeLessThan(1);
    }
  });

  it('produces valid L across hues for same Y and chroma', () => {
    const targetY = cieLstarToY(50);
    for (let hue = 0; hue < 360; hue += 30) {
      const L = findLForTargetY(targetY, 0.05, hue);
      expect(L).toBeGreaterThan(0);
      expect(L).toBeLessThan(1);
    }
  });

  it('increases L monotonically with Y (fixed chroma/hue)', () => {
    let prevL = -1;
    for (let tone = 5; tone <= 95; tone += 5) {
      const L = findLForTargetY(cieLstarToY(tone), 0.1, 264);
      expect(L).toBeGreaterThan(prevL);
      prevL = L;
    }
  });
});

describe('gamutMapWithTonePreservation', () => {
  it('returns a color within sRGB gamut', () => {
    for (let hue = 0; hue < 360; hue += 45) {
      const result = gamutMapWithTonePreservation(cieLstarToY(50), hue, 0.4);
      expect(result.c).toBeGreaterThanOrEqual(0);
      expect(result.c).toBeLessThanOrEqual(0.4);
      expect(result.l).toBeGreaterThan(0);
      expect(result.l).toBeLessThan(1);
    }
  });

  it('reduces chroma for highly saturated out-of-gamut colors', () => {
    const result = gamutMapWithTonePreservation(cieLstarToY(50), 264, 0.4);
    expect(result.c).toBeLessThan(0.4);
    expect(result.c).toBeGreaterThan(0);
  });

  it('preserves hue exactly', () => {
    for (const hue of [0, 90, 180, 264]) {
      const result = gamutMapWithTonePreservation(cieLstarToY(50), hue, 0.3);
      expect(result.h).toBe(hue);
    }
  });

  it('returns higher chroma for mid-tones than very dark tones', () => {
    const midC = gamutMapWithTonePreservation(cieLstarToY(50), 150, 0.4).c;
    const darkC = gamutMapWithTonePreservation(cieLstarToY(15), 150, 0.4).c;
    expect(midC).toBeGreaterThan(darkC);
  });

  it('returns the requested chroma unchanged when it fits', () => {
    const result = gamutMapWithTonePreservation(cieLstarToY(50), 150, 0.05);
    expect(result.c).toBe(0.05);
  });

  it('finds the upper feasible island at hue 264, whatever chroma was requested', () => {
    // Dark blues near the blue primary have a second feasible island above a gap.
    for (const [tone, lowerTop, minRequest] of [[10, 0.146, 0.2], [15, 0.169, 0.225], [20, 0.194, 0.25]]) {
      const Y = cieLstarToY(tone);
      const first = gamutMapWithTonePreservation(Y, 264, minRequest).c;
      expect(first).toBeGreaterThan(lowerTop + 0.01);
      for (let request = minRequest; request <= 0.4; request += 0.025) {
        expect(gamutMapWithTonePreservation(Y, 264, request).c).toBeCloseTo(first, 7);
      }
    }
  });

  it('mapped chroma never decreases as the request grows, and the result fits', () => {
    const hues = [0, 29, 100, 150, 200, 263.9, 264, 264.1, 300, 328];
    for (const tone of [0.5, 5, 10, 15, 20, 30, 50, 75, 99]) {
      const Y = cieLstarToY(tone);
      for (const hue of hues) {
        let prev = 0;
        for (let request = 0.01; request <= 0.4; request += 0.01) {
          const { l, c } = gamutMapWithTonePreservation(Y, hue, request);
          expect(c).toBeGreaterThanOrEqual(prev - 3e-8);
          const { r, g, b } = oklchToLrgb({ l, c, h: hue });
          expect(isDisplayable(r, g, b, 1e-4 + 1e-6)).toBe(true);
          prev = c;
        }
      }
    }
  });
});
