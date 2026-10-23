import { describe, it, expect } from 'vitest';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { tonalOklchToResult } from '../index';

const FIXTURE = fileURLToPath(new URL('./fixtures/golden-hex.txt', import.meta.url));

const TONES = [
  0.5, 1, 2, 3, 5, 10, 15, 20, 25, 30, 35, 40, 45, 49.83, 49.84,
  50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 97, 98, 99, 99.5,
];
const CHROMAS = Array.from({ length: 17 }, (_, i) => Number((i / 40).toFixed(3)));
const HUES = Array.from({ length: 45 }, (_, i) => i * 8);

function generate(): string {
  const lines = [
    `# tone chroma hex@hue... (hues ${HUES.join(',')}). Regenerate: UPDATE_GOLDEN=1 npx vitest run tests/golden.test.ts`,
  ];
  for (const tone of TONES) {
    for (const chroma of CHROMAS) {
      const hexes = HUES.map((hue) => tonalOklchToResult({ tone, chroma, hue }).hex);
      lines.push(`${tone} ${chroma.toFixed(3)} ${hexes.join(' ')}`);
    }
  }
  return lines.join('\n') + '\n';
}

describe('golden hex fixture', () => {
  it('every hex on the grid matches tests/fixtures/golden-hex.txt', () => {
    if (process.env.UPDATE_GOLDEN) {
      writeFileSync(FIXTURE, generate());
      return;
    }

    const lines = readFileSync(FIXTURE, 'utf8').trimEnd().split('\n').filter((l) => !l.startsWith('#'));
    expect(lines).toHaveLength(TONES.length * CHROMAS.length);

    const mismatches: string[] = [];
    for (const line of lines) {
      const [toneText, chromaText, ...hexes] = line.split(' ');
      const tone = Number(toneText);
      const chroma = Number(chromaText);
      expect(hexes).toHaveLength(HUES.length);
      HUES.forEach((hue, i) => {
        const actual = tonalOklchToResult({ tone, chroma, hue }).hex;
        if (actual !== hexes[i]) {
          mismatches.push(`tone ${tone} chroma ${chroma} hue ${hue}: expected ${hexes[i]}, got ${actual}`);
        }
      });
    }

    expect(mismatches, `${mismatches.length} hex mismatches:\n${mismatches.slice(0, 20).join('\n')}`).toEqual([]);
  });
});
