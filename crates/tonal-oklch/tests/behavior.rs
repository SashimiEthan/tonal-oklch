//! Behavioral tests ported from the npm package's vitest suites
//! (tests/index.test.ts, solve.test.ts, oklab.test.ts, luminance.test.ts,
//! contrast-stop.test.ts).

use tonal_oklch::*;

/// vitest's `toBeCloseTo(expected, digits)`: |a − b| < 10^−digits / 2.
fn close(actual: f64, expected: f64, digits: i32) -> bool {
    (actual - expected).abs() < 10f64.powi(-digits) / 2.0
}

macro_rules! assert_close {
    ($actual:expr, $expected:expr, $digits:expr) => {{
        let (a, e) = ($actual, $expected);
        assert!(
            close(a, e, $digits),
            "expected {a} to be within 10^-{} / 2 of {e}",
            $digits
        );
    }};
}

fn result(tone: f64, chroma: f64, hue: f64) -> TonalOklchResult {
    tonal_oklch_to_result(TonalOklch { tone, chroma, hue })
}

fn hex(tone: f64, chroma: f64, hue: f64) -> String {
    result(tone, chroma, hue).hex()
}

fn luminance_of_rgb8(rgb8: Rgb8) -> f64 {
    wcag_luminance(
        f64::from(rgb8.r) / 255.0,
        f64::from(rgb8.g) / 255.0,
        f64::from(rgb8.b) / 255.0,
    )
}

fn range(start: f64, end_inclusive: f64, step: f64) -> impl Iterator<Item = f64> {
    let n = ((end_inclusive - start) / step).round() as usize;
    (0..=n).map(move |i| start + step * i as f64)
}

// ── Edge cases ────────────────────────────────────────────

#[test]
fn tone_0_is_black_and_tone_100_is_white() {
    let black = result(0.0, 0.15, 264.0);
    assert_eq!(black.hex(), "#000000");
    assert_eq!(
        black.rgb,
        Srgb {
            r: 0.0,
            g: 0.0,
            b: 0.0
        }
    );
    assert_eq!(black.tone, 0.0);

    let white = result(100.0, 0.15, 264.0);
    assert_eq!(white.hex(), "#ffffff");
    assert_eq!(
        white.rgb,
        Srgb {
            r: 1.0,
            g: 1.0,
            b: 1.0
        }
    );
    assert_eq!(white.tone, 100.0);

    assert_eq!(hex(-10.0, 0.1, 100.0), "#000000");
    assert_eq!(hex(110.0, 0.1, 100.0), "#ffffff");
}

#[test]
fn chroma_0_produces_exact_neutrals() {
    for tone in [5.0, 20.0, 50.0, 80.0, 95.0] {
        let Rgb8 { r, g, b } = result(tone, 0.0, 0.0).rgb8;
        assert_eq!(r, g);
        assert_eq!(g, b);
    }
}

#[test]
fn float_grays_halfway_between_two_8_bit_steps_round_to_one_neutral_value() {
    for k in 0..255u32 {
        let v = (f64::from(k) + 0.5) / 255.0;
        let rgb8 = rgb_to_tonal_oklch(v, v, v).to_result().rgb8;
        assert_eq!(rgb8.g, rgb8.r);
        assert_eq!(rgb8.b, rgb8.r);
        assert!(
            u32::from(rgb8.r) == k || u32::from(rgb8.r) == k + 1,
            "k = {k}, got {}",
            rgb8.r
        );
    }
}

#[test]
fn a_nan_or_infinite_hue_counts_as_0() {
    let base = result(50.0, 0.1, 0.0);
    assert_eq!(base.hex(), "#aa6177");
    for hue in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let r = result(50.0, 0.1, hue);
        assert_eq!(r.hex(), base.hex());
        assert_eq!(r.rgb, base.rgb);
        assert_eq!(r.oklch, base.oklch);
    }
}

#[test]
fn float_channels_at_the_gamut_edge_are_exactly_1_and_0() {
    let coral = result(60.0, 0.25, 30.0);
    assert_eq!(coral.hex(), "#fe5642");
    assert_eq!(coral.rgb.r, 1.0);

    let green = rgb_to_tonal_oklch(6.0 / 255.0, 234.0 / 255.0, 0.0).to_result();
    assert_eq!(green.hex(), "#06ea00");
    assert_eq!(green.rgb.b, 0.0);
}

// ── Tone preservation ─────────────────────────────────────

#[test]
fn preserves_tone_in_result_metadata() {
    for tone in [10.0, 25.0, 50.0, 75.0, 90.0] {
        assert_eq!(result(tone, 0.15, 264.0).tone, tone);
    }
}

#[test]
fn float_rgb_holds_the_exact_tone_for_gamut_mapped_colors() {
    let tones = [
        0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 30.0, 50.0, 70.0, 90.0, 97.0, 99.0, 99.5,
    ];
    let mut max_y_err: f64 = 0.0;
    let mut min_channel = f64::INFINITY;
    let mut max_channel = f64::NEG_INFINITY;

    for tone in tones {
        for ci in 0..=8 {
            for hue in range(0.0, 350.0, 10.0) {
                let Srgb { r, g, b } = result(tone, f64::from(ci) / 20.0, hue).rgb;
                min_channel = min_channel.min(r).min(g).min(b);
                max_channel = max_channel.max(r).max(g).max(b);
                max_y_err = max_y_err.max((wcag_luminance(r, g, b) - cie_lstar_to_y(tone)).abs());
            }
        }
    }

    assert!(min_channel >= 0.0);
    assert!(max_channel <= 1.0);
    assert!(max_y_err <= 1e-9, "max Y error {max_y_err}");
}

#[test]
fn rgb8_is_rgb_rounded_then_nudged_by_at_most_one_step_per_channel() {
    let mut max_gap: f64 = 0.0;
    for tone in [
        0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 30.0, 50.0, 70.0, 90.0, 97.0, 99.5,
    ] {
        for ci in 0..=8 {
            for hue in range(0.0, 350.0, 10.0) {
                let TonalOklchResult { rgb, rgb8, .. } = result(tone, f64::from(ci) / 20.0, hue);
                for (float, byte) in [(rgb.r, rgb8.r), (rgb.g, rgb8.g), (rgb.b, rgb8.b)] {
                    max_gap = max_gap.max(((float * 255.0).round() - f64::from(byte)).abs());
                }
            }
        }
    }
    assert!(max_gap <= 1.0, "max gap {max_gap}");
}

#[test]
fn near_black_hexes_are_no_lighter_than_their_tone_beyond_8_bit_rounding() {
    assert_eq!(hex(0.1, 0.1, 252.0), "#000005");
    assert_eq!(hex(0.000001, 0.3, 260.0), "#000000");

    let mut max_lighter: f64 = 0.0;
    for tone in [0.001, 0.05, 0.1, 0.2, 0.3] {
        for ci in 1..=16 {
            for hue in range(0.0, 348.0, 12.0) {
                let rgb8 = result(tone, f64::from(ci) / 40.0, hue).rgb8;
                max_lighter = max_lighter.max(y_to_cie_lstar(luminance_of_rgb8(rgb8)) - tone);
            }
        }
    }
    assert!(max_lighter < 0.04, "max lighter {max_lighter}");
}

#[test]
fn a_dark_mapped_color_keeps_its_hex_and_has_an_exact_float_tone() {
    let r = result(5.0, 0.2, 264.0);
    assert_eq!(r.hex(), "#01004e");
    assert_close!(
        y_to_cie_lstar(wcag_luminance(r.rgb.r, r.rgb.g, r.rgb.b)),
        5.0,
        9
    );
}

#[test]
fn never_clamps_l_where_the_float_tone_is_exact() {
    let mut min_l = f64::INFINITY;
    let mut max_l = f64::NEG_INFINITY;
    for tone in [
        0.01, 0.02, 0.05, 0.1, 0.5, 1.0, 50.0, 99.0, 99.9, 99.99, 99.999, 99.9995,
    ] {
        for ci in 1..=40 {
            for hue in range(0.0, 358.0, 2.0) {
                let l = tonal_oklch_to_oklch(TonalOklch {
                    tone,
                    chroma: f64::from(ci) / 100.0,
                    hue,
                })
                .l;
                min_l = min_l.min(l);
                max_l = max_l.max(l);
            }
        }
    }
    assert!(min_l > 0.0);
    assert!(max_l < 1.0);
}

#[test]
fn same_tone_produces_nearly_identical_contrast_ratios() {
    let ratios: Vec<f64> = range(0.0, 345.0, 15.0)
        .map(|hue| wcag_contrast(luminance_of_rgb8(result(50.0, 0.15, hue).rgb8), 1.0))
        .collect();
    let min = ratios.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = ratios.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(max - min < 0.1, "contrast spread {}", max - min);

    for hue in range(0.0, 330.0, 30.0) {
        let y = luminance_of_rgb8(result(50.0, 0.15, hue).rgb8);
        assert!((y - cie_lstar_to_y(50.0)).abs() < 0.01);
    }
}

// ── Gamut mapping ─────────────────────────────────────────

#[test]
fn reduces_chroma_only_when_needed() {
    let mapped = tonal_oklch_to_oklch(TonalOklch {
        tone: 50.0,
        chroma: 0.4,
        hue: 264.0,
    });
    assert!(mapped.c < 0.4 && mapped.c > 0.0);

    let kept = tonal_oklch_to_oklch(TonalOklch {
        tone: 50.0,
        chroma: 0.01,
        hue: 100.0,
    });
    assert_close!(kept.c, 0.01, 3);

    // Request 0.2 at tone 15 hue 264 sits just inside the gamut tolerance.
    assert_eq!(
        tonal_oklch_to_oklch(TonalOklch {
            tone: 15.0,
            chroma: 0.2,
            hue: 264.0
        })
        .c,
        0.2
    );
    assert_eq!(hex(15.0, 0.2, 264.0), "#00028b");
}

#[test]
fn dark_blues_at_hue_264_map_to_the_strongest_feasible_chroma_regardless_of_the_request() {
    assert_eq!(hex(10.0, 0.4, 264.0), "#00016d");
    assert_eq!(hex(15.0, 0.4, 264.0), "#00018c");
    assert_eq!(hex(20.0, 0.4, 264.0), "#0100ac");
    for chroma in [0.225, 0.3, 0.375] {
        assert_eq!(hex(15.0, chroma, 264.0), "#00018c");
    }
}

#[test]
fn gamut_map_finds_the_upper_feasible_island_at_hue_264() {
    for (tone, lower_top, min_request) in [
        (10.0, 0.146, 0.2),
        (15.0, 0.169, 0.225),
        (20.0, 0.194, 0.25),
    ] {
        let y = cie_lstar_to_y(tone);
        let first = gamut_map_with_tone_preservation(y, 264.0, min_request).c;
        assert!(first > lower_top + 0.01);
        for request in range(min_request, 0.4, 0.025) {
            assert_close!(
                gamut_map_with_tone_preservation(y, 264.0, request).c,
                first,
                7
            );
        }
    }
}

#[test]
fn mapped_chroma_never_decreases_as_the_request_grows_and_the_result_fits() {
    let hues = [
        0.0, 29.0, 100.0, 150.0, 200.0, 263.9, 264.0, 264.1, 300.0, 328.0,
    ];
    for tone in [0.5, 5.0, 10.0, 15.0, 20.0, 30.0, 50.0, 75.0, 99.0] {
        let y = cie_lstar_to_y(tone);
        for hue in hues {
            let mut prev = 0.0;
            for request in range(0.01, 0.4, 0.01) {
                let Oklch { l, c, h } = gamut_map_with_tone_preservation(y, hue, request);
                assert_eq!(h, hue);
                assert!(
                    c >= prev - 3e-8,
                    "tone {tone} hue {hue} request {request}: {c} < {prev}"
                );
                let Lrgb { r, g, b } = oklch_to_lrgb(Oklch { l, c, h });
                assert!(
                    is_displayable(r, g, b, 1e-4 + 1e-6),
                    "tone {tone} hue {hue} request {request}"
                );
                prev = c;
            }
        }
    }
}

#[test]
fn gamut_map_returns_the_requested_chroma_unchanged_when_it_fits() {
    assert_eq!(
        gamut_map_with_tone_preservation(cie_lstar_to_y(50.0), 150.0, 0.05).c,
        0.05
    );
    let mid = gamut_map_with_tone_preservation(cie_lstar_to_y(50.0), 150.0, 0.4).c;
    let dark = gamut_map_with_tone_preservation(cie_lstar_to_y(15.0), 150.0, 0.4).c;
    assert!(mid > dark);
}

// ── Solver ────────────────────────────────────────────────

#[test]
fn solver_round_trips_to_the_target_y_wherever_the_root_is_inside_0_1() {
    let tones: Vec<f64> = [0.01, 0.1, 0.5]
        .into_iter()
        .chain((1..=99).map(f64::from))
        .collect();
    let mut max_err: f64 = 0.0;
    let mut inside = 0;
    let mut total = 0;

    for tone in tones {
        let target_y = cie_lstar_to_y(tone);
        for ci in 0..=20 {
            let chroma = f64::from(ci) / 50.0;
            for hue in range(0.0, 355.0, 5.0) {
                let l = find_l_for_target_y(target_y, chroma, hue);
                total += 1;
                assert!((0.0..=1.0).contains(&l));
                if l > 0.0 && l < 1.0 {
                    inside += 1;
                    let Lrgb { r, g, b } = oklch_to_lrgb(Oklch {
                        l,
                        c: chroma,
                        h: hue,
                    });
                    max_err = max_err.max((0.2126 * r + 0.7152 * g + 0.0722 * b - target_y).abs());
                }
            }
        }
    }

    assert!(inside as f64 > total as f64 * 0.95);
    assert!(max_err <= 1e-12, "max error {max_err}");
}

#[test]
fn solver_returns_known_roots() {
    assert_close!(
        find_l_for_target_y(cie_lstar_to_y(50.0), 0.1, 264.0),
        0.5711138,
        7
    );
    // Impossible inputs: the cubic has three real roots; the largest is returned.
    assert_close!(
        find_l_for_target_y(cie_lstar_to_y(3.0), 0.4, 65.0),
        0.1702,
        4
    );
    // Raw root is negative (−0.304); clamped.
    assert_eq!(find_l_for_target_y(cie_lstar_to_y(1.0), 0.4, 99.0), 0.0);
    assert_close!(find_l_for_target_y(0.999, 0.0, 0.0), 1.0, 1);
}

#[test]
fn solver_l_increases_monotonically_with_y() {
    let mut prev = -1.0;
    for tone in range(5.0, 95.0, 5.0) {
        let l = find_l_for_target_y(cie_lstar_to_y(tone), 0.1, 264.0);
        assert!(l > prev);
        prev = l;
    }
}

// ── sRGB ↔ OKLCh ──────────────────────────────────────────

#[test]
fn matches_the_css_color_4_sample_conversions_for_the_srgb_primaries() {
    let red = rgb_to_oklch(1.0, 0.0, 0.0);
    assert_close!(red.l, 0.627955, 5);
    assert_close!(red.c, 0.257683, 5);
    assert_close!(red.h, 29.2339, 3);

    let green = rgb_to_oklch(0.0, 1.0, 0.0);
    assert_close!(green.l, 0.86644, 5);
    assert_close!(green.c, 0.294827, 5);
    assert_close!(green.h, 142.4953, 3);

    let blue = rgb_to_oklch(0.0, 0.0, 1.0);
    assert_close!(blue.l, 0.452014, 5);
    assert_close!(blue.c, 0.313214, 5);
    assert_close!(blue.h, 264.052, 3);
}

#[test]
fn maps_white_and_black_exactly_enough() {
    let white = oklch_to_lrgb(Oklch {
        l: 1.0,
        c: 0.0,
        h: 0.0,
    });
    assert_close!(white.r, 1.0, 12);
    assert_close!(white.g, 1.0, 12);
    assert_close!(white.b, 1.0, 12);
    assert_eq!(
        oklch_to_lrgb(Oklch {
            l: 0.0,
            c: 0.0,
            h: 0.0
        }),
        Lrgb {
            r: 0.0,
            g: 0.0,
            b: 0.0
        }
    );
}

#[test]
fn neutral_inputs_give_chroma_0_and_hue_0_exactly() {
    for v in 0..256 {
        let v = f64::from(v) / 255.0;
        let Oklch { c, h, .. } = rgb_to_oklch(v, v, v);
        assert_eq!(c, 0.0);
        assert_eq!(h, 0.0);
    }
}

#[test]
fn round_trips_8_bit_srgb_through_oklch() {
    let mut max_err: f64 = 0.0;
    for r in (0..256).step_by(15) {
        for g in (0..256).step_by(15) {
            for b in (0..256).step_by(15) {
                let (r, g, b) = (
                    f64::from(r) / 255.0,
                    f64::from(g) / 255.0,
                    f64::from(b) / 255.0,
                );
                let back = oklch_to_rgb(rgb_to_oklch(r, g, b));
                max_err = max_err
                    .max((back.r - r).abs())
                    .max((back.g - g).abs())
                    .max((back.b - b).abs());
            }
        }
    }
    assert!(max_err < 1e-9, "max error {max_err}");
}

#[test]
fn round_trips_out_of_gamut_srgb_mirroring_negative_channels() {
    let wide = rgb_to_oklch(-0.2, 0.3, 0.9);
    assert_close!(wide.c, 0.2509, 4);
    assert_close!(wide.h, 255.88, 2);
    let back = oklch_to_rgb(wide);
    assert_close!(back.r, -0.2, 9);
    assert_close!(back.g, 0.3, 9);
    assert_close!(back.b, 0.9, 9);
}

#[test]
fn delinearize_inverts_linearize_and_is_mirrored_and_unclamped() {
    for k in 0..=255 {
        let v = f64::from(k) / 255.0;
        assert_close!(delinearize(linearize(v)), v, 12);
    }
    assert_eq!(delinearize(-0.01), -delinearize(0.01));
    assert_eq!(delinearize(-0.001), -delinearize(0.001));
    assert!(delinearize(1.2) > 1.0);
}

// ── Luminance ─────────────────────────────────────────────

#[test]
fn luminance_utilities_match_wcag_and_cie() {
    assert_eq!(linearize(0.0), 0.0);
    assert_close!(linearize(1.0), 1.0, 10);
    assert_close!(linearize(0.04045), 0.04045 / 12.92, 10);
    assert_eq!(wcag_luminance(0.0, 0.0, 0.0), 0.0);
    assert_close!(wcag_luminance(1.0, 1.0, 1.0), 1.0, 5);
    assert_close!(wcag_luminance(0.5, 0.5, 0.5), 0.2140, 3);
    assert_close!(wcag_contrast(0.0, 1.0), 21.0, 1);
    assert_eq!(wcag_contrast(0.5, 0.5), 1.0);
    assert_eq!(wcag_contrast(0.2, 0.8), wcag_contrast(0.8, 0.2));

    assert_eq!(cie_lstar_to_y(0.0), 0.0);
    assert_close!(cie_lstar_to_y(100.0), 1.0, 5);
    assert_close!(cie_lstar_to_y(50.0), 0.1841, 3);
    for lstar in [0.0, 8.0, 10.0, 25.0, 50.0, 75.0, 90.0, 100.0] {
        assert_close!(y_to_cie_lstar(cie_lstar_to_y(lstar)), lstar, 8);
    }

    assert!(is_displayable(0.0, 0.0, 0.0, 1e-4));
    assert!(is_displayable(1.00005, 0.0, 0.0, 1e-4));
    assert!(is_displayable(-0.00005, 0.0, 0.0, 1e-4));
    assert!(!is_displayable(1.01, 0.0, 0.0, 1e-4));
    assert!(!is_displayable(0.0, -0.01, 0.0, 1e-4));
}

// ── Reverse conversion ────────────────────────────────────

#[test]
fn rgb_to_tonal_oklch_inverts_the_forward_conversion_closely() {
    for tone in [20.0, 50.0, 80.0] {
        let Srgb { r, g, b } = tonal_oklch_to_rgb(TonalOklch {
            tone,
            chroma: 0.0,
            hue: 0.0,
        });
        let back = rgb_to_tonal_oklch(r, g, b);
        assert_close!(back.tone, tone, 0);
        assert_close!(back.chroma, 0.0, 2);
    }
    for hue in [30.0, 150.0, 264.0] {
        let Srgb { r, g, b } = tonal_oklch_to_rgb(TonalOklch {
            tone: 60.0,
            chroma: 0.12,
            hue,
        });
        assert!((rgb_to_tonal_oklch(r, g, b).tone - 60.0).abs() < 1.0);
    }

    assert_close!(
        rgb_to_tonal_oklch(1.0, 0.0, 0.0).tone,
        y_to_cie_lstar(0.2126),
        3
    );
    assert_close!(
        rgb_to_tonal_oklch(0.0, 1.0, 0.0).tone,
        y_to_cie_lstar(0.7152),
        3
    );
    assert_close!(
        rgb_to_tonal_oklch(0.0, 0.0, 1.0).tone,
        y_to_cie_lstar(0.0722),
        3
    );
}

#[test]
fn tonal_oklch_to_rgb_matches_rgb8_from_the_full_result() {
    let color = TonalOklch {
        tone: 65.0,
        chroma: 0.15,
        hue: 264.0,
    };
    let full = tonal_oklch_to_result(color);
    let rgb = tonal_oklch_to_rgb(color);
    assert_eq!(rgb.r, f64::from(full.rgb8.r) / 255.0);
    assert_eq!(rgb.g, f64::from(full.rgb8.g) / 255.0);
    assert_eq!(rgb.b, f64::from(full.rgb8.b) / 255.0);
}

// ── tonal-oklch.com's 4.5:1 stop ──────────────────────────

/// Contrast against white computed from rgb8 and floored to two decimals, as the site displays it.
fn displayed_contrast(tone: f64, chroma: f64, hue: f64) -> f64 {
    let raw = wcag_contrast(luminance_of_rgb8(result(tone, chroma, hue).rgb8), 1.0);
    (raw * 100.0).floor() / 100.0
}

#[test]
fn the_4_5_to_1_stop_at_tone_49_84_passes_against_white() {
    let stop = 49.84;
    let hues: Vec<f64> = range(0.0, 360.0, 30.0).collect();
    for &hue in &hues {
        assert!(displayed_contrast(stop, 0.15, hue) >= 4.5, "hue {hue}");
    }
    for &reference in &hues[..hues.len() - 1] {
        let chroma = result(stop, 0.5, reference).oklch.c;
        for &hue in &hues {
            assert!(
                displayed_contrast(stop, chroma, hue) >= 4.5,
                "ref {reference} hue {hue}"
            );
        }
    }
    assert_eq!(hex(49.83, 0.0, 0.0), "#767676");
    assert!(displayed_contrast(49.83, 0.0, 0.0) >= 4.5);
}
