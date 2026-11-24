import { describe, expect, it } from 'vitest';
import { applyAdjustPixels, FILTER_PRESETS, presetAdjust } from './adjust';
import { adjustAffine } from './adjustMatrix';
import type { PhotoAdjust } from '../model/types';

function viaMatrix(a: PhotoAdjust, rgb: [number, number, number]): number[] {
  const m = adjustAffine(a);
  const [r, g, b] = rgb.map((v) => v / 255);
  return [0, 1, 2].map((i) => Math.max(0, Math.min(255, Math.round((m[i * 4] * r + m[i * 4 + 1] * g + m[i * 4 + 2] * b + m[i * 4 + 3]) * 255))));
}

describe('adjust matrix', () => {
  const samples: [number, number, number][] = [
    [0, 0, 0],
    [255, 255, 255],
    [200, 40, 90],
    [12, 180, 240],
    [128, 128, 128],
    [250, 210, 20],
  ];
  const looks: PhotoAdjust[] = [
    ...FILTER_PRESETS.map((p) => presetAdjust(p.id)),
    { brightness: 0.4, contrast: -0.3, saturation: 0.6, warmth: -0.7, fade: 0.2, vignette: 0, grayscale: 0.3, sepia: 0.5 },
  ];
  it('matches the pixel implementation', () => {
    for (const a of looks)
      for (const s of samples) {
        const px = new Uint8ClampedArray([...s, 255]);
        applyAdjustPixels(px, a);
        const want = [px[0], px[1], px[2]];
        const got = viaMatrix(a, s);
        for (let i = 0; i < 3; i++) expect(Math.abs(got[i] - want[i])).toBeLessThanOrEqual(1);
      }
  });
});
