# Changelog

## 0.2.0 - 2026-10-22

### Changed

- `findLForTargetY` solves OKLCh L in closed form (the cubic formula) instead of binary searching. Every hex output is unchanged; the full conversion runs about 9× faster. For impossible inputs (roughly tone ≤ 12.5 at chroma ≥ 0.14, where no OKLCh color reaches the target luminance) it now returns the largest real root where it returned ~0 before; those colors are gamut-mapped anyway, so `tonalOklchToResult` is unaffected. The `maxIter` parameter is ignored.
- `TonalOklchResult.rgb` now holds the exact tone for gamut-mapped colors. Previously, clamping a channel that sat just below 0 (within the gamut tolerance) raised the float color's tone by up to 0.07 at dark tones. `rgb8`, `hex`, `oklch`, and `tonalOklchToRgb` are unchanged.

### Removed

- The `culori` dependency. Conversions are in-package, using the CSS Color 4 Oklab matrices, and produce identical results. The package has no runtime dependencies.

### Added

- `delinearize`: gamma-encode a linear sRGB channel (inverse of `linearize`).
- Golden hex fixture covering 22,185 colors, so future solver changes can prove they leave output unchanged.
