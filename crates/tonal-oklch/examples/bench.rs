//! Throughput on the golden grid (29 tones × 17 chromas × 45 hues).
//! Run with `cargo run --release --example bench`; compare with `node scripts/bench.mjs`.

use std::hint::black_box;
use std::time::Instant;

use tonal_oklch::{TonalOklch, tonal_oklch_to_oklch, tonal_oklch_to_result};

const TONES: [f64; 29] = [
    0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 49.83, 49.84, 50.0,
    55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 90.0, 95.0, 97.0, 98.0, 99.0, 99.5,
];

fn grid() -> Vec<TonalOklch> {
    let mut colors = Vec::new();
    for tone in TONES {
        for ci in 0..17 {
            for hi in 0..45 {
                colors.push(TonalOklch {
                    tone,
                    chroma: f64::from(ci) / 40.0,
                    hue: f64::from(hi * 8),
                });
            }
        }
    }
    colors
}

/// Median wall time of `reps` passes over `colors`, in seconds.
fn time<T>(colors: &[TonalOklch], reps: usize, f: impl Fn(TonalOklch) -> T) -> f64 {
    let mut runs: Vec<f64> = (0..reps)
        .map(|_| {
            let start = Instant::now();
            for &c in colors {
                black_box(f(black_box(c)));
            }
            start.elapsed().as_secs_f64()
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    runs[reps / 2]
}

fn report(label: &str, colors: &[TonalOklch], seconds: f64) {
    let per = seconds / colors.len() as f64;
    println!(
        "{label:<10} {:>6} colors  {:>7.3} µs/color  {:>6.2} M colors/s",
        colors.len(),
        per * 1e6,
        1.0 / per / 1e6
    );
}

fn main() {
    let all = grid();
    let (mapped, in_gamut): (Vec<_>, Vec<_>) = all
        .iter()
        .partition(|&&c| tonal_oklch_to_oklch(c).c != c.chroma);
    println!(
        "Golden grid: {} colors, {} ({:.0}%) need gamut mapping. Median of 9 runs.\n",
        all.len(),
        mapped.len(),
        100.0 * mapped.len() as f64 / all.len() as f64
    );

    for _ in 0..3 {
        time(&all, 1, tonal_oklch_to_result);
    }

    println!("tonal_oklch_to_result");
    report("all", &all, time(&all, 9, tonal_oklch_to_result));
    report(
        "in-gamut",
        &in_gamut,
        time(&in_gamut, 9, tonal_oklch_to_result),
    );
    report("mapped", &mapped, time(&mapped, 9, tonal_oklch_to_result));

    println!("\ntonal_oklch_to_oklch");
    report("all", &all, time(&all, 9, tonal_oklch_to_oklch));
}
