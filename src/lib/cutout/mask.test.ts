import { describe, expect, it } from 'vitest';
import { alphaBounds, applyLevels, buildAlpha, composite, feather, iou, normalizeMask, paintDabs, resizeMask } from './mask';
import { cutoutName } from './names';
import { toInput, MODEL_SIZE } from './model';

/** A w×h mask with a filled disc. */
function disc(w: number, h: number, cx: number, cy: number, r: number, inside = 1, outside = 0): Float32Array {
  const m = new Float32Array(w * h);
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) m[y * w + x] = Math.hypot(x + 0.5 - cx, y + 0.5 - cy) <= r ? inside : outside;
  return m;
}

describe('mask post-processing', () => {
  it('normalizes to 0..1 and survives flat masks', () => {
    expect([...normalizeMask(new Float32Array([0.2, 0.4, 0.6]))].map((v) => +v.toFixed(3))).toEqual([0, 0.5, 1]);
    expect([...normalizeMask(new Float32Array([0.3, 0.3]))]).toEqual([0, 0]);
  });

  it('resizes bilinearly, keeping shapes and constants', () => {
    const flat = resizeMask(new Float32Array(4).fill(0.7), 2, 2, 5, 3);
    expect(flat.every((v) => Math.abs(v - 0.7) < 1e-6)).toBe(true);
    const small = disc(32, 32, 16, 16, 8);
    const big = resizeMask(small, 32, 32, 128, 128);
    expect(iou(big, disc(128, 128, 64, 64, 32))).toBeGreaterThan(0.95);
  });

  it('thresholds with a smooth ramp', () => {
    const m = applyLevels(new Float32Array([0, 0.3, 0.5, 0.7, 1]), 0.5, 0.4);
    expect(m[0]).toBe(0);
    expect(m[4]).toBe(1);
    expect(m[2]).toBeCloseTo(0.5, 5);
    expect(m[1]).toBeLessThan(m[2]);
    expect(m[3]).toBeGreaterThan(m[2]);
    // softness 0 = a hard edge
    const hard = applyLevels(new Float32Array([0.49, 0.51]), 0.5, 0);
    expect([...hard]).toEqual([0, 1]);
  });

  it('feathers edges without changing flat areas', () => {
    const m = disc(64, 64, 32, 32, 16);
    feather(m, 64, 64, 2);
    expect(m[32 * 64 + 32]).toBeCloseTo(1, 5);
    expect(m[0]).toBeCloseTo(0, 5);
    const edge = m[32 * 64 + 48];
    expect(edge).toBeGreaterThan(0.1);
    expect(edge).toBeLessThan(0.9);
    // radius < 1 px: untouched
    const same = disc(8, 8, 4, 4, 2);
    expect(feather(same.slice(), 8, 8, 0.3)).toEqual(same);
  });

  it('erases and restores with brush dabs', () => {
    const m = new Float32Array(100 * 100).fill(1);
    paintDabs(m, 100, 100, [{ x: 0.5, y: 0.5, r: 0.1, mode: 'erase' }]);
    expect(m[50 * 100 + 50]).toBe(0);
    expect(m[0]).toBe(1);
    paintDabs(m, 100, 100, [{ x: 0.5, y: 0.5, r: 0.05, mode: 'restore' }]);
    expect(m[50 * 100 + 50]).toBe(1);
    expect(m[50 * 100 + 56]).toBe(0);
  });

  it('finds the visible bounds and composites straight alpha', () => {
    const a = new Float32Array(10 * 10);
    a[3 * 10 + 2] = 1;
    a[6 * 10 + 7] = 0.5;
    expect(alphaBounds(a, 10, 10, 0.03, 0)).toEqual({ x: 2, y: 3, w: 6, h: 4 });
    expect(alphaBounds(new Float32Array(9), 3, 3)).toBeNull();
    const rgba = new Uint8ClampedArray(10 * 10 * 4).fill(200);
    const out = composite(rgba, a, 10, { x: 2, y: 3, w: 6, h: 4 });
    expect([...out.slice(0, 4)]).toEqual([200, 200, 200, 255]);
    expect(out[(3 * 6 + 5) * 4 + 3]).toBe(128);
    // transparent pixels are zeroed (compresses better)
    expect([...out.slice(4, 8)]).toEqual([0, 0, 0, 0]);
  });

  it('runs the whole pipeline at any size', () => {
    const raw = disc(MODEL_SIZE, MODEL_SIZE, 160, 160, 80, 0.9, 0.05);
    const a = buildAlpha(raw, MODEL_SIZE, MODEL_SIZE, 400, 300, { threshold: 0.5, softness: 0.3, feather: 0.002 });
    expect(a.length).toBe(400 * 300);
    // the disc becomes an ellipse in a non-square image
    const b = alphaBounds(a, 400, 300, 0.5, 0)!;
    expect(b.w).toBeGreaterThan(190);
    expect(b.w).toBeLessThan(210);
    expect(b.h).toBeGreaterThan(140);
    expect(b.h).toBeLessThan(160);
  });

  it('names cut-outs after their source', () => {
    expect(cutoutName('/a/b/IMG_0042.HEIC')).toBe('IMG_0042 cut-out.png');
    expect(cutoutName('weird:name?.jpg')).toBe('weird_name_ cut-out.png');
    expect(cutoutName('.hidden')).toBe('hidden cut-out.png');
    expect(cutoutName('...')).toBe('photo cut-out.png');
  });
});

describe('model input', () => {
  it('scales by the brightest value and standardises, NCHW', () => {
    const n = MODEL_SIZE * MODEL_SIZE;
    const rgba = new Uint8ClampedArray(n * 4);
    for (let i = 0; i < n; i++) rgba.set([100, 50, 0, 255], i * 4);
    const t = toInput(rgba);
    expect(t.length).toBe(3 * n);
    expect(t[0]).toBeCloseTo((1 - 0.485) / 0.229, 4);
    expect(t[n]).toBeCloseTo((0.5 - 0.456) / 0.224, 4);
    expect(t[2 * n]).toBeCloseTo((0 - 0.406) / 0.225, 4);
    expect(() => toInput(new Uint8ClampedArray(16))).toThrow();
  });
});
