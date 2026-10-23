/**
 * Oklab ↔ linear sRGB matrices, verbatim from culori 4.0.2
 * (src/oklab/convertOklabToLrgb.js), which match CSS Color 4.
 */

/** Oklab (a, b) → offset added to L for each cone response, rows L, M, S. */
export const M_AB: ReadonlyArray<readonly [number, number]> = [
  [0.3963377773761749, 0.2158037573099136],
  [-0.1055613458156586, -0.0638541728258133],
  [-0.0894841775298119, -1.2914855480194092],
];

/** Cubed cone responses (L, M, S) → linear sRGB, rows r, g, b. */
export const LMS2RGB: ReadonlyArray<readonly [number, number, number]> = [
  [4.0767416360759574, -3.3077115392580616, 0.2309699031821044],
  [-1.2684379732850317, 2.6097573492876887, -0.3413193760026573],
  [-0.0041960761386756, -0.7034186179359362, 1.7076146940746117],
];
