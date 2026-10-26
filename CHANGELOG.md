# Changelog

## 0.2.0 - 2026-10-22

### Changed

- `findLForTargetY` solves OKLCh L in closed form (the cubic formula) instead of binary searching. On its own, the solver swap leaves hex output unchanged across the 22,185-color golden fixture; grays whose channels land exactly halfway between two 8-bit steps, and roughly one in a few hundred thousand other inputs, can shift by a step or two (both results sit within rounding of the target tone). The full conversion runs about 9× faster. For impossible inputs (very dark tones at high chroma, where no OKLCh color reaches the target luminance; the region grows with chroma, reaching about tone 5 at chroma 0.4) it now returns the largest real root where it returned ~0 before; those colors are gamut-mapped anyway, so `tonalOklchToResult` is unaffected. The `maxIter` parameter is ignored.
- `TonalOklchResult.rgb`, `rgb8` and `hex` are now all derived from one exact-tone color. Previously, a channel that sat just below 0 (within the gamut tolerance) was clipped to 0, which raised the float color's tone by up to 0.07 at dark tones and, near black, left the hex several steps too light (tone 0.1 at chroma 0.1, hue 252 gave `#000008`, which measures tone 0.158). Out-of-range channels are now pulled toward gray instead, and `rgb8` is `rgb` rounded then nudged by at most one step per channel, as before. The near-black example now gives `#000005` (tone 0.099). 842 of the 22,185 golden hexes change (3.8%, 598 of them at tone 5 or below), each by at most 3 steps per channel and none by more than about one just-noticeable difference; `oklch` and `tonalOklchToRgb` follow `rgb8`.

### Removed

- The `culori` dependency. Conversions are in-package, using the CSS Color 4 Oklab matrices, and produce identical results. The package has no runtime dependencies.

### Fixed

- A hue of `NaN`, `Infinity` or `-Infinity` now counts as 0, the way CSS Color 4 treats a missing (`none`) or infinite hue, instead of producing `#NaNNaNNaN` with all-NaN `rgb`, `rgb8` and `oklch`.

### Added

- `delinearize`: gamma-encode a linear sRGB channel (inverse of `linearize` for values of 0 and up; negatives are mirrored, as CSS Color 4 does).
- Golden hex fixture covering 22,185 colors, so future solver changes can prove they leave output unchanged.
