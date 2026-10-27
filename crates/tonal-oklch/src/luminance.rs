//! Luminance and CIE L* utilities, from WCAG 2.1 and the CIE standards.

/// Linearize (gamma-decode) one sRGB channel.
#[inline]
pub fn linearize(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Gamma-encode one linear sRGB channel. Mirrored around zero and unclamped,
/// as in CSS Color 4, so out-of-gamut values stay out of range.
#[inline]
pub fn delinearize(c: f64) -> f64 {
    let abs = c.abs();
    if abs > 0.0031308 {
        c.signum() * (1.055 * abs.powf(1.0 / 2.4) - 0.055)
    } else {
        c * 12.92
    }
}

/// WCAG 2.1 relative luminance of gamma-encoded sRGB channels in 0–1.
#[inline]
pub fn wcag_luminance(r: f64, g: f64, b: f64) -> f64 {
    0.2126 * linearize(r) + 0.7152 * linearize(g) + 0.0722 * linearize(b)
}

/// WCAG 2.1 contrast ratio between two relative luminances.
#[inline]
pub fn wcag_contrast(y1: f64, y2: f64) -> f64 {
    let (lighter, darker) = if y1 >= y2 { (y1, y2) } else { (y2, y1) };
    (lighter + 0.05) / (darker + 0.05)
}

/// (29/3)³
const KAPPA: f64 = 903.2963;
/// (6/29)³
const EPSILON: f64 = 216.0 / 24389.0;

/// CIE L* (0–100) → CIE Y (relative luminance, 0–1).
#[inline]
pub fn cie_lstar_to_y(lstar: f64) -> f64 {
    if lstar <= 8.0 {
        return lstar / KAPPA;
    }
    let fy = (lstar + 16.0) / 116.0;
    fy * fy * fy
}

/// CIE Y (relative luminance, 0–1) → CIE L* (0–100).
#[inline]
pub fn y_to_cie_lstar(y: f64) -> f64 {
    if y <= EPSILON {
        return y * KAPPA;
    }
    116.0 * y.cbrt() - 16.0
}

/// Whether RGB channels are within `epsilon` of the displayable [0, 1] range.
#[inline]
pub fn is_displayable(r: f64, g: f64, b: f64, epsilon: f64) -> bool {
    r >= -epsilon
        && r <= 1.0 + epsilon
        && g >= -epsilon
        && g <= 1.0 + epsilon
        && b >= -epsilon
        && b <= 1.0 + epsilon
}
