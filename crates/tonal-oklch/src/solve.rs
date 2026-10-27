//! Solvers for the hybrid OKLCh + CIE L* color space.
//!
//! CIE Y (≡ WCAG relative luminance) is the immutable constraint. At fixed
//! (C, h), Y is a cubic in OKLCh L, so L is solved in closed form. Gamut
//! mapping reduces chroma while re-solving L at every step, so tone is never
//! stale.

use crate::luminance::is_displayable;
use crate::oklab::{LMS2RGB, Lrgb, M_AB, Oklch, chroma_hue_to_ab, hue_direction, lab_to_lrgb};

/// Luminance weight of each cubed cone response: Y = Σ W[i]·LMS[i].
const W: [f64; 3] = {
    let mut w = [0.0; 3];
    let mut i = 0;
    while i < 3 {
        w[i] = 0.2126 * LMS2RGB[0][i] + 0.7152 * LMS2RGB[1][i] + 0.0722 * LMS2RGB[2][i];
        i += 1;
    }
    w
};

/// Largest real root of t³ + B·t² + C·t + D = 0.
#[inline]
fn largest_real_root(b: f64, c: f64, d: f64) -> f64 {
    let p = c - (b * b) / 3.0;
    let q = (2.0 * (b * b * b)) / 27.0 - (b * c) / 3.0 + d;
    let shift = -b / 3.0;
    let disc = (q * q) / 4.0 + (p * p * p) / 27.0;

    if disc > 0.0 {
        let s = disc.sqrt();
        // Cube-root only the larger-magnitude term and derive the other from it;
        // adding both cube roots cancels and drifts ~1e-7 at light tones.
        let u = (if q < 0.0 { -q / 2.0 + s } else { -q / 2.0 - s }).cbrt();
        return u - p / (3.0 * u) + shift;
    }

    if p == 0.0 {
        return (-q).cbrt() + shift;
    }

    let r = 2.0 * (-p / 3.0).sqrt();
    let phi = (((3.0 * q) / (2.0 * p)) * (-3.0 / p).sqrt())
        .clamp(-1.0, 1.0)
        .acos()
        / 3.0;
    r * phi.cos() + shift
}

/// OKLCh L that produces `target_y` at Oklab offsets (a, b), clamped to [0, 1].
#[inline]
fn solve_l(target_y: f64, a: f64, b: f64) -> f64 {
    let (mut a3, mut a2, mut a1, mut a0) = (0.0, 0.0, 0.0, 0.0);
    for i in 0..3 {
        let o = M_AB[i][0] * a + M_AB[i][1] * b;
        a3 += W[i];
        a2 += 3.0 * W[i] * o;
        a1 += 3.0 * W[i] * o * o;
        a0 += W[i] * (o * o * o);
    }

    largest_real_root(a2 / a3, a1 / a3, (a0 - target_y) / a3).clamp(0.0, 1.0)
}

/// Solve for the OKLCh L that produces a target CIE Y at given chroma and
/// hue. Each cone response is (L + o)³ with o fixed by (C, h), so
/// Y = Σ W·(L + o)³ is a cubic in L. Returns the largest real root, clamped
/// to [0, 1].
pub fn find_l_for_target_y(target_y: f64, chroma: f64, hue: f64) -> f64 {
    let (a, b) = chroma_hue_to_ab(chroma, hue);
    solve_l(target_y, a, b)
}

/// Linear-RGB tolerance for "fits in sRGB". Also the band `provably_empty` reasons about.
const EPS: f64 = 1e-4;
/// Bound on |d²channel/dc²| along constant-tone paths near the gamut edge (measured max 27.7).
const CURVATURE: f64 = 32.0;
/// Chroma intervals narrower than this are not searched for islands.
const MIN_WIDTH: f64 = 1e-6;
/// Bisection steps; the final bracket is `max_chroma / 2^OUTER_ITER` wide.
const OUTER_ITER: u32 = 25;

#[derive(Clone, Copy)]
struct Sample {
    c: f64,
    l: f64,
    r: f64,
    g: f64,
    b: f64,
    fits: bool,
}

struct Bracket {
    lo: f64,
    hi: Sample,
}

/// True when no chroma strictly between a and b (both infeasible) can fit: some
/// channel is outside the band on the same side at both ends by more than the
/// curvature bound lets it bend back over an interval this wide.
fn provably_empty(a: &Sample, b: &Sample) -> bool {
    let width = b.c - a.c;
    let bend = (CURVATURE * (width * width)) / 8.0;
    for (av, bv) in [(a.r, b.r), (a.g, b.g), (a.b, b.b)] {
        if -EPS - av > bend && -EPS - bv > bend {
            return true;
        }
        if av - 1.0 - EPS > bend && bv - 1.0 - EPS > bend {
            return true;
        }
    }
    false
}

/// The constant-Y path through chroma at one hue; (u, v) is the hue's unit vector.
struct Path {
    target_y: f64,
    u: f64,
    v: f64,
}

impl Path {
    #[inline]
    fn l_at(&self, c: f64) -> f64 {
        solve_l(self.target_y, c * self.u, c * self.v)
    }

    /// Linear RGB of the color at chroma c on the path, and whether it fits.
    #[inline]
    fn sample(&self, c: f64) -> Sample {
        let (a, b) = (c * self.u, c * self.v);
        let l = solve_l(self.target_y, a, b);
        let Lrgb { r, g, b } = lab_to_lrgb(l, a, b);
        Sample {
            c,
            l,
            r,
            g,
            b,
            fits: is_displayable(r, g, b, EPS),
        }
    }

    /// Bracket around the highest feasible chroma strictly inside (a, b), both infeasible, or None.
    fn top_inside(&self, a: &Sample, b: &Sample, tol: f64) -> Option<Bracket> {
        let width = b.c - a.c;
        // A NaN width must also stop the recursion.
        if width.is_nan() || width < MIN_WIDTH || provably_empty(a, b) {
            return None;
        }
        let m = self.sample((a.c + b.c) / 2.0);
        if m.fits {
            return Some(self.top_between(m.c, *b, tol));
        }
        self.top_inside(&m, b, tol)
            .or_else(|| self.top_inside(a, &m, tol))
    }

    /// Narrow [lo, hi) (lo feasible, hi infeasible) to width tol around the highest feasible chroma.
    fn top_between(&self, mut lo: f64, mut hi: Sample, tol: f64) -> Bracket {
        while hi.c - lo > tol {
            let m = self.sample((lo + hi.c) / 2.0);
            if m.fits {
                lo = m.c;
                continue;
            }
            if let Some(inner) = self.top_inside(&m, &hi, tol) {
                return inner;
            }
            hi = m;
        }
        Bracket { lo, hi }
    }
}

/// Find the highest OKLCh chroma at or below `max_chroma` that fits in sRGB
/// while hitting a target CIE Y exactly. Bisection on chroma, re-solving L
/// at every step, but an infeasible midpoint only discards the interval
/// above it once `provably_empty` rules out a hidden feasible island there.
/// Near the blue primary the constant-Y path grazes a gamut face and the
/// feasible set splits in two; plain bisection would land on either half
/// depending on `max_chroma`.
///
/// Typically 26–30 samples, one solve and one Oklab→RGB conversion each.
pub fn gamut_map_with_tone_preservation(target_y: f64, hue: f64, max_chroma: f64) -> Oklch {
    let (u, v) = hue_direction(hue);
    let path = Path { target_y, u, v };

    let top = path.sample(max_chroma);
    if top.fits {
        return Oklch {
            l: top.l,
            c: max_chroma,
            h: hue,
        };
    }

    let tol = max_chroma / f64::from(1u32 << OUTER_ITER);
    let mut lo = 0.0;
    let mut hi = top;
    let mut island: Option<Bracket> = None;
    for _ in 0..OUTER_ITER {
        let m = path.sample((lo + hi.c) / 2.0);
        if m.fits {
            lo = m.c;
            continue;
        }
        island = path.top_inside(&m, &hi, tol);
        if island.is_some() {
            break;
        }
        hi = m;
    }

    let c = match &island {
        Some(bracket) => (bracket.lo + bracket.hi.c) / 2.0,
        None => (lo + hi.c) / 2.0,
    };
    Oklch {
        l: path.l_at(c),
        c,
        h: hue,
    }
}
