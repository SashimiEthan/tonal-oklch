import { describe, it, expect } from 'vitest';
import {
  linearize,
  wcagLuminance,
  wcagContrast,
  cieLstarToY,
  yToCieLstar,
  isDisplayable,
} from '../luminance';

describe('linearize', () => {
  it('returns 0 for 0', () => {
    expect(linearize(0)).toBe(0);
  });

  it('returns 1 for 1', () => {
    expect(linearize(1)).toBeCloseTo(1, 10);
  });

  it('uses linear segment below threshold', () => {
    // Below 0.04045, linearize(c) = c / 12.92
    expect(linearize(0.04045)).toBeCloseTo(0.04045 / 12.92, 10);
  });

  it('uses gamma segment above threshold', () => {
    expect(linearize(0.5)).toBeCloseTo(((0.5 + 0.055) / 1.055) ** 2.4, 10);
  });

  it('is monotonically increasing', () => {
    let prev = -1;
    for (let c = 0; c <= 1; c += 0.01) {
      const val = linearize(c);
      expect(val).toBeGreaterThan(prev);
      prev = val;
    }
  });
});

describe('wcagLuminance', () => {
  it('returns 0 for black', () => {
    expect(wcagLuminance(0, 0, 0)).toBe(0);
  });

  it('returns 1 for white', () => {
    expect(wcagLuminance(1, 1, 1)).toBeCloseTo(1, 5);
  });

  it('weights green highest', () => {
    const rOnly = wcagLuminance(1, 0, 0);
    const gOnly = wcagLuminance(0, 1, 0);
    const bOnly = wcagLuminance(0, 0, 1);
    expect(gOnly).toBeGreaterThan(rOnly);
    expect(rOnly).toBeGreaterThan(bOnly);
  });

  it('matches known sRGB mid-gray', () => {
    // sRGB (0.5, 0.5, 0.5) → Y ≈ 0.2140
    const Y = wcagLuminance(0.5, 0.5, 0.5);
    expect(Y).toBeCloseTo(0.2140, 3);
  });
});

describe('wcagContrast', () => {
  it('returns 21 for black vs white', () => {
    expect(wcagContrast(0, 1)).toBeCloseTo(21, 1);
  });

  it('returns 1 for identical luminances', () => {
    expect(wcagContrast(0.5, 0.5)).toBe(1);
  });

  it('is symmetric', () => {
    expect(wcagContrast(0.2, 0.8)).toBe(wcagContrast(0.8, 0.2));
  });

  it('is always >= 1', () => {
    for (let i = 0; i < 20; i++) {
      const a = Math.random();
      const b = Math.random();
      expect(wcagContrast(a, b)).toBeGreaterThanOrEqual(1);
    }
  });
});

describe('cieLstarToY / yToCieLstar', () => {
  it('maps L*=0 to Y=0', () => {
    expect(cieLstarToY(0)).toBe(0);
  });

  it('maps L*=100 to Y≈1', () => {
    expect(cieLstarToY(100)).toBeCloseTo(1, 5);
  });

  it('maps L*=50 to Y≈0.184', () => {
    expect(cieLstarToY(50)).toBeCloseTo(0.1841, 3);
  });

  it('round-trips L* through Y', () => {
    for (const Lstar of [0, 8, 10, 25, 50, 75, 90, 100]) {
      const Y = cieLstarToY(Lstar);
      expect(yToCieLstar(Y)).toBeCloseTo(Lstar, 8);
    }
  });

  it('handles the linear/cubic boundary (L*=8)', () => {
    const Y = cieLstarToY(8);
    expect(yToCieLstar(Y)).toBeCloseTo(8, 8);
  });

  it('is monotonically increasing', () => {
    let prevY = -1;
    for (let L = 0; L <= 100; L += 1) {
      const Y = cieLstarToY(L);
      expect(Y).toBeGreaterThanOrEqual(prevY);
      prevY = Y;
    }
  });
});

describe('isDisplayable', () => {
  it('accepts black', () => {
    expect(isDisplayable(0, 0, 0)).toBe(true);
  });

  it('accepts white', () => {
    expect(isDisplayable(1, 1, 1)).toBe(true);
  });

  it('rejects out-of-gamut values', () => {
    expect(isDisplayable(1.01, 0, 0)).toBe(false);
    expect(isDisplayable(0, -0.01, 0)).toBe(false);
  });

  it('tolerates values within epsilon', () => {
    expect(isDisplayable(1.00005, 0, 0)).toBe(true);
    expect(isDisplayable(-0.00005, 0, 0)).toBe(true);
  });
});
