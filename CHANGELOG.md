# Changelog

## 0.2.0 - 2026-10-26

### Changed

- `findLForTargetY` solves OKLCh L in closed form (the cubic formula) instead of binary searching. On its own, the solver swap leaves hex output unchanged across the 22,185-color golden fixture; grays whose channels land exactly halfway between two 8-bit steps, and roughly one in a few hundred thousand other inputs, can shift by a step or two (both results sit within rounding of the target tone). The full conversion runs about 9× faster. For impossible inputs (very dark tones at high chroma, where no OKLCh color reaches the target luminance; the region grows with chroma, reaching about tone 5 at chroma 0.4) it now returns the largest real root where it returned ~0 before; those colors are gamut-mapped anyway, so `tonalOklchToResult` is unaffected. The `maxIter` parameter is ignored.
- `TonalOklchResult.rgb`, `rgb8` and `hex` are now all derived from one exact-tone color. Previously, a channel that sat just below 0 (within the gamut tolerance) was clipped to 0, which raised the float color's tone by up to 0.07 at dark tones and, near black, left the hex several steps too light (tone 0.1 at chroma 0.1, hue 252 gave `#000008`, which measures tone 0.158). Out-of-range channels are now pulled toward gray instead, and `rgb8` is `rgb` rounded then nudged by at most one step per channel, as before. The near-black example now gives `#000005` (tone 0.099). 842 of the 22,185 golden hexes change (3.8%, 598 of them at tone 5 or below), each by at most 3 steps per channel and none by more than about one just-noticeable difference; `oklch` and `tonalOklchToRgb` follow `rgb8`.

### Removed

- The `culori` dependency. Conversions are in-package, using the CSS Color 4 Oklab matrices, and produce identical results. The package has no runtime dependencies.

### Fixed

- Dark blues near the sRGB blue primary no longer depend on the requested chroma. In a hue band about 0.15° wide (around hue 262.6 at tone 1 and 263.8 at tone 5, drifting to 264.1 by tone 30; hue 264 itself is inside it at tones 7.5–21) the constant-tone path grazes the gamut boundary, so the chromas that fit form two separate ranges, and the bisection landed on whichever range its midpoints happened to hit: `{ tone: 15, chroma: 0.4, hue: 264 }` gave `#00018c` while the smaller request `{ tone: 15, chroma: 0.3, hue: 264 }` gave the duller `#00147a`. Gamut mapping now returns the highest fitting chroma at or below the request, so mapped chroma never decreases as the request grows; at hue 264 that is `#00016d` at tone 10, `#00018c` at tone 15 and `#0100ac` at tone 20 for any request at or above about 0.17, 0.2 and 0.23. 23 of the 22,185 golden hexes change, all at hue 264; on a 2.3-million-color sweep at 0.5° hue steps, 222 hexes change, all at hues 263.5 and 264. The search takes a few more steps than before (typically 26–30 rather than 25) to rule out a hidden range.
- `tonalOklchToOklch` judged whether the requested chroma fits in gamma-encoded RGB, while gamut mapping judges in linear RGB, where the same 1e-4 tolerance is about 13× looser near 0 and 2.3× tighter near 1. One check now applies, in linear RGB, so a request is kept as is exactly when the mapping would have accepted it. On its own this lowers chroma by at most 2.5e-4 (light greens and yellows whose linear channel sat up to 1.8e-4 above 1) and changes no hex on the sweep above; combined with the fix above it is why `{ tone: 15, chroma: 0.2, hue: 264 }` is now kept at chroma 0.2 (`#00028b`).
- A hue of `NaN`, `Infinity` or `-Infinity` now counts as 0, the way CSS Color 4 treats a missing (`none`) or infinite hue, instead of producing `#NaNNaNNaN` with all-NaN `rgb`, `rgb8` and `oklch`.

### Added

- `delinearize`: gamma-encode a linear sRGB channel (inverse of `linearize` for values of 0 and up; negatives are mirrored, as CSS Color 4 does).
- Golden hex fixture covering 22,185 colors, so future solver changes can prove they leave output unchanged.
