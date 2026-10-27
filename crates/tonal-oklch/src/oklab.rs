//! sRGB ↔ Oklab ↔ OKLCh conversions.
//! Matrices verbatim from culori 4.0.2 (src/oklab/convertOklabToLrgb.js,
//! src/oklab/convertLrgbToOklab.js), which match CSS Color 4.

// Keep the matrix literals byte-identical to culori's source.
#![allow(clippy::excessive_precision)]

use std::f64::consts::PI;

use crate::Srgb;
use crate::luminance::{delinearize, linearize};

/// A color in OKLCh: lightness 0–1, chroma 0–~0.4, hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

/// A color in linear sRGB. Out-of-gamut colors have channels outside [0, 1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lrgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// Oklab (a, b) → offset added to L for each cone response, rows L, M, S.
pub(crate) const M_AB: [[f64; 2]; 3] = [
    [0.3963377773761749, 0.2158037573099136],
    [-0.1055613458156586, -0.0638541728258133],
    [-0.0894841775298119, -1.2914855480194092],
];

/// Cubed cone responses (L, M, S) → linear sRGB, rows r, g, b.
pub(crate) const LMS2RGB: [[f64; 3]; 3] = [
    [4.0767416360759574, -3.3077115392580616, 0.2309699031821044],
    [-1.2684379732850317, 2.6097573492876887, -0.3413193760026573],
    [-0.0041960761386756, -0.7034186179359362, 1.7076146940746117],
];

/// Linear sRGB → cubed cone responses, rows L, M, S.
const RGB2LMS: [[f64; 3]; 3] = [
    [0.412221469470763, 0.5363325372617348, 0.0514459932675022],
    [0.2119034958178252, 0.6806995506452344, 0.1073969535369406],
    [0.0883024591900564, 0.2817188391361215, 0.6299787016738222],
];

/// Cube-rooted cone responses → Oklab, rows l, a, b.
const LMS2LAB: [[f64; 3]; 3] = [
    [0.210454268309314, 0.7936177747023054, -0.0040720430116193],
    [1.9779985324311684, -2.4285922420485799, 0.450593709617411],
    [0.0259040424655478, 0.7827717124575296, -0.8086757549230774],
];

/// Unit vector of a hue angle in the Oklab (a, b) plane.
/// A NaN or infinite hue counts as 0, as CSS Color 4 does.
#[inline]
pub(crate) fn hue_direction(h: f64) -> (f64, f64) {
    let deg = if h.is_finite() { h } else { 0.0 };
    let rad = (deg / 180.0) * PI;
    (rad.cos(), rad.sin())
}

/// Chroma and hue → Oklab a, b.
#[inline]
pub(crate) fn chroma_hue_to_ab(c: f64, h: f64) -> (f64, f64) {
    let (u, v) = hue_direction(h);
    (c * u, c * v)
}

#[inline]
fn cube(x: f64) -> f64 {
    x * x * x
}

/// Oklab → linear sRGB.
#[inline]
pub(crate) fn lab_to_lrgb(l: f64, a: f64, b: f64) -> Lrgb {
    let long = cube(l + M_AB[0][0] * a + M_AB[0][1] * b);
    let medium = cube(l + M_AB[1][0] * a + M_AB[1][1] * b);
    let short = cube(l + M_AB[2][0] * a + M_AB[2][1] * b);

    Lrgb {
        r: LMS2RGB[0][0] * long + LMS2RGB[0][1] * medium + LMS2RGB[0][2] * short,
        g: LMS2RGB[1][0] * long + LMS2RGB[1][1] * medium + LMS2RGB[1][2] * short,
        b: LMS2RGB[2][0] * long + LMS2RGB[2][1] * medium + LMS2RGB[2][2] * short,
    }
}

/// OKLCh → linear sRGB. Out-of-gamut colors come back with channels outside [0, 1].
pub fn oklch_to_lrgb(color: Oklch) -> Lrgb {
    let (a, b) = chroma_hue_to_ab(color.c, color.h);
    lab_to_lrgb(color.l, a, b)
}

/// OKLCh → gamma-encoded sRGB (unclamped).
pub fn oklch_to_rgb(color: Oklch) -> Srgb {
    let Lrgb { r, g, b } = oklch_to_lrgb(color);
    Srgb {
        r: delinearize(r),
        g: delinearize(g),
        b: delinearize(b),
    }
}

#[inline]
fn normalize_hue(hue: f64) -> f64 {
    let hue = hue % 360.0;
    if hue < 0.0 { hue + 360.0 } else { hue }
}

/// Mirrored around zero, as CSS Color 4 specifies, so out-of-gamut channels keep their sign and magnitude.
#[inline]
fn linearize_mirrored(c: f64) -> f64 {
    if c < 0.0 {
        -linearize(-c)
    } else {
        linearize(c)
    }
}

/// Linear sRGB → OKLCh. `neutral` forces c = 0, h = 0 exactly; it is decided
/// on the gamma-encoded channels because `linearize` is not injective at its
/// two-segment junction.
#[inline]
pub(crate) fn lrgb_to_oklch(lr: f64, lg: f64, lb: f64, neutral: bool) -> Oklch {
    let long = (RGB2LMS[0][0] * lr + RGB2LMS[0][1] * lg + RGB2LMS[0][2] * lb).cbrt();
    let medium = (RGB2LMS[1][0] * lr + RGB2LMS[1][1] * lg + RGB2LMS[1][2] * lb).cbrt();
    let short = (RGB2LMS[2][0] * lr + RGB2LMS[2][1] * lg + RGB2LMS[2][2] * lb).cbrt();

    let l = LMS2LAB[0][0] * long + LMS2LAB[0][1] * medium + LMS2LAB[0][2] * short;
    if neutral {
        return Oklch { l, c: 0.0, h: 0.0 };
    }

    let a = LMS2LAB[1][0] * long + LMS2LAB[1][1] * medium + LMS2LAB[1][2] * short;
    let b = LMS2LAB[2][0] * long + LMS2LAB[2][1] * medium + LMS2LAB[2][2] * short;
    let c = (a * a + b * b).sqrt();
    let h = if c > 0.0 {
        normalize_hue(b.atan2(a) * 180.0 / PI)
    } else {
        0.0
    };
    Oklch { l, c, h }
}

/// Gamma-encoded sRGB → OKLCh. Neutral inputs (r = g = b) give c = 0, h = 0 exactly.
pub fn rgb_to_oklch(r: f64, g: f64, b: f64) -> Oklch {
    lrgb_to_oklch(
        linearize_mirrored(r),
        linearize_mirrored(g),
        linearize_mirrored(b),
        r == g && g == b,
    )
}
