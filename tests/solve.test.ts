import { describe, it, expect } from 'vitest';
import { findLForTargetY, gamutMapWithTonePreservation } from '../solve';
import { cieLstarToY } from '../luminance';

describe('findLForTargetY', () => {
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
});
