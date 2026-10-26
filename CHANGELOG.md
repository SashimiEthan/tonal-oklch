# Changelog

## 0.2.0 - 2026-10-22

### Changed

- `findLForTargetY` solves OKLCh L in closed form (the cubic formula) instead of binary searching. Hex output is unchanged across the 22,185-color golden fixture; grays whose channels land exactly halfway between two 8-bit steps, and roughly one in a few hundred thousand other inputs, can shift by a step or two (both results sit within rounding of the target tone). The full conversion runs about 9× faster. For impossible inputs (very dark tones at high chroma, where no OKLCh color reaches the target luminance; the region grows with chroma, reaching about tone 5 at chroma 0.4) it now returns the largest real root where it returned ~0 before; those colors are gamut-mapped anyway, so `tonalOklchToResult` is unaffected. The `maxIter` parameter is ignored.
- `TonalOklchResult.rgb` now holds the exact tone for gamut-mapped colors. Previously, clamping a channel that sat just below 0 (within the gamut tolerance) raised the float color's tone by up to 0.07 at dark tones. Out-of-range channels are now pulled toward gray rather than clipped, so at the gamut edge `rgb` can sit a few 8-bit steps from `rgb8` and `hex` (up to about 4 for very dark colors). `rgb8`, `hex`, `oklch`, and `tonalOklchToRgb` are unchanged.

### Removed

- The `culori` dependency. Conversions are in-package, using the CSS Color 4 Oklab matrices, and produce identical results. The package has no runtime dependencies.

### Added

- `delinearize`: gamma-encode a linear sRGB channel (inverse of `linearize` for values of 0 and up; negatives are mirrored, as CSS Color 4 does).
- Golden hex fixture covering 22,185 colors, so future solver changes can prove they leave output unchanged.
