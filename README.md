# tonal-oklch

[![npm](https://img.shields.io/npm/v/tonal-oklch)](https://www.npmjs.com/package/tonal-oklch)

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
console.log(result.hex);    // "#6e9cfb"
console.log(result.tone);   // 65 (preserved exactly)

// Quick conversion (returns { r, g, b } in 0–1)
const rgb = tonalOklchToRgb({ tone: 65, chroma: 0.15, hue: 264 });

// Reverse: sRGB → Tonal OKLCh
const tonal = rgbToTonalOklch(0.43, 0.61, 0.98);
console.log(tonal.tone);    // CIE L* lightness, ≈ 65
```

## API

### `tonalOklchToResult(color: TonalOklch): TonalOklchResult`

Full conversion with metadata — returns sRGB (float + 8-bit), hex, final OKLCh coordinates, and tone. The float `rgb` holds the exact tone (out-of-range channels are pulled toward gray, not clipped); `rgb8` and `hex` are `rgb` rounded, then nudged by at most one step per channel toward the exact tone.

### `tonalOklchToRgb(color: TonalOklch): SrgbColor`

Convenience wrapper returning `{ r, g, b }` in 0–1.

### `tonalOklchToOklch(color: TonalOklch): { l, c, h }`

Returns gamut-mapped OKLCh coordinates. Tone is preserved exactly; chroma is reduced only as needed.

### `rgbToTonalOklch(r, g, b): TonalOklch`

Reverse conversion from sRGB (channels 0–1) to Tonal OKLCh.

### Low-level exports

- `cieLstarToY` / `yToCieLstar` — CIE L\* ↔ Y conversions
- `wcagLuminance` / `wcagContrast` — WCAG 2.1 utilities
- `findLForTargetY` — closed-form cubic solver for OKLCh L
- `gamutMapWithTonePreservation` — chroma reduction with exact tone

## How it works

1. **Tone → Y**: CIE L\* is converted to CIE Y (≡ WCAG relative luminance)
2. **Solve L**: At fixed chroma & hue, Y is a cubic in OKLCh lightness, so the cubic formula gives the lightness that produces the target Y directly
3. **Gamut map**: If the color is outside sRGB, chroma is reduced to the highest value that fits, re-solving L at every step — tone is never stale
4. **Pull toward gray**: Channels left a hair out of range by the gamut tolerance are slid toward the gray of the same luminance instead of clipped, so the tone stays exact
5. **Nudge**: 8-bit quantization is refined by ±1 per channel to minimize luminance drift

## Rust and WebAssembly

The same algorithm ships as a dependency-free Rust crate in [`crates/tonal-oklch`](crates/tonal-oklch), with [`crates/tonal-oklch-wasm`](crates/tonal-oklch-wasm) providing a batch API for WebAssembly. Both produce the same output as this package: all 22,185 colors in the golden fixture match byte for byte. Native Rust runs about 7.8× faster than the JS package, WASM about 2.7×.

```bash
cargo test --workspace        # Rust unit, behavioral and golden tests
cargo run --release --example bench
npm run test:wasm             # builds JS and WASM, then checks parity
```

## License

MIT
