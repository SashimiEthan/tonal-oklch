# tonal-oklch-wasm

WebAssembly bindings for the [`tonal-oklch`](../tonal-oklch) crate. Not published; build it from the repository root with [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```bash
npm run build:wasm   # wasm-pack build crates/tonal-oklch-wasm --target web --release
npm run test:wasm    # builds JS and WASM, then checks parity and timing
```

The output lands in `pkg/`: an ES module, its `.d.ts`, and a 47 KB `.wasm` file.

## Why batch functions

Every call across the JS ↔ WASM boundary costs more than converting one color, so the API takes flat typed arrays and converts many colors per call. On the 22,185-color golden grid, one batch call runs 2.7× faster than the JS package (1.7 vs 4.5 µs/color in Node 24). The native Rust crate is faster still (0.4 µs/color); the gap is WASM's software math library.

## Usage

```js
import init, { tonalOklchToRgb8Batch, tonalOklchToResultBatch } from './pkg/tonal_oklch_wasm.js';

await init();

// [tone, chroma, hue, ...] → Uint8Array [r, g, b, ...]
const rgb8 = tonalOklchToRgb8Batch(new Float64Array([65, 0.15, 264, 50, 0.1, 30]));
// rgb8 = [110, 156, 251, ...]  → #6e9cfb
```

In Node, load the bytes yourself:

```js
import { readFileSync } from 'node:fs';
import { initSync, tonalOklchToHex } from './pkg/tonal_oklch_wasm.js';

initSync({ module: readFileSync('./pkg/tonal_oklch_wasm_bg.wasm') });
tonalOklchToHex(65, 0.15, 264); // "#6e9cfb"
```

## API

All batch inputs are `Float64Array`s of `[tone, chroma, hue, ...]` triples unless noted. A length that isn't a multiple of 3 throws.

| Function | Returns | Layout per color |
|---|---|---|
| `tonalOklchToRgb8Batch(colors)` | `Uint8Array` | `r, g, b` (8-bit) |
| `tonalOklchToResultBatch(colors)` | `Float64Array` | `r, g, b, r8, g8, b8, l, c, h, tone` (stride 10) |
| `tonalOklchToOklchBatch(colors)` | `Float64Array` | gamut-mapped `l, c, h` |
| `rgbToTonalOklchBatch(rgb)` | `Float64Array` | `tone, chroma, hue` from `[r, g, b, ...]` in 0–1 |
| `tonalOklchToHex(tone, chroma, hue)` | `string` | one color's lowercase `#rrggbb` |

In the result layout, `r, g, b` are the exact-tone float channels (0–1), `r8, g8, b8` the nudged 8-bit channels, `l, c, h` the OKLCh read back from the 8-bit color, and `tone` the input tone. These are the fields of the npm package's `TonalOklchResult`, flattened.

## Parity

`tests/parity.mjs` checks the build against the repository's golden fixture (all 22,185 hexes match) and against the JS build (identical `rgb8`, `hex` and `tone`; float channels agree to within 2e-14).
