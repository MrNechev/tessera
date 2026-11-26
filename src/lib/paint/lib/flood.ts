/**
 * Colour matching for the fill bucket and the magic wand: which pixels are
 * "the same colour" as the one clicked, either connected to it (contiguous,
 * a scanline flood fill) or anywhere in the image.
 */
import type { Rect } from './mask';

/**
 * Difference of two RGBA pixels, 0..255: the largest channel difference,
 * where fully transparent pixels count as equal whatever their colour.
 */
export function pixelDistance(d: Uint8ClampedArray | Uint8Array, i: number, r: number, g: number, b: number, a: number): number {
  const pa = d[i + 3];
  if (pa === 0 && a === 0) return 0;
  return Math.max(Math.abs(d[i] - r), Math.abs(d[i + 1] - g), Math.abs(d[i + 2] - b), Math.abs(pa - a));
}

export interface MatchResult {
  /** 255 where the pixel matches, 0 elsewhere. */
  mask: Uint8Array;
  bounds: Rect | null;
  count: number;
}

/**
 * Pixels matching the colour at (x, y) within `tolerance` (0..100 %).
 * `contiguous`: only those connected to (x, y) (4-neighbourhood).
 */
export function matchColor(
  d: Uint8ClampedArray | Uint8Array,
  w: number,
  h: number,
  x: number,
  y: number,
  tolerance: number,
  contiguous: boolean,
): MatchResult {
  const mask = new Uint8Array(w * h);
  x = Math.floor(x);
  y = Math.floor(y);
  if (x < 0 || y < 0 || x >= w || y >= h) return { mask, bounds: null, count: 0 };
  const s = (y * w + x) * 4;
  const r = d[s];
  const g = d[s + 1];
  const b = d[s + 2];
  const a = d[s + 3];
  const t = Math.max(0, Math.min(100, tolerance)) * 2.55;
  const ok = (p: number) => pixelDistance(d, p * 4, r, g, b, a) <= t;
  let x0 = w;
  let y0 = h;
  let x1 = -1;
  let y1 = -1;
  let count = 0;
  const mark = (row: number, from: number, to: number) => {
    mask.fill(255, row * w + from, row * w + to + 1);
    count += to - from + 1;
    if (from < x0) x0 = from;
    if (to > x1) x1 = to;
    if (row < y0) y0 = row;
    if (row > y1) y1 = row;
  };
  if (!contiguous) {
    for (let yy = 0; yy < h; yy++) {
      let run = -1;
      for (let xx = 0; xx <= w; xx++) {
        const hit = xx < w && ok(yy * w + xx);
        if (hit && run < 0) run = xx;
        else if (!hit && run >= 0) {
          mark(yy, run, xx - 1);
          run = -1;
        }
      }
    }
  } else {
    // Scanline fill: a stack of seeds; each seed fills its whole run, then
    // queues the runs above and below.
    const stack: number[] = [x, y];
    while (stack.length) {
      const sy = stack.pop()!;
      const sx = stack.pop()!;
      const row = sy * w;
      if (mask[row + sx] || !ok(row + sx)) continue;
      let l = sx;
      while (l > 0 && !mask[row + l - 1] && ok(row + l - 1)) l--;
      let rr = sx;
      while (rr < w - 1 && !mask[row + rr + 1] && ok(row + rr + 1)) rr++;
      mark(sy, l, rr);
      for (const ny of [sy - 1, sy + 1]) {
        if (ny < 0 || ny >= h) continue;
        const nrow = ny * w;
        let inRun = false;
        for (let xx = l; xx <= rr; xx++) {
          const free = !mask[nrow + xx] && ok(nrow + xx);
          if (free && !inRun) {
            stack.push(xx, ny);
            inRun = true;
          } else if (!free) inRun = false;
        }
      }
    }
  }
  return { mask, bounds: x1 < 0 ? null : { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 }, count };
}

/**
 * Paint `rgba` (straight alpha) where `mask` (0..255, optionally multiplied
 * by `limit`, the selection; both image-sized, `imgW` wide) is set. `d` holds
 * only the pixels of `r` (as from getImageData of that rectangle). Soft mask
 * values blend "source over".
 */
export function fillMasked(
  d: Uint8ClampedArray,
  r: Rect,
  imgW: number,
  mask: Uint8Array,
  rgba: [number, number, number, number],
  limit: Uint8Array | null = null,
): void {
  const [cr, cg, cb, ca] = rgba;
  for (let y = 0; y < r.h; y++) {
    for (let x = 0; x < r.w; x++) {
      const p = (r.y + y) * imgW + r.x + x;
      let m = mask[p];
      if (limit) m = (m * limit[p]) / 255;
      if (!m) continue;
      const i = (y * r.w + x) * 4;
      const sa = (ca / 255) * (m / 255);
      const da = d[i + 3] / 255;
      const oa = sa + da * (1 - sa);
      if (oa <= 0) continue;
      d[i] = (cr * sa + d[i] * da * (1 - sa)) / oa;
      d[i + 1] = (cg * sa + d[i + 1] * da * (1 - sa)) / oa;
      d[i + 2] = (cb * sa + d[i + 2] * da * (1 - sa)) / oa;
      d[i + 3] = oa * 255;
    }
  }
}
