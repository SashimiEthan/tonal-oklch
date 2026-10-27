//! Solvers for the hybrid OKLCh + CIE L* color space.
//!
//! CIE Y (≡ WCAG relative luminance) is the immutable constraint. At fixed
//! (C, h), Y is a cubic in OKLCh L, so L is solved in closed form. Gamut
//! mapping reduces chroma while re-solving L at every step, so tone is never
//! stale.

use crate::luminance::is_displayable;
use crate::oklab::{LMS2RGB, Lrgb, M_AB, Oklch, hue_direction, lab_to_lrgb};

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

/// Σ W[i]: the L³ coefficient of the luminance cubic.
const W_SUM: f64 = W[0] + W[1] + W[2];

/// Largest real root of the depressed cubic t³ + p·t + q = 0, plus `shift`.
/// `p_third` = p/3 and `p3_27` = p³/27 arrive precomputed so the discriminant
/// and the Cardano correction need no division on the dependent path.
#[inline]
fn largest_real_root(p: f64, p_third: f64, p3_27: f64, q: f64, shift: f64) -> f64 {
    let disc = (q * q) / 4.0 + p3_27;

    if disc > 0.0 {
        let s = disc.sqrt();
        // Cube-root only the larger-magnitude term and derive the other from it;
        // adding both cube roots cancels and drifts ~1e-7 at light tones.
        let u = (if q < 0.0 { -q / 2.0 + s } else { -q / 2.0 - s }).cbrt();
        return u - p_third / u + shift;
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

/// Solve for the OKLCh L that produces a target CIE Y at given chroma and
/// hue. Each cone response is (L + o)³ with o fixed by (C, h), so
/// Y = Σ W·(L + o)³ is a cubic in L. Returns the largest real root, clamped
/// to [0, 1].
pub fn find_l_for_target_y(target_y: f64, chroma: f64, hue: f64) -> f64 {
    Path::new(target_y, hue).l_at(chroma)
}

/// Linear-RGB tolerance for "fits in sRGB". Also the band `provably_empty` reasons about.
const EPS: f64 = 1e-4;
/// Bound on |d²channel/dc²| along constant-tone paths near the gamut edge (measured max 27.7).
const CURVATURE: f64 = 32.0;
/// Chroma intervals narrower than this are not searched for islands.
const MIN_WIDTH: f64 = 1e-6;
/// Bisection steps; the final bracket is `max_chroma / 2^OUTER_ITER` wide.
const OUTER_ITER: u32 = 25;
/// Overlapped bisection rounds before the boundary is predicted; see `Path::bisect_predicted`.
const PREDICT_ROUNDS: u32 = 3;
/// Predicted midpoints solved before the refinement sample lands.
const HEAD: usize = 8;

#[derive(Clone, Copy)]
struct Sample {
    c: f64,
    l: f64,
    r: f64,
    g: f64,
    b: f64,
    fits: bool,
}

/// The bracket's low end before any feasible sample: chroma 0, NaN channels.
const UNSAMPLED_LO: Sample = Sample {
    c: 0.0,
    l: f64::NAN,
    r: f64::NAN,
    g: f64::NAN,
    b: f64::NAN,
    fits: true,
};

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

/// The band edge `hi` is furthest outside: 0..3 below -EPS for r, g, b; 3..6 above 1+EPS.
#[inline(always)]
fn active_bound(hi: &Sample) -> usize {
    let v = [
        -EPS - hi.r,
        -EPS - hi.g,
        -EPS - hi.b,
        hi.r - 1.0 - EPS,
        hi.g - 1.0 - EPS,
        hi.b - 1.0 - EPS,
    ];
    let mut best = 0;
    for (i, x) in v.iter().enumerate().skip(1) {
        if *x > v[best] {
            best = i;
        }
    }
    best
}

/// How far `s` is outside band edge `bound`; the same expressions `provably_empty` tests.
#[inline(always)]
fn violation(s: &Sample, bound: usize) -> f64 {
    match bound {
        0 => -EPS - s.r,
        1 => -EPS - s.g,
        2 => -EPS - s.b,
        3 => s.r - 1.0 - EPS,
        4 => s.g - 1.0 - EPS,
        _ => s.b - 1.0 - EPS,
    }
}

/// Estimated boundary chroma strictly inside (lo, hi), or NaN: inverse
/// quadratic interpolation of the active channel through lo, hi and a third
/// nearby sample, falling back to the secant.
#[inline(always)]
fn predict_boundary(lo: &Sample, hi: &Sample, third: &Sample) -> f64 {
    let bound = active_bound(hi);
    let (fl, fh, ft) = (
        violation(lo, bound),
        violation(hi, bound),
        violation(third, bound),
    );
    let p = lo.c * fh * ft / ((fl - fh) * (fl - ft))
        + hi.c * fl * ft / ((fh - fl) * (fh - ft))
        + third.c * fl * fh / ((ft - fl) * (ft - fh));
    if p > lo.c && p < hi.c {
        return p;
    }
    let s = lo.c + (hi.c - lo.c) * fl / (fl - fh);
    if s > lo.c && s < hi.c { s } else { f64::NAN }
}

/// The constant-Y path through chroma at one hue; (u, v) is the hue's unit vector.
///
/// Each cone offset is c·k with k fixed by the hue, so the luminance cubic's
/// coefficients are polynomials in c and its depressed form is
/// t³ + (α·c²)·t + (β·c³ − γ) = 0 with L = t + σ·c. The constants are
/// computed once here; `l_at` is then a handful of multiplies plus the root.
struct Path {
    u: f64,
    v: f64,
    alpha: f64,
    alpha_third: f64,
    alpha3_27: f64,
    beta: f64,
    gamma: f64,
    sigma: f64,
}

impl Path {
    fn new(target_y: f64, hue: f64) -> Self {
        let (u, v) = hue_direction(hue);

        let (mut s1, mut s2, mut s3) = (0.0, 0.0, 0.0);
        for i in 0..3 {
            let k = M_AB[i][0] * u + M_AB[i][1] * v;
            s1 += W[i] * k;
            s2 += W[i] * k * k;
            s3 += W[i] * (k * k * k);
        }
        // Monic cubic L³ + B·L² + C·L + D with B = b·c, C = cc·c², D = d·c³ − y.
        let b = 3.0 * s1 / W_SUM;
        let cc = 3.0 * s2 / W_SUM;
        let d = s3 / W_SUM;
        let y = target_y / W_SUM;

        let alpha = cc - (b * b) / 3.0;
        Self {
            u,
            v,
            alpha,
            alpha_third: alpha / 3.0,
            alpha3_27: (alpha * alpha * alpha) / 27.0,
            beta: (2.0 * (b * b * b)) / 27.0 - (b * cc) / 3.0 + d,
            gamma: y,
            sigma: -b / 3.0,
        }
    }

    /// OKLCh L at chroma c on the path, clamped to [0, 1].
    #[inline]
    fn l_at(&self, c: f64) -> f64 {
        let c2 = c * c;
        let c3 = c2 * c;
        largest_real_root(
            self.alpha * c2,
            self.alpha_third * c2,
            self.alpha3_27 * (c3 * c3),
            self.beta * c3 - self.gamma,
            self.sigma * c,
        )
        .clamp(0.0, 1.0)
    }

    /// Linear RGB of the color at chroma c on the path, and whether it fits.
    // Out of line, `Sample` returns through memory and adds ~20% to the bisection loop.
    #[inline(always)]
    fn sample(&self, c: f64) -> Sample {
        self.finish(c, self.l_at(c))
    }

    /// The sample at chroma c once its L is known.
    #[inline(always)]
    fn finish(&self, c: f64, l: f64) -> Sample {
        let Lrgb { r, g, b } = lab_to_lrgb(l, c * self.u, c * self.v);
        Sample {
            c,
            l,
            r,
            g,
            b,
            fits: is_displayable(r, g, b, EPS),
        }
    }

    /// True when (a, b), both infeasible, need no island search.
    #[inline(always)]
    fn no_island(&self, a: &Sample, b: &Sample) -> bool {
        let width = b.c - a.c;
        // A NaN width must also stop the recursion.
        width.is_nan() || width < MIN_WIDTH || provably_empty(a, b)
    }

    /// Bracket around the highest feasible chroma strictly inside (a, b), both infeasible, or None.
    #[inline(always)]
    fn top_inside(&self, a: &Sample, b: &Sample, tol: f64) -> Option<Bracket> {
        if self.no_island(a, b) {
            return None;
        }
        self.search_inside(a, b, tol, None)
    }

    /// `top_inside` past its cheap rejection. `probe` is the sample at the
    /// midpoint of (a, b) when the caller already has it.
    // Rare path; inlining it bloats the bisection loop.
    #[inline(never)]
    fn search_inside(
        &self,
        a: &Sample,
        b: &Sample,
        tol: f64,
        probe: Option<&Sample>,
    ) -> Option<Bracket> {
        let m = match probe {
            Some(m) => *m,
            None => self.sample((a.c + b.c) / 2.0),
        };
        debug_assert_eq!(m.c, (a.c + b.c) / 2.0);
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
/// Typically 26–30 samples, one solve and one Oklab→RGB conversion each,
/// issued a few at a time and then as one predicted batch rather than one
/// by one; see `Path::bisect_predicted`.
pub fn gamut_map_with_tone_preservation(target_y: f64, hue: f64, max_chroma: f64) -> Oklch {
    let Sample { l, c, .. } = map_chroma(target_y, hue, max_chroma);
    Oklch { l, c, h: hue }
}

/// `gamut_map_with_tone_preservation` plus the mapped color's linear RGB,
/// which the final sample already holds.
pub(crate) fn gamut_map_to_lrgb(target_y: f64, hue: f64, max_chroma: f64) -> (Oklch, Lrgb) {
    let Sample { l, c, r, g, b, .. } = map_chroma(target_y, hue, max_chroma);
    (Oklch { l, c, h: hue }, Lrgb { r, g, b })
}

/// The sample at the mapped chroma. Inlined into each wrapper so the
/// bisection loop keeps its state in registers rather than a returned struct.
#[inline(always)]
fn map_chroma(target_y: f64, hue: f64, max_chroma: f64) -> Sample {
    let path = Path::new(target_y, hue);

    let top = path.sample(max_chroma);
    if top.fits {
        return top;
    }

    let tol = max_chroma / f64::from(1u32 << OUTER_ITER);
    path.bisect_predicted(top, tol)
}

impl Path {
    /// The reference: one bisection step at a time. `top` is the infeasible
    /// sample at `max_chroma`.
    #[cfg(test)]
    fn bisect_serial(&self, top: Sample, tol: f64) -> Sample {
        let mut lo = 0.0;
        let mut hi = top;
        let mut island: Option<Bracket> = None;
        for _ in 0..OUTER_ITER {
            let m = self.sample((lo + hi.c) / 2.0);
            if m.fits {
                lo = m.c;
                continue;
            }
            island = self.top_inside(&m, &hi, tol);
            if island.is_some() {
                break;
            }
            hi = m;
        }

        let c = match &island {
            Some(bracket) => (bracket.lo + bracket.hi.c) / 2.0,
            None => (lo + hi.c) / 2.0,
        };
        self.sample(c)
    }

    /// `bisect_serial`'s result with far fewer dependent steps. The first
    /// `PREDICT_ROUNDS` rounds sample the midpoint and both candidate next
    /// midpoints before either decision, so three solves overlap in flight.
    /// The boundary is then estimated from the channel `hi` violates most,
    /// every remaining midpoint along the predicted decision path is solved
    /// at once, and the decisions are checked against the serial loop's. A
    /// wrong one, or an island check the fast path cannot settle, replays
    /// exactly from the last verified bracket. Same points, same
    /// expressions, so the result is bit-identical.
    #[inline(always)]
    fn bisect_predicted(&self, top: Sample, tol: f64) -> Sample {
        const BATCH: usize = (OUTER_ITER - 2 * PREDICT_ROUNDS) as usize + 1;

        let mut lo = UNSAMPLED_LO;
        let mut hi = top;
        let mut third = top;
        for _ in 0..PREDICT_ROUNDS {
            let mc = (lo.c + hi.c) / 2.0;
            let m = self.sample(mc);
            let above = self.sample((mc + hi.c) / 2.0);
            let below = self.sample((lo.c + mc) / 2.0);

            if !m.fits && !self.no_island(&m, &hi) {
                if let Some(island) = self.search_inside(&m, &hi, tol, Some(&above)) {
                    return self.sample((island.lo + island.hi.c) / 2.0);
                }
            }
            let next = select(m.fits, &above, &below);
            let lo1 = select(m.fits, &m, &lo);
            let hi1 = select(m.fits, &hi, &m);
            if !next.fits {
                if let Some(island) = self.top_inside(&next, &hi1, tol) {
                    return self.sample((island.lo + island.hi.c) / 2.0);
                }
            }
            // The displaced endpoint nearest the new bracket, for interpolation.
            let lo_sampled = !lo.l.is_nan();
            third = if m.fits == next.fits || (!m.fits && !lo_sampled) {
                m
            } else if m.fits {
                hi
            } else {
                lo
            };
            lo = select(next.fits, &next, &lo1);
            hi = select(next.fits, &hi1, &next);
        }
        if lo.l.is_nan() {
            lo = self.sample(0.0);
        }

        let mut done = 2 * PREDICT_ROUNDS;
        loop {
            let remaining = (OUTER_ITER - done) as usize;
            // Prediction endpoints only; the bracket itself stays on dyadic midpoints.
            let (mut plo, mut phi) = (lo, hi);
            let mut c_pred = predict_boundary(&plo, &phi, &third);
            let valid = c_pred > plo.c && c_pred < phi.c;
            // One sample at the prediction sharpens it while the first HEAD
            // midpoints, predicted without it, are already in flight.
            let refine = self.sample(if valid { c_pred } else { lo.c });

            let mut cs = [0.0; BATCH];
            let mut ls = [0.0; BATCH];
            let mut predicted = 0u32;
            let (mut qlo, mut qhi) = (lo.c, hi.c);
            let mut predict = |i: usize, c_pred: f64| {
                let m = (qlo + qhi) / 2.0;
                cs[i] = m;
                ls[i] = self.l_at(m);
                let fits = m <= c_pred;
                predicted |= u32::from(fits) << i;
                qlo = if fits { m } else { qlo };
                qhi = if fits { qhi } else { m };
            };
            let head = HEAD.min(remaining + 1);
            for i in 0..head {
                predict(i, c_pred);
            }
            if valid {
                if refine.fits {
                    third = plo;
                    plo = refine;
                } else {
                    third = phi;
                    phi = refine;
                }
                c_pred = predict_boundary(&plo, &phi, &third);
            }
            for i in head..=remaining {
                predict(i, c_pred);
            }

            // Island checks on the channel `hi` violates most only: passing
            // one clause of `provably_empty` implies `no_island`; failing
            // defers to the exact replay below.
            let bound = active_bound(&hi);
            let mut actual = 0u32;
            let mut no_search = true;
            let (mut prev_c, mut prev_v) = (hi.c, violation(&hi, bound));
            for i in 0..remaining {
                let s = self.finish(cs[i], ls[i]);
                actual |= u32::from(s.fits) << i;
                let v = violation(&s, bound);
                let width = prev_c - s.c;
                let bend = (CURVATURE * (width * width)) / 8.0;
                no_search &= s.fits | ((v > bend) & (prev_v > bend));
                prev_c = if s.fits { prev_c } else { s.c };
                prev_v = if s.fits { prev_v } else { v };
            }

            // The serial loop takes every step up to and including the first
            // decision the prediction got wrong.
            let mismatch = (actual ^ predicted) & ((1u32 << remaining) - 1);
            if mismatch == 0 && no_search {
                return self.finish(cs[remaining], ls[remaining]);
            }
            let steps = if mismatch == 0 {
                remaining
            } else {
                mismatch.trailing_zeros() as usize + 1
            };
            for j in 0..steps {
                let m = self.finish(cs[j], ls[j]);
                debug_assert_eq!(m.c, (lo.c + hi.c) / 2.0);
                if m.fits {
                    third = lo;
                    lo = m;
                } else {
                    if !self.no_island(&m, &hi) {
                        if let Some(island) = self.search_inside(&m, &hi, tol, None) {
                            return self.sample((island.lo + island.hi.c) / 2.0);
                        }
                    }
                    third = hi;
                    hi = m;
                }
            }
            done += steps as u32;
        }
    }
}

/// `a` if `cond`, else `b`. Conditional moves, not a branch: the decision
/// is unpredictable, and a mispredict would discard the next round's
/// samples in flight.
#[inline(always)]
fn select(cond: bool, a: &Sample, b: &Sample) -> Sample {
    Sample {
        c: if cond { a.c } else { b.c },
        l: if cond { a.l } else { b.l },
        r: if cond { a.r } else { b.r },
        g: if cond { a.g } else { b.g },
        b: if cond { a.b } else { b.b },
        fits: if cond { a.fits } else { b.fits },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::luminance::cie_lstar_to_y;

    fn bits(s: &Sample) -> ([u64; 5], bool) {
        (
            [
                s.c.to_bits(),
                s.l.to_bits(),
                s.r.to_bits(),
                s.g.to_bits(),
                s.b.to_bits(),
            ],
            s.fits,
        )
    }

    #[test]
    fn predicted_bisection_matches_serial() {
        let mut hues: Vec<f64> = (0..360).step_by(3).map(f64::from).collect();
        // The hues where the feasible set splits into islands.
        hues.extend((2620..=2660).map(|h| f64::from(h) / 10.0));
        let tones = [
            0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 35.0, 50.0, 65.0, 80.0, 90.0, 95.0, 98.0, 99.5,
        ];
        let chromas = [0.02, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.37, 0.4];

        let mut checked = 0;
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rand = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut cases: Vec<(f64, f64, f64)> = Vec::new();
        for &tone in &tones {
            for &chroma in &chromas {
                for &hue in &hues {
                    cases.push((tone, chroma, hue));
                }
            }
        }
        for _ in 0..4000 {
            cases.push((0.5 + 99.0 * rand(), 0.4 * rand(), 360.0 * rand()));
        }

        for (tone, chroma, hue) in cases {
            let path = Path::new(cie_lstar_to_y(tone), hue);
            let top = path.sample(chroma);
            if top.fits {
                continue;
            }
            let tol = chroma / f64::from(1u32 << OUTER_ITER);
            let serial = path.bisect_serial(top, tol);
            let predicted = path.bisect_predicted(top, tol);
            assert_eq!(
                bits(&serial),
                bits(&predicted),
                "tone {tone} chroma {chroma} hue {hue}"
            );
            checked += 1;
        }
        assert!(checked > 5000, "only {checked} mapped colors checked");
    }
}
