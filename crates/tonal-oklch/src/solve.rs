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
/// No sRGB color has chroma this high (the gamut peaks at 0.3226, magenta
/// corner, EPS included), so no island starts above it. Also the only stop for
/// absurd requests (≳ 1e150), whose NaN samples `provably_empty` cannot rule out.
const MAX_FEASIBLE_CHROMA: f64 = 0.5;
/// Bisection steps; the final bracket is `max_chroma / 2^OUTER_ITER` wide.
const OUTER_ITER: u32 = 25;
/// Levels left after the two exact opening levels.
const MAX_REM: usize = OUTER_ITER as usize - 2;
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

/// Debug-assert equality that a NaN request (which propagates to every chroma) does not trip.
fn same_c(a: f64, b: f64) -> bool {
    a == b || (a.is_nan() && b.is_nan())
}

/// True when no chroma strictly between a and b (both infeasible) can fit: some
/// channel is outside the band on the same side at both ends by more than the
/// curvature bound lets it bend back over an interval this wide.
#[inline(always)]
fn provably_empty(a: &Sample, b: &Sample) -> bool {
    let width = b.c - a.c;
    let bend = (CURVATURE * (width * width)) / 8.0;
    let mut any = false;
    for (av, bv) in [(a.r, b.r), (a.g, b.g), (a.b, b.b)] {
        any |= (-EPS - av > bend) & (-EPS - bv > bend);
        any |= (av - 1.0 - EPS > bend) & (bv - 1.0 - EPS > bend);
    }
    any
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
    b: f64,
    cc: f64,
    d: f64,
    k: [f64; 3],
}

impl Path {
    fn new(target_y: f64, hue: f64) -> Self {
        let (u, v) = hue_direction(hue);

        let mut k = [0.0; 3];
        let (mut s1, mut s2, mut s3) = (0.0, 0.0, 0.0);
        for i in 0..3 {
            k[i] = M_AB[i][0] * u + M_AB[i][1] * v;
            s1 += W[i] * k[i];
            s2 += W[i] * k[i] * k[i];
            s3 += W[i] * (k[i] * k[i] * k[i]);
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
            b,
            cc,
            d,
            k,
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
        (a.c >= MAX_FEASIBLE_CHROMA) | width.is_nan() | (width < MIN_WIDTH) | provably_empty(a, b)
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
        debug_assert!(same_c(m.c, (a.c + b.c) / 2.0));
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

    // The first two bisection levels' candidates and gray depend only on
    // max_chroma, so their solves are issued with the top sample and overlap
    // it; `black_box` keeps them ahead of the branch. Wasted when top fits.
    let c4 = (0.0 + max_chroma) / 2.0;
    let c2 = (0.0 + c4) / 2.0;
    let c6 = (c4 + max_chroma) / 2.0;
    let l_top = path.l_at(max_chroma);
    let l4 = core::hint::black_box(path.l_at(c4));
    let l2 = core::hint::black_box(path.l_at(c2));
    let l6 = core::hint::black_box(path.l_at(c6));
    let l0 = core::hint::black_box(path.l_at(0.0));
    let top = path.finish(max_chroma, l_top);
    if top.fits {
        return top;
    }
    if target_y.is_nan() {
        // Every sample is NaN: nothing fits and nothing is provably empty, so the
        // island search would walk ~2^17 sub-intervals (20 ms) to end where plain
        // bisection ends.
        let mut c = max_chroma;
        for _ in 0..=OUTER_ITER {
            c = (0.0 + c) / 2.0;
        }
        return path.sample(c);
    }
    let opening = Opening {
        top,
        q4: path.finish(c4, l4),
        q2: path.finish(c2, l2),
        q6: path.finish(c6, l6),
        gray: path.finish(0.0, l0),
    };
    path.bisect(&opening)
}

/// The top sample, the two exact opening levels' candidates and gray.
struct Opening {
    top: Sample,
    q4: Sample,
    q2: Sample,
    q6: Sample,
    gray: Sample,
}

impl Path {
    /// `bisect_serial`'s result from an opening whose `top` does not fit.
    #[inline(always)]
    fn bisect(&self, o: &Opening) -> Sample {
        debug_assert!(!o.top.fits);
        debug_assert!(same_c(o.q4.c, (0.0 + o.top.c) / 2.0));
        debug_assert!(same_c(o.q2.c, (0.0 + o.q4.c) / 2.0));
        debug_assert!(same_c(o.q6.c, (o.q4.c + o.top.c) / 2.0));
        debug_assert!(same_c(o.gray.c, 0.0));
        let tol = o.top.c / f64::from(1u32 << OUTER_ITER);
        match self.bisect_certified(o, tol) {
            Some(s) => s,
            None => self.bisect_predicted(o, tol),
        }
    }
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

    /// `bisect_serial`'s result with far fewer dependent steps; the fallback
    /// when `bisect_certified` declines. The first `PREDICT_ROUNDS` rounds
    /// sample the midpoint and both candidate next midpoints before either
    /// decision, so three solves overlap in flight (the opening already
    /// holds the first round's).
    /// The boundary is then estimated from the channel `hi` violates most,
    /// every remaining midpoint along the predicted decision path is solved
    /// at once, and the decisions are checked against the serial loop's. A
    /// wrong one, or an island check the fast path cannot settle, replays
    /// exactly from the last verified bracket. Same points, same
    /// expressions, so the result is bit-identical.
    #[inline(always)]
    fn bisect_predicted(&self, o: &Opening, tol: f64) -> Sample {
        const BATCH: usize = (OUTER_ITER - 2 * PREDICT_ROUNDS) as usize + 1;

        let top = o.top;
        let mut lo = UNSAMPLED_LO;
        let mut hi = top;
        let mut third = top;
        for round in 0..PREDICT_ROUNDS {
            let mc = (lo.c + hi.c) / 2.0;
            let (m, above, below) = if round == 0 {
                (o.q4, o.q6, o.q2)
            } else {
                (
                    self.sample(mc),
                    self.sample((mc + hi.c) / 2.0),
                    self.sample((lo.c + mc) / 2.0),
                )
            };

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
            lo = o.gray;
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
                debug_assert!(same_c(m.c, (lo.c + hi.c) / 2.0));
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

/// Bound on |d²channel/dc²| used by certificates; surveyed max 43.2 on
/// certifiable paths with top chroma ≤ `CERT_MAX_TOP`.
const CERT_CURVATURE: f64 = 64.0;
const CERT_MAX_TOP: f64 = 0.5;
/// Covers the solver's rounding error in a channel value (measured ≤ 1e-15).
const CERT_SLACK: f64 = 1e-11;

/// An exact sample with the channel slopes along the path.
#[derive(Clone, Copy)]
struct Anchor {
    c: f64,
    ch: [f64; 3],
    d1: [f64; 3],
    fits: bool,
}

impl Anchor {
    /// Violation and slope of band edge `e` (0..3 below -EPS, 3..6 above 1+EPS).
    #[inline(always)]
    fn edge(&self, e: usize) -> (f64, f64) {
        let j = e % 3;
        if e < 3 {
            (-EPS - self.ch[j], -self.d1[j])
        } else {
            (self.ch[j] - 1.0 - EPS, self.d1[j])
        }
    }

    /// The edge violated most (or least inside).
    #[inline(always)]
    fn active(&self) -> usize {
        let mut best = 0;
        let mut bv = self.edge(0).0;
        for e in 1..6 {
            let v = self.edge(e).0;
            if v > bv {
                bv = v;
                best = e;
            }
        }
        best
    }
}

impl Path {
    /// The t = c/L in (t_lo, t_hi) where edge `e` is crossed, or NaN when
    /// the solve lands outside. In t every channel is L³·Q(t) and Y is
    /// L³·W_SUM·P(t), so "channel = edge" is the cubic
    /// gamma·Q(t) − edge·P(t) = 0, independent of L.
    #[inline(always)]
    fn edge_crossing(&self, e: usize, t_lo: f64, t_hi: f64) -> f64 {
        let j = e % 3;
        let ev = if e < 3 { -EPS } else { 1.0 + EPS };
        let r = LMS2RGB[j];
        let (mut r0, mut r1, mut r2, mut r3) = (0.0, 0.0, 0.0, 0.0);
        for (&ri, &k) in r.iter().zip(&self.k) {
            r0 += ri;
            r1 += ri * k;
            r2 += ri * (k * k);
            r3 += ri * (k * k * k);
        }
        let y = self.gamma;
        let f0 = y * r0 - ev;
        let f1 = 3.0 * y * r1 - ev * self.b;
        let f2 = 3.0 * y * r2 - ev * self.cc;
        let f3 = y * r3 - ev * self.d;
        let f = |t: f64| ((f3 * t + f2) * t + f1) * t + f0;
        let fp = |t: f64| (3.0 * f3 * t + 2.0 * f2) * t + f1;
        // Two Halley steps from the secant start; the anchor it yields is
        // exact wherever it lands, so a slow case only costs prediction quality.
        let (flo, fhi) = (f(t_lo), f(t_hi));
        let mut t = t_lo + (t_hi - t_lo) * (flo / (flo - fhi));
        for _ in 0..2 {
            let v = f(t);
            let d1 = fp(t);
            let d2 = 6.0 * f3 * t + 2.0 * f2;
            t -= (2.0 * v * d1) / (2.0 * d1 * d1 - v * d2);
        }
        if t > t_lo && t < t_hi { t } else { f64::NAN }
    }

    /// Chroma and lightness of the path point with t = c/L.
    #[inline(always)]
    fn c_of_t(&self, t: f64) -> (f64, f64) {
        let p = 1.0 + t * (self.b + t * (self.cc + t * self.d));
        let l = (self.gamma / p).cbrt();
        (t * l, l)
    }

    /// Whether certificates are valid on [0, top]: the solved L is a single
    /// analytic branch (q < 0 keeps the largest root simple), never clamped,
    /// and the curvature bound was surveyed up to this chroma.
    #[inline(always)]
    #[allow(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "a NaN must fail each check"
    )]
    fn certifiable(&self, top: f64) -> bool {
        if !(top <= CERT_MAX_TOP) {
            return false;
        }
        let top3 = top * top * top;
        if !(self.beta * top3 - self.gamma < -1e-12) {
            return false;
        }
        if self.d > 0.0 && !(self.d * top3 < self.gamma - 1e-9) {
            return false;
        }
        // L < 1 ⟺ G(c) = 1 − y + b c + cc c² + d c³ > 0; check the ends and interior extrema.
        let g = |c: f64| 1.0 - self.gamma + self.b * c + self.cc * c * c + self.d * c * c * c;
        let mut gmin = g(top).min(1.0 - self.gamma);
        let (a2, a1, a0) = (3.0 * self.d, 2.0 * self.cc, self.b);
        let disc = a1 * a1 - 4.0 * a2 * a0;
        if a2 != 0.0 && disc >= 0.0 {
            let sq = disc.sqrt();
            for r in [(-a1 + sq) / (2.0 * a2), (-a1 - sq) / (2.0 * a2)] {
                if r > 0.0 && r < top {
                    gmin = gmin.min(g(r));
                }
            }
        } else if a2 == 0.0 && a1 != 0.0 {
            let r = -a0 / a1;
            if r > 0.0 && r < top {
                gmin = gmin.min(g(r));
            }
        }
        gmin > 1e-9
    }

    /// Channel slopes at an exact sample: dL/dc from the implicit cubic, then the chain rule.
    #[inline(always)]
    fn anchor(&self, s: &Sample) -> Anchor {
        let (l, c) = (s.l, s.c);
        let f_l = 3.0 * l * l + 2.0 * self.b * c * l + self.cc * c * c;
        let f_c = self.b * l * l + 2.0 * self.cc * c * l + 3.0 * self.d * c * c;
        let l1 = -f_c / f_l;
        let cube1: [f64; 3] = core::array::from_fn(|i| {
            let lms = l + self.k[i] * c;
            3.0 * lms * lms * (l1 + self.k[i])
        });
        let d1: [f64; 3] = core::array::from_fn(|j| {
            LMS2RGB[j][0] * cube1[0] + LMS2RGB[j][1] * cube1[1] + LMS2RGB[j][2] * cube1[2]
        });
        Anchor {
            c,
            ch: [s.r, s.g, s.b],
            d1,
            fits: s.fits,
        }
    }

    /// Edge lower bounds at chroma `m` certified from `a`.
    #[inline(always)]
    fn certify(&self, a: &Anchor, m: f64) -> [f64; 6] {
        let d = m - a.c;
        let err = (CERT_CURVATURE * (d * d)) / 2.0 + CERT_SLACK;
        let t = [
            a.ch[0] + a.d1[0] * d,
            a.ch[1] + a.d1[1] * d,
            a.ch[2] + a.d1[2] * d,
        ];
        [
            (-EPS - t[0]) - err,
            (-EPS - t[1]) - err,
            (-EPS - t[2]) - err,
            (t[0] - 1.0 - EPS) - err,
            (t[1] - 1.0 - EPS) - err,
            (t[2] - 1.0 - EPS) - err,
        ]
    }

    /// Violations of the six band edges at an exact sample.
    #[inline(always)]
    fn violations(s: &Sample) -> [f64; 6] {
        [
            -EPS - s.r,
            -EPS - s.g,
            -EPS - s.b,
            s.r - 1.0 - EPS,
            s.g - 1.0 - EPS,
            s.b - 1.0 - EPS,
        ]
    }

    /// No chroma strictly between two infeasible points fits: some edge's
    /// violation, known to be at least `vm` and `vh` at the ends, stays above
    /// the chord bound over an interval `width` wide.
    #[inline(always)]
    fn empty_between(vm: &[f64; 6], vh: &[f64; 6], width: f64) -> bool {
        let bend = (CERT_CURVATURE * (width * width)) / 8.0;
        let mut any = false;
        for e in 0..6 {
            any |= (vm[e] > bend) & (vh[e] > bend);
        }
        any
    }

    /// Newton step from an anchor to the crossing of the edge expected to
    /// bind: the nearest edge rising above it when it fits, else the edge it
    /// violates most. Returns (edge, chroma).
    #[inline(always)]
    fn newton(a: &Anchor) -> (usize, f64) {
        if a.fits {
            let mut r = f64::INFINITY;
            let mut best = 0;
            for e in 0..6 {
                let (v, dv) = a.edge(e);
                let root = a.c - v / dv;
                let better = dv > 0.0 && root < r;
                r = if better { root } else { r };
                best = if better { e } else { best };
            }
            (best, r)
        } else {
            let e = a.active();
            let (v, dv) = a.edge(e);
            (e, a.c - v / dv)
        }
    }

    /// Predicted decisions for the remaining levels: the binary digits of
    /// `c`'s position in the bracket, level 0 in bit 0. `inv_w` is the
    /// bracket width's reciprocal; the estimate need not be exact.
    #[inline(always)]
    fn pred_bits(c: f64, lo_c: f64, inv_w: f64, rem: usize) -> u32 {
        let x = (c - lo_c) * inv_w;
        let digits = ((x * f64::from(1u32 << rem)) as u32).min((1u32 << rem) - 1);
        digits.reverse_bits() >> (32 - rem)
    }

    #[inline(always)]
    fn chain_init(
        cs: &mut [f64; MAX_REM + 1],
        qlos: &mut [f64; MAX_REM + 1],
        qhis: &mut [f64; MAX_REM + 1],
        lo_c: f64,
        hi_c: f64,
    ) {
        cs[0] = (lo_c + hi_c) / 2.0;
        qlos[0] = lo_c;
        qhis[0] = hi_c;
    }

    /// Midpoints and brackets of the predicted path from level `from`, whose
    /// entries must already hold. `m.mul_add(0.5, other * 0.5)` rounds like
    /// `(m + other) / 2.0` but is a software libcall without a hardware FMA.
    #[inline(always)]
    fn chain(
        cs: &mut [f64; MAX_REM + 1],
        qlos: &mut [f64; MAX_REM + 1],
        qhis: &mut [f64; MAX_REM + 1],
        pred: u32,
        from: usize,
        rem: usize,
    ) {
        let (mut qlo, mut qhi, mut m) = (qlos[from], qhis[from], cs[from]);
        for i in from..rem {
            cs[i] = m;
            qlos[i] = qlo;
            qhis[i] = qhi;
            let f = (pred >> i) & 1 == 1;
            let other = if f { qhi } else { qlo };
            qlo = if f { m } else { qlo };
            qhi = if f { qhi } else { m };
            m = if cfg!(any(target_arch = "aarch64", target_feature = "fma")) {
                m.mul_add(0.5, other * 0.5)
            } else {
                (m + other) / 2.0
            };
        }
        cs[rem] = m;
        qlos[rem] = qlo;
        qhis[rem] = qhi;
    }

    /// Chroma interval on which `a` certifies every channel inside the band.
    /// Empty (lo > hi) when some channel can't be certified anywhere.
    #[inline(always)]
    #[allow(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "a NaN discriminant must count as bad"
    )]
    fn fit_interval(&self, a: &Anchor) -> (f64, f64) {
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let mut bad = false;
        let k2 = CERT_CURVATURE / 2.0;
        for j in 0..3 {
            let (ch, d1) = (a.ch[j], a.d1[j]);
            // Below: ch + d1·d − k2·d² − slack ≥ −EPS.
            let disc = d1 * d1 + 4.0 * k2 * (ch + EPS - CERT_SLACK);
            bad |= !(disc >= 0.0);
            let r = disc.sqrt();
            lo = lo.max(a.c + (d1 - r) / (2.0 * k2));
            hi = hi.min(a.c + (d1 + r) / (2.0 * k2));
            // Above: ch + d1·d + k2·d² + slack ≤ 1 + EPS.
            let disc = d1 * d1 - 4.0 * k2 * (ch - 1.0 - EPS + CERT_SLACK);
            bad |= !(disc >= 0.0);
            let r = disc.sqrt();
            lo = lo.max(a.c + (-d1 - r) / (2.0 * k2));
            hi = hi.min(a.c + (-d1 + r) / (2.0 * k2));
        }
        if bad {
            return (f64::INFINITY, f64::NEG_INFINITY);
        }
        (lo, hi)
    }

    /// Open chroma interval on which `a` certifies edge `e` violated.
    #[inline(always)]
    #[allow(
        clippy::neg_cmp_op_on_partial_ord,
        reason = "a NaN discriminant must give an empty interval"
    )]
    fn nofit_interval(&self, a: &Anchor, e: usize) -> (f64, f64) {
        let (v, dv) = a.edge(e);
        let k2 = CERT_CURVATURE / 2.0;
        let disc = dv * dv + 4.0 * k2 * (v - CERT_SLACK);
        if !(disc > 0.0) {
            return (f64::INFINITY, f64::NEG_INFINITY);
        }
        let r = disc.sqrt();
        (a.c + (dv - r) / (2.0 * k2), a.c + (dv + r) / (2.0 * k2))
    }

    /// Lower bound of edge `e`'s violation at chroma `m`, certified from `a`.
    #[inline(always)]
    fn edge_lb(&self, a: &Anchor, e: usize, m: f64) -> f64 {
        let d = m - a.c;
        let err = (CERT_CURVATURE / 2.0) * (d * d) + CERT_SLACK;
        let j = e % 3;
        let t = a.ch[j] + a.d1[j] * d;
        if e < 3 {
            (-EPS - t) - err
        } else {
            (t - 1.0 - EPS) - err
        }
    }

    /// `bisect_serial`'s result from one exact anchor near the boundary, or
    /// None when the path is not certifiable. The opening decides levels 0
    /// and 1 exactly; the closed-form crossing of the edge `hi` violates
    /// most, solved in t = c/L, gives an exact anchor with channel slopes,
    /// and a Taylor bound from it certifies each remaining midpoint of the
    /// predicted path. Only undecided midpoints are solved exactly, in one
    /// batch with the final sample. A wrong prediction re-anchors on the
    /// sample that exposed it and predicts again from there.
    #[inline(always)]
    fn bisect_certified(&self, o: &Opening, tol: f64) -> Option<Sample> {
        /// A first anchor further than this from its Newton root is refined by a second sample.
        const REFINE_TAU: f64 = 2e-6;
        const OPEN_LEVELS: usize = 2;

        let Opening {
            top,
            q4,
            q2,
            q6,
            gray,
        } = *o;

        // Levels 0 and 1 exactly. Their island checks are deferred past the
        // next sample so their own samples overlap it; the bracket below
        // assumes no island, and a found island returns directly.
        let lo0 = UNSAMPLED_LO;
        let lo1 = select(q4.fits, &q4, &lo0);
        let hi1 = select(q4.fits, &top, &q4);
        let m1 = select(q4.fits, &q6, &q2);
        let lo = select(m1.fits, &m1, &lo1);
        let hi = select(m1.fits, &hi1, &m1);
        let check0 = !q4.fits && !self.no_island(&q4, &top);
        let check1 = !m1.fits && !self.no_island(&m1, &hi1);
        // Certificates need the solved L analytic and unclamped, but only
        // on the bracket the exact levels left.
        if !self.certifiable(hi.c) {
            return None;
        }

        // Locate the crossing between the bracket ends.
        let lo_s = select(lo.l.is_nan(), &gray, &lo);
        let t_lo = lo_s.c / lo_s.l;
        let t_hi = hi.c / hi.l;
        let ex = active_bound(&hi);
        let tx = self.edge_crossing(ex, t_lo, t_hi);
        let (cx, lx) = self.c_of_t(tx);
        let crossed = cx > lo.c && cx < hi.c;
        let (c1, s1) = if crossed {
            (cx, self.finish(cx, lx))
        } else {
            let c = (lo.c + hi.c) / 2.0;
            (c, self.sample(c))
        };
        // Speculative path from c1 while s1 is in flight; later estimates
        // only redo the levels below the first digit that changes.
        let mut cs = [0.0f64; MAX_REM + 1];
        let mut qlos = [0.0f64; MAX_REM + 1];
        let mut qhis = [0.0f64; MAX_REM + 1];
        let rem0 = OUTER_ITER as usize - OPEN_LEVELS;
        let mut inv_w = 1.0 / (hi.c - lo.c);
        Self::chain_init(&mut cs, &mut qlos, &mut qhis, lo.c, hi.c);
        let mut chain_pred = Self::pred_bits(c1, lo.c, inv_w, rem0);
        Self::chain(&mut cs, &mut qlos, &mut qhis, chain_pred, 0, rem0);
        let mut have_chain = true;

        if check0 {
            if let Some(island) = self.search_inside(&q4, &top, tol, Some(&q6)) {
                return Some(self.sample((island.lo + island.hi.c) / 2.0));
            }
        }
        if check1 {
            if let Some(island) = self.search_inside(&m1, &hi1, tol, None) {
                return Some(self.sample((island.lo + island.hi.c) / 2.0));
            }
        }

        let mut a = self.anchor(&s1);
        // Newton only when the anchor is not a clean root of `ex` alone.
        let mut other = false;
        for k in 0..6 {
            other |= (k != ex) & (a.edge(k).0 > 0.0);
        }
        let short = a.edge(ex).0.abs() > 1e-9;
        let (mut estar, mut c2) = (ex, c1);
        if other | short | !crossed {
            let (e, c) = Self::newton(&a);
            estar = e;
            c2 = if c > lo.c && c < hi.c { c } else { c1 };
        }
        if (c2 - c1).abs() > REFINE_TAU {
            let s2 = self.sample(c2);
            let p2 = Self::pred_bits(c2, lo.c, inv_w, rem0);
            let from = ((chain_pred ^ p2).trailing_zeros() as usize).min(rem0);
            if from < rem0 {
                Self::chain(&mut cs, &mut qlos, &mut qhis, p2, from, rem0);
            }
            chain_pred = p2;
            let a2 = self.anchor(&s2);
            let (e2, c3) = Self::newton(&a2);
            if c3 > lo.c && c3 < hi.c {
                a = a2;
                estar = e2;
                c2 = c3;
            }
        }

        let (mut lo_c, mut hi_c) = (lo.c, hi.c);
        let mut hi_exact: Option<Sample> = Some(hi);
        let mut done = OPEN_LEVELS;
        let k8 = CERT_CURVATURE / 8.0;
        'cycle: loop {
            let rem = OUTER_ITER as usize - done;
            let (jstar, sgn, off) = if estar < 3 {
                (estar, -1.0, -EPS)
            } else {
                (estar - 3, 1.0, -1.0 - EPS)
            };
            let (hi_v, hi_cert) = match hi_exact {
                Some(h) => (sgn * [h.r, h.g, h.b][jstar] + off, false),
                None => {
                    let lb = self.edge_lb(&a, estar, hi_c);
                    (lb, lb > 0.0)
                }
            };

            // Predicted path: the decisions are the binary digits of c2's
            // position in the bracket, so the midpoints follow by one average
            // each (bit-identical to serial's (lo + hi) / 2).
            let pred = Self::pred_bits(c2, lo_c, inv_w, rem);
            let from = if have_chain {
                ((chain_pred ^ pred).trailing_zeros() as usize).min(rem)
            } else {
                Self::chain_init(&mut cs, &mut qlos, &mut qhis, lo_c, hi_c);
                0
            };
            if from < rem {
                Self::chain(&mut cs, &mut qlos, &mut qhis, pred, from, rem);
            }
            chain_pred = pred;
            debug_assert!(same_c(cs[rem], (qlos[rem] + qhis[rem]) / 2.0));
            let mut ls = [0.0f64; MAX_REM + 1];
            ls[rem] = self.l_at(cs[rem]);

            // Each certificate is a quadratic in chroma, so it holds on an
            // interval. An infeasible level also needs its bracket's hi
            // certified on the same edge (concavity, or the chord bound
            // against the exact hi at the first one) unless it is too narrow
            // to hold an island; a carry through the fit levels between two
            // infeasible ones finds the ones whose hi is uncertified.
            let (f_lo, f_hi) = self.fit_interval(&a);
            let (n_lo, n_hi) = self.nofit_interval(&a, estar);
            // Fixed trip count so the loop unrolls; entries past `rem` are
            // stale and masked off below.
            let (mut inf, mut inn, mut small) = (0u32, 0u32, 0u32);
            for i in 0..MAX_REM {
                let m = cs[i];
                inf |= u32::from((m >= f_lo) & (m <= f_hi)) << i;
                inn |= u32::from((m > n_lo) & (m < n_hi)) << i;
                small |= u32::from(qhis[i] - m < MIN_WIDTH) << i;
            }
            let all = (1u32 << rem) - 1;
            inf &= all;
            inn &= all;
            small &= all;
            let f_mask = pred & all;
            let n_mask = !pred & all;
            let above = ((n_mask & !inn) << 1) | u32::from(!hi_cert);
            let blocked =
                ((f_mask.wrapping_add(above & f_mask)) & !f_mask & all) | (above & n_mask);
            let mut settled = (f_mask & inf) | (n_mask & inn & (small | !blocked));
            let i0 = pred.trailing_ones() as usize;
            if i0 < rem && !hi_cert {
                let m0 = cs[i0];
                let in_n = (inn >> i0) & 1 == 1;
                let width = hi_c - m0;
                let bend = k8 * (width * width);
                let nofit_lb = self.edge_lb(&a, estar, m0);
                let ok = in_n & ((width < MIN_WIDTH) | ((nofit_lb > bend) & (hi_v > bend)));
                settled |= u32::from(ok) << i0;
            }

            // Solve the undecided midpoints and the final one together.
            let unsettled = !settled & all;
            let mut bits = unsettled;
            while bits != 0 {
                let i = bits.trailing_zeros() as usize;
                ls[i] = self.l_at(cs[i]);
                bits &= bits - 1;
            }

            // Check the exact levels in order; everything else is certified.
            let mut bits = unsettled;
            while bits != 0 {
                let i = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                let s = self.finish(cs[i], ls[i]);
                let f = (pred >> i) & 1 == 1;
                // The bracket at this level, from the decisions before it.
                let below = (1u32 << i) - 1;
                let nb = !pred & below;
                let lo_i = qlos[i];
                let hi_i = qhis[i];
                let hi_i_exact = if nb == 0 {
                    hi_exact
                } else {
                    let j = 31 - nb.leading_zeros() as usize;
                    let exact = (unsettled >> j) & 1 == 1;
                    if exact {
                        Some(self.finish(cs[j], ls[j]))
                    } else {
                        None
                    }
                };
                debug_assert!(same_c(s.c, (lo_i + hi_i) / 2.0));
                if !s.fits {
                    let width = hi_i - s.c;
                    if width >= MIN_WIDTH {
                        let h = match hi_i_exact {
                            Some(h) => Some(h),
                            None => {
                                let lb = self.certify(&a, hi_i);
                                if Self::empty_between(&Self::violations(&s), &lb, width) {
                                    None
                                } else {
                                    Some(self.sample(hi_i))
                                }
                            }
                        };
                        if let Some(h) = h {
                            if !self.no_island(&s, &h) {
                                if let Some(island) = self.search_inside(&s, &h, tol, None) {
                                    return Some(self.sample((island.lo + island.hi.c) / 2.0));
                                }
                            }
                        }
                    }
                }
                if s.fits != f {
                    // Wrong prediction: take this level exactly and predict again from here.
                    lo_c = if s.fits { s.c } else { lo_i };
                    hi_c = if s.fits { hi_i } else { s.c };
                    hi_exact = if s.fits { hi_i_exact } else { Some(s) };
                    inv_w = 1.0 / (hi_c - lo_c);
                    done += i + 1;
                    have_chain = false;
                    if done == OUTER_ITER as usize {
                        return Some(self.sample((lo_c + hi_c) / 2.0));
                    }
                    a = self.anchor(&s);
                    let (e, c) = Self::newton(&a);
                    estar = e;
                    c2 = if c > lo_c && c < hi_c {
                        c
                    } else {
                        (lo_c + hi_c) / 2.0
                    };
                    continue 'cycle;
                }
            }
            return Some(self.finish(cs[rem], ls[rem]));
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

    /// Bit-for-bit equality, with every NaN counting as the same NaN.
    fn same_sample(a: &Sample, b: &Sample) -> bool {
        let same = |x: f64, y: f64| x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan());
        same(a.c, b.c)
            && same(a.l, b.l)
            && same(a.r, b.r)
            && same(a.g, b.g)
            && same(a.b, b.b)
            && a.fits == b.fits
    }

    fn describe(s: &Sample) -> String {
        format!("c {:e} l {:e} fits {}", s.c, s.l, s.fits)
    }

    /// Checks every fast path against `bisect_serial` for one request. None
    /// when the request fits as is; otherwise whether the certified path took it.
    fn check_against_serial(target_y: f64, hue: f64, chroma: f64) -> Option<bool> {
        let path = Path::new(target_y, hue);
        let top = path.sample(chroma);
        if top.fits {
            assert!(same_sample(&top, &map_chroma(target_y, hue, chroma)));
            return None;
        }
        let c4 = (0.0 + chroma) / 2.0;
        let opening = Opening {
            top,
            q4: path.sample(c4),
            q2: path.sample((0.0 + c4) / 2.0),
            q6: path.sample((c4 + chroma) / 2.0),
            gray: path.sample(0.0),
        };
        let tol = chroma / f64::from(1u32 << OUTER_ITER);
        let serial = path.bisect_serial(top, tol);
        let label = |what: &str, got: &Sample| {
            format!(
                "{what}: target_y {target_y:e} hue {hue} chroma {chroma:e}: serial {} but got {}",
                describe(&serial),
                describe(got)
            )
        };

        let predicted = path.bisect_predicted(&opening, tol);
        assert!(
            same_sample(&serial, &predicted),
            "{}",
            label("predicted", &predicted)
        );
        let certified = path.bisect_certified(&opening, tol);
        if let Some(certified) = &certified {
            assert!(
                same_sample(&serial, certified),
                "{}",
                label("certified", certified)
            );
        }
        let dispatched = path.bisect(&opening);
        assert!(
            same_sample(&serial, &dispatched),
            "{}",
            label("bisect", &dispatched)
        );
        let mapped = map_chroma(target_y, hue, chroma);
        assert!(
            same_sample(&serial, &mapped),
            "{}",
            label("map_chroma", &mapped)
        );
        Some(certified.is_some())
    }

    #[test]
    fn fast_paths_match_serial_on_the_design_domain() {
        let mut hues: Vec<f64> = (0..360).step_by(3).map(f64::from).collect();
        // The hues where the feasible set splits into islands.
        hues.extend((2620..=2660).map(|h| f64::from(h) / 10.0));
        let tones = [
            0.5, 1.0, 2.5, 5.0, 10.0, 20.0, 35.0, 50.0, 65.0, 80.0, 90.0, 95.0, 98.0, 99.5,
        ];
        let chromas = [0.02, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.37, 0.4];

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

        let mut checked = 0;
        let mut certified_count = 0;
        for (tone, chroma, hue) in cases {
            if let Some(certified) = check_against_serial(cie_lstar_to_y(tone), hue, chroma) {
                checked += 1;
                certified_count += usize::from(certified);
            }
        }
        assert!(checked > 5000, "only {checked} mapped colors checked");
        assert!(
            certified_count * 10 > checked * 9,
            "certified path covered only {certified_count} of {checked}"
        );
    }

    #[test]
    fn fast_paths_match_serial_on_extreme_and_degenerate_requests() {
        let mut seed: u64 = 0x1234_5678_9abc_def1;
        let mut rand = move || {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut cases: Vec<(f64, f64, f64)> = Vec::new();
        for _ in 0..20_000 {
            let u = rand();
            // Tones log-spread toward both ends, where L approaches its clamps.
            let tone = if u < 0.25 {
                10f64.powf(-6.0 + 6.0 * rand())
            } else if u < 0.5 {
                100.0 - 10f64.powf(-6.0 + 6.0 * rand())
            } else {
                100.0 * rand()
            };
            let v = rand();
            // Above the certified path's 0.5 limit, negative, and far past the gamut.
            let chroma = if v < 0.6 {
                0.6 * rand()
            } else if v < 0.8 {
                -0.5 * rand()
            } else {
                2.0 * rand()
            };
            let hue = if rand() < 0.1 {
                262.0 + 3.0 * rand()
            } else {
                360.0 * rand()
            };
            cases.push((cie_lstar_to_y(tone), hue, chroma));
        }
        let specials = [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -0.0,
            0.0,
            1e-300,
            -1e-300,
            1e10,
            1e300,
            f64::MAX,
        ];
        for &chroma in &specials {
            for &tone in &[1e-9, 0.001, 0.5, 50.0, 99.0, 99.999_999] {
                for &hue in &[0.0, 120.0, 264.0, f64::NAN, f64::INFINITY] {
                    cases.push((cie_lstar_to_y(tone), hue, chroma));
                }
            }
        }
        // Raw targets reachable through `gamut_map_with_tone_preservation`.
        for &y in &[f64::INFINITY, f64::NEG_INFINITY, -0.1, 0.0, 1.0, 1.5, 1e300] {
            for &chroma in &[-0.2, 0.0, 0.4, 1e300] {
                cases.push((y, 264.0, chroma));
            }
        }
        // A NaN target makes the serial search exhaustive (tens of ms each), so only a few.
        for &chroma in &[0.4, 1e300, f64::NAN] {
            cases.push((f64::NAN, 264.0, chroma));
        }

        let mut checked = 0;
        for (y, hue, chroma) in cases {
            checked += usize::from(check_against_serial(y, hue, chroma).is_some());
        }
        assert!(checked > 15_000, "only {checked} mapped requests checked");
    }
}
