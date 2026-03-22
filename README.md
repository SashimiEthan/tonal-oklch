# tonal-oklch

A hybrid color space pairing **OKLCh's** perceptual hue & chroma with **CIE L\*'s** guaranteed WCAG contrast.

Colors at the same **tone** are guaranteed to have identical WCAG contrast ratios against any background — regardless of hue or chroma.

## Install

```bash
npm install tonal-oklch
```

## Usage

```ts
import { tonalOklchToResult, tonalOklchToRgb, rgbToTonalOklch } from 'tonal-oklch';

// Convert a Tonal OKLCh color → sRGB
const result = tonalOklchToResult({ tone: 65, chroma: 0.15, hue: 264 });
console.log(result.hex);    // e.g. "#8b7cc8"
console.log(result.tone);   // 65 (preserved exactly)

// Quick conversion (returns { r, g, b } in 0–1)
const rgb = tonalOklchToRgb({ tone: 65, chroma: 0.15, hue: 264 });

// Reverse: sRGB → Tonal OKLCh
const tonal = rgbToTonalOklch(0.55, 0.49, 0.78);
console.log(tonal.tone);    // CIE L* lightness
```

## API

### `tonalOklchToResult(color: TonalOklch): TonalOklchResult`

Full conversion with metadata — returns sRGB (float + 8-bit), hex, final OKLCh coordinates, and tone.

### `tonalOklchToRgb(color: TonalOklch): SrgbColor`

Convenience wrapper returning `{ r, g, b }` in 0–1.

### `tonalOklchToOklch(color: TonalOklch): { l, c, h }`

Returns gamut-mapped OKLCh coordinates. Tone is preserved exactly; chroma is reduced only as needed.

### `rgbToTonalOklch(r, g, b): TonalOklch`

Reverse conversion from sRGB (channels 0–1) to Tonal OKLCh.

### Low-level exports

- `cieLstarToY` / `yToCieLstar` — CIE L\* ↔ Y conversions
- `wcagLuminance` / `wcagContrast` — WCAG 2.1 utilities
- `findLForTargetY` — binary search solver for OKLCh L
- `gamutMapWithTonePreservation` — chroma reduction with exact tone

## How it works

1. **Tone → Y**: CIE L\* is converted to CIE Y (≡ WCAG relative luminance)
2. **Solve L**: Binary search finds the OKLCh lightness that produces the target Y at the given chroma & hue
3. **Gamut map**: If the color is outside sRGB, chroma is reduced while re-solving L at every step — tone is never stale
4. **Nudge**: 8-bit quantization is refined by ±1 per channel to minimize luminance drift

## License

MIT
