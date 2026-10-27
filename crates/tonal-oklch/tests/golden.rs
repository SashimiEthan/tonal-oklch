//! Every hex on the golden grid must match the npm package's fixture at
//! tests/fixtures/golden-hex.txt (regenerated there with
//! `UPDATE_GOLDEN=1 npx vitest run tests/golden.test.ts`).

use std::fs;

use tonal_oklch::{TonalOklch, tonal_oklch_to_result};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/golden-hex.txt"
);

#[test]
fn every_hex_on_the_grid_matches_the_js_fixture() {
    let text = fs::read_to_string(FIXTURE).expect("read tests/fixtures/golden-hex.txt");
    let mut lines = text.lines();

    let header = lines.next().expect("header line");
    let hues: Vec<f64> = header
        .split("(hues ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .expect("hue list in header")
        .split(',')
        .map(|h| h.parse().expect("hue"))
        .collect();
    assert_eq!(hues.len(), 45);

    let mut checked = 0;
    let mut mismatches = Vec::new();
    for line in lines.filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let mut parts = line.split(' ');
        let tone: f64 = parts.next().unwrap().parse().unwrap();
        let chroma: f64 = parts.next().unwrap().parse().unwrap();
        let hexes: Vec<&str> = parts.collect();
        assert_eq!(
            hexes.len(),
            hues.len(),
            "line for tone {tone} chroma {chroma}"
        );

        for (expected, &hue) in hexes.iter().zip(&hues) {
            checked += 1;
            let actual = tonal_oklch_to_result(TonalOklch { tone, chroma, hue }).hex();
            if actual != *expected {
                mismatches.push(format!(
                    "tone {tone} chroma {chroma} hue {hue}: expected {expected}, got {actual}"
                ));
            }
        }
    }

    assert_eq!(checked, 22_185);
    assert!(
        mismatches.is_empty(),
        "{} hex mismatches:\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
