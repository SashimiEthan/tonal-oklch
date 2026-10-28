# tonal-oklch

A hybrid color space pairing **OKLCh's** perceptual hue and chroma with **CIE L\*'s** guaranteed WCAG contrast. Colors at the same **tone** have identical WCAG contrast ratios against any background, regardless of hue or chroma.

This crate is a dependency-free port of the [`tonal-oklch`](https://www.npmjs.com/package/tonal-oklch) npm package (0.2.0). It gives the same 8-bit colors: every one of the 22,185 hexes in the package's golden fixture matches, float sRGB channels agree to within 1e-13, and the package's behavioral test suite is ported to `tests/behavior.rs`.

## Install

```toml
[dependencies]
tonal-oklch = "0.1"
```

## Usage

```rust
use tonal_oklch::{TonalOklch, rgb_to_tonal_oklch, tonal_oklch_to_result, tonal_oklch_to_rgb};

// Tonal OKLCh → sRGB
let result = tonal_oklch_to_result(TonalOklch { tone: 65.0, chroma: 0.15, hue: 264.0 });
assert_eq!(result.hex(), "#6e9cfb");
assert_eq!(result.tone, 65.0); // preserved exactly

// Quick conversion (channels 0–1)
let rgb = tonal_oklch_to_rgb(TonalOklch::new(65.0, 0.15, 264.0));

// Reverse: sRGB → Tonal OKLCh
let tonal = rgb_to_tonal_oklch(0.43, 0.61, 0.98);
assert!((tonal.tone - 65.0).abs() < 1.0); // CIE L* lightness
```

The same functions are available as methods: `color.to_result()`, `color.to_rgb()`, `color.to_oklch()` and `TonalOklch::from_rgb(r, g, b)`.

## API

### `tonal_oklch_to_result(color: TonalOklch) -> TonalOklchResult`

Full conversion with metadata: `rgb` (float sRGB at the exact tone, with out-of-range channels pulled toward gray rather than clipped), `rgb8` (rounded, then nudged by at most one step per channel toward the exact tone), `oklch` (read back from `rgb8`) and `tone`. `result.hex()` and `result.rgb8.hex()` give the lowercase `#rrggbb` string; `Rgb8` also implements `Display`.

### `tonal_oklch_to_rgb8(color: TonalOklch) -> Rgb8`

The nudged 8-bit color alone, without the rest of the result. The cheapest route to a hex string: `tonal_oklch_to_rgb8(color).hex()`.

### `tonal_oklch_to_rgb(color: TonalOklch) -> Srgb`

Convenience wrapper returning the nudged 8-bit color as `{ r, g, b }` in 0–1.

### `tonal_oklch_to_oklch(color: TonalOklch) -> Oklch`

Gamut-mapped OKLCh coordinates. Tone is preserved exactly; chroma is reduced only as needed.

### `rgb_to_tonal_oklch(r: f64, g: f64, b: f64) -> TonalOklch`

Reverse conversion from sRGB (channels 0–1) to Tonal OKLCh.

### Low-level exports

- `cie_lstar_to_y` / `y_to_cie_lstar`: CIE L\* ↔ Y
- `wcag_luminance` / `wcag_contrast`: WCAG 2.1 utilities
- `linearize` / `delinearize` / `is_displayable`: sRGB transfer function and gamut test
- `oklch_to_rgb` / `rgb_to_oklch` / `oklch_to_lrgb`: plain OKLCh ↔ sRGB
- `find_l_for_target_y`: closed-form cubic solver for OKLCh L
- `gamut_map_with_tone_preservation`: chroma reduction with exact tone

## Performance

Single thread, on the 22,185-color golden grid (72% of which needs gamut mapping), median of 9 runs on an Apple Silicon laptop:

| | Rust (native) | JS (Node 24) |
|---|---|---|
| `tonal_oklch_to_result`, all colors | 0.31 µs/color | 4.54 µs/color |
| in-gamut colors | 0.12 µs/color | 0.88 µs/color |
| gamut-mapped colors | 0.37 µs/color | 5.98 µs/color |
| `tonal_oklch_to_rgb8`, all colors | 0.26 µs/color | — |
| `tonal_oklch_to_oklch`, all colors | 0.22 µs/color | 3.99 µs/color |

Reproduce with `cargo run --release --example bench` and `npm run bench` from the repository root. The conversions are pure functions, so large palettes parallelize trivially with [rayon](https://crates.io/crates/rayon): `colors.par_iter().map(|&c| tonal_oklch_to_result(c))`.

## WebAssembly

The repository also ships a `tonal-oklch-wasm` crate with a batch API for JavaScript consumers. See `crates/tonal-oklch-wasm/README.md`.

## License

MIT
