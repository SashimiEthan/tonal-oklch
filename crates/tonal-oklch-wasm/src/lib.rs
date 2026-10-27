//! WebAssembly bindings for `tonal-oklch`.
//!
//! Every function works on flat typed arrays so a whole palette crosses the
//! JS↔WASM boundary in one call. Input colors are `[tone, chroma, hue, ...]`
//! triples; see each function for its output layout.

use tonal_oklch::{TonalOklch, rgb_to_tonal_oklch, tonal_oklch_to_oklch, tonal_oklch_to_result};
use wasm_bindgen::prelude::*;

/// Number of values per color in [`tonal_oklch_to_result_batch`]'s output.
pub const RESULT_STRIDE: usize = 10;

fn triples<'a>(
    values: &'a [f64],
    what: &str,
) -> Result<impl Iterator<Item = (f64, f64, f64)> + 'a, JsError> {
    if values.len() % 3 != 0 {
        return Err(JsError::new(&format!(
            "{what} expects a flat array of [{what}] triples; got {} values",
            values.len()
        )));
    }
    Ok(values.chunks_exact(3).map(|c| (c[0], c[1], c[2])))
}

/// `[tone, chroma, hue, ...]` → `[r, g, b, ...]` as 8-bit channels, one `Uint8Array`.
#[wasm_bindgen(js_name = tonalOklchToRgb8Batch)]
pub fn tonal_oklch_to_rgb8_batch(colors: &[f64]) -> Result<Vec<u8>, JsError> {
    let mut out = Vec::with_capacity(colors.len());
    for (tone, chroma, hue) in triples(colors, "tone, chroma, hue")? {
        let rgb8 = tonal_oklch_to_result(TonalOklch { tone, chroma, hue }).rgb8;
        out.extend_from_slice(&[rgb8.r, rgb8.g, rgb8.b]);
    }
    Ok(out)
}

/// `[tone, chroma, hue, ...]` → 10 values per color:
/// `[r, g, b, r8, g8, b8, l, c, h, tone]`, where `r, g, b` are the exact-tone
/// float channels (0–1), `r8, g8, b8` the nudged 8-bit channels, `l, c, h` the
/// OKLCh read back from the 8-bit color, and `tone` the input tone.
#[wasm_bindgen(js_name = tonalOklchToResultBatch)]
pub fn tonal_oklch_to_result_batch(colors: &[f64]) -> Result<Vec<f64>, JsError> {
    let mut out = Vec::with_capacity(colors.len() / 3 * RESULT_STRIDE);
    for (tone, chroma, hue) in triples(colors, "tone, chroma, hue")? {
        let r = tonal_oklch_to_result(TonalOklch { tone, chroma, hue });
        out.extend_from_slice(&[
            r.rgb.r,
            r.rgb.g,
            r.rgb.b,
            f64::from(r.rgb8.r),
            f64::from(r.rgb8.g),
            f64::from(r.rgb8.b),
            r.oklch.l,
            r.oklch.c,
            r.oklch.h,
            r.tone,
        ]);
    }
    Ok(out)
}

/// `[tone, chroma, hue, ...]` → gamut-mapped `[l, c, h, ...]`.
#[wasm_bindgen(js_name = tonalOklchToOklchBatch)]
pub fn tonal_oklch_to_oklch_batch(colors: &[f64]) -> Result<Vec<f64>, JsError> {
    let mut out = Vec::with_capacity(colors.len());
    for (tone, chroma, hue) in triples(colors, "tone, chroma, hue")? {
        let o = tonal_oklch_to_oklch(TonalOklch { tone, chroma, hue });
        out.extend_from_slice(&[o.l, o.c, o.h]);
    }
    Ok(out)
}

/// Gamma-encoded `[r, g, b, ...]` (0–1) → `[tone, chroma, hue, ...]`.
#[wasm_bindgen(js_name = rgbToTonalOklchBatch)]
pub fn rgb_to_tonal_oklch_batch(rgb: &[f64]) -> Result<Vec<f64>, JsError> {
    let mut out = Vec::with_capacity(rgb.len());
    for (r, g, b) in triples(rgb, "r, g, b")? {
        let t = rgb_to_tonal_oklch(r, g, b);
        out.extend_from_slice(&[t.tone, t.chroma, t.hue]);
    }
    Ok(out)
}

/// One color's lowercase `#rrggbb`. For many colors, use the batch functions.
#[wasm_bindgen(js_name = tonalOklchToHex)]
pub fn tonal_oklch_to_hex(tone: f64, chroma: f64, hue: f64) -> String {
    tonal_oklch_to_result(TonalOklch { tone, chroma, hue }).hex()
}
