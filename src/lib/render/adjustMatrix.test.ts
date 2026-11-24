import { describe, expect, it } from 'vitest';
import { applyAdjustPixels, FILTER_PRESETS, presetAdjust } from './adjust';
import { adjustAffine } from './adjustMatrix';
import { applyLookPixels, autoAdjust, toneCurve } from './look';
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
    // presets made only of the original eight adjustments
    ...FILTER_PRESETS.map((p) => presetAdjust(p.id)).filter((a) => Object.keys(a).length === 8),
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

describe('look pipeline', () => {
  it('matrix stage of applyLookPixels equals the affine matrix (incl. exposure and tint)', () => {
    const a = { ...presetAdjust('none'), exposure: 0.4, tint: -0.3, contrast: 0.2, warmth: 0.3 };
    const px = new Uint8ClampedArray([200, 40, 90, 255, 12, 180, 240, 255]);
    const want = [viaMatrix(a, [200, 40, 90]), viaMatrix(a, [12, 180, 240])].flat();
    applyLookPixels(px, 2, 1, a);
    expect([px[0], px[1], px[2], px[4], px[5], px[6]].every((v, i) => Math.abs(v - want[i]) <= 1)).toBe(true);
  });
  it('tone curve is monotonic and fixes black and white', () => {
    for (const s of [-1, 1])
      for (const h of [-1, 1]) {
        const c = toneCurve({ shadows: s, highlights: h })!;
        expect(c[0]).toBe(0);
        expect(c[c.length - 1]).toBe(1);
        for (let i = 1; i < c.length; i++) expect(c[i]).toBeGreaterThanOrEqual(c[i - 1] - 1e-9);
      }
  });
  it('duotone maps black and white to the two colours', () => {
    const a = { ...presetAdjust('none'), duotone: { dark: '#102030', light: '#f0e0d0', amount: 1 } };
    const px = new Uint8ClampedArray([0, 0, 0, 255, 255, 255, 255, 255]);
    applyLookPixels(px, 2, 1, a);
    expect([...px.slice(0, 3)]).toEqual([16, 32, 48]);
    expect([...px.slice(4, 7)]).toEqual([240, 224, 208]);
  });
  it('hidden areas only change their rectangle', () => {
    const w = 40;
    const h = 20;
    const px = new Uint8ClampedArray(w * h * 4).map((_, i) => (i % 4 === 3 ? 255 : (i * 37) % 256));
    const before = px.slice();
    applyLookPixels(px, w, h, presetAdjust('none'), [{ x: 0.5, y: 0, w: 0.5, h: 1, mode: 'pixelate' }]);
    for (let y = 0; y < h; y++) for (let x = 0; x < 20; x++) expect(px[(y * w + x) * 4]).toBe(before[(y * w + x) * 4]);
  });
  it('auto enhance brightens a dark, blue photo', () => {
    const px = new Uint8ClampedArray(64 * 64 * 4);
    for (let i = 0; i < px.length; i += 4) px.set([30 + (i % 40), 35 + (i % 40), 70 + (i % 40), 255], i);
    const a = autoAdjust(px);
    expect(a.exposure!).toBeGreaterThan(0);
    expect(a.warmth!).toBeGreaterThan(0);
  });
});
