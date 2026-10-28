//! Tonal OKLCh: a hybrid color space pairing OKLCh's perceptual hue and
//! chroma with CIE L*'s guaranteed WCAG contrast. Colors at the same tone
//! have identical WCAG contrast ratios against any background, regardless of
//! hue or chroma.
//!
//! This is a port of the `tonal-oklch` npm package. It gives the same 8-bit
//! colors: every hex in the package's 22,185-color golden fixture matches.
//! Float channels agree to within 2e-14, not bit for bit.
//!
//! ```
//! use tonal_oklch::{TonalOklch, tonal_oklch_to_result};
//!
//! let result = tonal_oklch_to_result(TonalOklch { tone: 65.0, chroma: 0.15, hue: 264.0 });
//! assert_eq!(result.rgb8.hex(), "#6e9cfb");
//! assert_eq!(result.tone, 65.0);
//! ```

mod luminance;
mod oklab;
mod solve;

use std::fmt;
use std::sync::LazyLock;

pub use luminance::{
    cie_lstar_to_y, delinearize, is_displayable, linearize, wcag_contrast, wcag_luminance,
    y_to_cie_lstar,
};
pub use oklab::{Lrgb, Oklch, oklch_to_lrgb, oklch_to_rgb, rgb_to_oklch};
pub use solve::{find_l_for_target_y, gamut_map_with_tone_preservation};

use oklab::lrgb_to_oklch;
use solve::gamut_map_to_lrgb;

/// `linearize(k / 255)` for every 8-bit `k`. Filled by `linearize` itself on
/// first use, so it is bit-identical to calling it.
static LINEAR_8BIT: LazyLock<[f64; 256]> =
    LazyLock::new(|| std::array::from_fn(|k| linearize(k as f64 / 255.0)));

/// A color in Tonal OKLCh space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TonalOklch {
    /// CIE L* lightness, 0–100. Determines WCAG contrast.
    pub tone: f64,
    /// OKLCh chroma, 0–~0.4. Gamut-mapped down if necessary.
    pub chroma: f64,
    /// OKLCh hue angle in degrees.
    pub hue: f64,
}

/// A gamma-encoded sRGB color, channels 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Srgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// An 8-bit sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb8 {
    /// Lowercase `#rrggbb`.
    pub fn hex(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Rgb8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// Full result of a Tonal OKLCh conversion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TonalOklchResult {
    /// The sRGB color (channels 0–1) at the exact tone. Out-of-range channels
    /// are pulled toward gray rather than clipped.
    pub rgb: Srgb,
    /// `rgb` rounded to 8 bits, then nudged ±1 per channel to best match the target luminance.
    pub rgb8: Rgb8,
    /// The OKLCh coordinates read back from `rgb8`, so they match what any converter reports for its hex.
    pub oklch: Oklch,
    /// The input tone, preserved exactly after gamut mapping.
    pub tone: f64,
}

impl TonalOklchResult {
    /// Lowercase `#rrggbb` of `rgb8`.
    pub fn hex(&self) -> String {
        self.rgb8.hex()
    }
}

impl TonalOklch {
    pub const fn new(tone: f64, chroma: f64, hue: f64) -> Self {
        Self { tone, chroma, hue }
    }

    /// See [`tonal_oklch_to_oklch`].
    pub fn to_oklch(self) -> Oklch {
        tonal_oklch_to_oklch(self)
    }

    /// See [`tonal_oklch_to_result`].
    pub fn to_result(self) -> TonalOklchResult {
        tonal_oklch_to_result(self)
    }

    /// See [`tonal_oklch_to_rgb`].
    pub fn to_rgb(self) -> Srgb {
        tonal_oklch_to_rgb(self)
    }

    /// See [`tonal_oklch_to_rgb8`].
    pub fn to_rgb8(self) -> Rgb8 {
        tonal_oklch_to_rgb8(self)
    }

    /// See [`rgb_to_tonal_oklch`].
    pub fn from_rgb(r: f64, g: f64, b: f64) -> Self {
        rgb_to_tonal_oklch(r, g, b)
    }
}

/// Bring out-of-range linear RGB channels into [0, 1] by sliding the color
/// toward the gray of luminance Y. Every point on that line has luminance Y,
/// so the tone survives where plain clamping would raise it (by up to 0.07
/// for dark, high-chroma colors mapped to the gamut tolerance). Assumes the
/// input already has luminance Y; below about tone 0.006 or above 99.9995 it
/// doesn't, and the tone is only approximate.
fn pull_toward_gray(Lrgb { r, g, b }: Lrgb, y: f64) -> Lrgb {
    let mut s: f64 = 1.0;
    for v in [r, g, b] {
        if v < 0.0 {
            s = s.min(y / (y - v));
        } else if v > 1.0 {
            s = s.min((1.0 - y) / (v - y));
        }
    }
    if s == 1.0 {
        return Lrgb { r, g, b };
    }

    let pull = |v: f64| (y + s * (v - y)).clamp(0.0, 1.0);
    Lrgb {
        r: pull(r),
        g: pull(g),
        b: pull(b),
    }
}

/// Nudge 8-bit RGB channels by ±1 to minimise distance from a target CIE Y.
/// Tries all 27 neighbour combinations and picks the closest. The visual
/// difference is imperceptible (< 0.4% per channel) but tightens WCAG
/// contrast spread from ~0.04 to ~0.01. Also returns the winner's linear RGB.
fn nudge_to_target_y(r: i32, g: i32, b: i32, target_y: f64) -> (Rgb8, Lrgb) {
    let lut = &*LINEAR_8BIT;
    // Linear values of k-1, k, k+1; a neighbour off the 8-bit range is
    // marked so its error reads +inf and can never win.
    let around = |k: i32| -> ([f64; 3], [bool; 3]) {
        let mut lin = [0.0; 3];
        let mut valid = [false; 3];
        for (d, (l, v)) in lin.iter_mut().zip(valid.iter_mut()).enumerate() {
            let n = k - 1 + d as i32;
            *v = (0..=255).contains(&n);
            *l = lut[n.clamp(0, 255) as usize];
        }
        (lin, valid)
    };
    let (lr, vr) = around(r);
    let (lg, vg) = around(g);
    let (lb, vb) = around(b);

    // The centre is seeded first, so a neighbour must beat it strictly;
    // then the 27 candidates in scan order, first minimum winning. Selects
    // rather than branches: the winner is unpredictable.
    let mut best_err = (0.2126 * lr[1] + 0.7152 * lg[1] + 0.0722 * lb[1] - target_y).abs();
    let mut best = 13usize;
    for ir in 0..3 {
        let y_r = 0.2126 * lr[ir];
        for ig in 0..3 {
            let y_rg = y_r + 0.7152 * lg[ig];
            for ib in 0..3 {
                let err = (y_rg + 0.0722 * lb[ib] - target_y).abs();
                let err = if vr[ir] & vg[ig] & vb[ib] {
                    err
                } else {
                    f64::INFINITY
                };
                let better = err < best_err;
                best_err = if better { err } else { best_err };
                best = if better { ir * 9 + ig * 3 + ib } else { best };
            }
        }
    }

    let (ir, ig, ib) = (best / 9, (best / 3) % 3, best % 3);
    (
        Rgb8 {
            r: (r - 1 + ir as i32) as u8,
            g: (g - 1 + ig as i32) as u8,
            b: (b - 1 + ib as i32) as u8,
        },
        Lrgb {
            r: lr[ir],
            g: lg[ig],
            b: lb[ib],
        },
    )
}

/// Convert a Tonal OKLCh color to gamut-mapped OKLCh coordinates.
/// Tone (CIE L*) is preserved exactly; chroma is reduced only as needed.
pub fn tonal_oklch_to_oklch(color: TonalOklch) -> Oklch {
    let TonalOklch { tone, chroma, hue } = color;

    if tone <= 0.0 {
        return Oklch {
            l: 0.0,
            c: 0.0,
            h: hue,
        };
    }
    if tone >= 100.0 {
        return Oklch {
            l: 1.0,
            c: 0.0,
            h: hue,
        };
    }

    gamut_map_with_tone_preservation(cie_lstar_to_y(tone), hue, chroma)
}

/// Snap, not clamp: edge channels must come out as exactly 0 and 1.
#[inline]
fn gamma(v: f64) -> f64 {
    if v >= 1.0 {
        1.0
    } else if v <= 0.0 {
        0.0
    } else {
        delinearize(v)
    }
}

/// Everything up to the 8-bit color for a tone strictly inside (0, 100):
/// the exact-tone float sRGB, the nudged 8-bit color and its linear RGB.
fn convert(color: TonalOklch) -> (Srgb, Rgb8, Lrgb) {
    let TonalOklch { tone, chroma, hue } = color;
    let target_y = cie_lstar_to_y(tone);
    let (_, linear) = gamut_map_to_lrgb(target_y, hue, chroma);

    let exact = pull_toward_gray(linear, target_y);
    let rgb = Srgb {
        r: gamma(exact.r),
        g: gamma(exact.g),
        b: gamma(exact.b),
    };

    let (rgb8, lin) = if chroma == 0.0 {
        // One rounding for all three channels keeps R=G=B exact, so no nudge.
        let v = (gamma(target_y) * 255.0).round();
        let lv = LINEAR_8BIT[v as usize];
        (
            Rgb8 {
                r: v as u8,
                g: v as u8,
                b: v as u8,
            },
            Lrgb {
                r: lv,
                g: lv,
                b: lv,
            },
        )
    } else {
        let round = |v: f64| (v * 255.0).round() as i32;
        nudge_to_target_y(round(rgb.r), round(rgb.g), round(rgb.b), target_y)
    };

    (rgb, rgb8, lin)
}

/// Convert a Tonal OKLCh color to sRGB with full result metadata.
/// Tone is preserved exactly regardless of gamut mapping. The 8-bit output
/// is nudged ±1 per channel to minimise luminance drift from quantisation,
/// tightening contrast spread from ~0.04 to ~0.01 across hues.
pub fn tonal_oklch_to_result(color: TonalOklch) -> TonalOklchResult {
    let TonalOklch { tone, hue, .. } = color;

    if tone <= 0.0 {
        return TonalOklchResult {
            rgb: Srgb {
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            rgb8: Rgb8 { r: 0, g: 0, b: 0 },
            oklch: Oklch {
                l: 0.0,
                c: 0.0,
                h: hue,
            },
            tone: 0.0,
        };
    }
    if tone >= 100.0 {
        return TonalOklchResult {
            rgb: Srgb {
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            rgb8: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            oklch: Oklch {
                l: 1.0,
                c: 0.0,
                h: hue,
            },
            tone: 100.0,
        };
    }

    let (rgb, rgb8, lin) = convert(color);

    // Derive OKLCh from the final 8-bit RGB so the coordinates match what
    // culori (or any other tool) would report for this hex value.
    let neutral = rgb8.r == rgb8.g && rgb8.g == rgb8.b;
    let oklch = lrgb_to_oklch(lin.r, lin.g, lin.b, neutral);

    TonalOklchResult {
        rgb,
        rgb8,
        oklch,
        tone,
    }
}

/// Convert a Tonal OKLCh color to 8-bit sRGB: the same `rgb8` as
/// [`tonal_oklch_to_result`], without computing the rest of the result.
pub fn tonal_oklch_to_rgb8(color: TonalOklch) -> Rgb8 {
    if color.tone <= 0.0 {
        return Rgb8 { r: 0, g: 0, b: 0 };
    }
    if color.tone >= 100.0 {
        return Rgb8 {
            r: 255,
            g: 255,
            b: 255,
        };
    }
    convert(color).1
}

/// Convert a Tonal OKLCh color to sRGB. Returns the nudged 8-bit values as
/// 0–1 floats so what you see on screen matches the target luminance as
/// closely as possible.
pub fn tonal_oklch_to_rgb(color: TonalOklch) -> Srgb {
    let Rgb8 { r, g, b } = tonal_oklch_to_rgb8(color);
    Srgb {
        r: f64::from(r) / 255.0,
        g: f64::from(g) / 255.0,
        b: f64::from(b) / 255.0,
    }
}

/// Convert gamma-encoded sRGB values (0–1) to Tonal OKLCh.
pub fn rgb_to_tonal_oklch(r: f64, g: f64, b: f64) -> TonalOklch {
    let oklch = rgb_to_oklch(r, g, b);
    let y = wcag_luminance(r, g, b);
    TonalOklch {
        tone: y_to_cie_lstar(y),
        chroma: oklch.c,
        hue: oklch.h,
    }
}
