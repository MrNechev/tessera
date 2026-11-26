/**
 * Selections as pixel masks (one byte per pixel, 0 = outside, 255 = inside,
 * values in between for soft edges such as "Select subject"). Pure functions,
 * no DOM: building masks from rectangles, ellipses and lasso polygons,
 * combining them, inverting, bounds, and the outline drawn as marching ants.
 */

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type Combine = 'replace' | 'add' | 'subtract' | 'intersect';

/** Integer rectangle clipped to the image (null if empty). */
export function clipRect(r: Rect, w: number, h: number): Rect | null {
  const x0 = Math.max(0, Math.floor(Math.min(r.x, r.x + r.w)));
  const y0 = Math.max(0, Math.floor(Math.min(r.y, r.y + r.h)));
  const x1 = Math.min(w, Math.ceil(Math.max(r.x, r.x + r.w)));
  const y1 = Math.min(h, Math.ceil(Math.max(r.y, r.y + r.h)));
  return x1 > x0 && y1 > y0 ? { x: x0, y: y0, w: x1 - x0, h: y1 - y0 } : null;
}

export function unionRect(a: Rect | null, b: Rect | null): Rect | null {
  if (!a) return b;
  if (!b) return a;
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
}

export function rectMask(w: number, h: number, r: Rect): Uint8Array {
  const m = new Uint8Array(w * h);
  const c = clipRect(r, w, h);
  if (!c) return m;
  for (let y = c.y; y < c.y + c.h; y++) m.fill(255, y * w + c.x, y * w + c.x + c.w);
  return m;
}

export function ellipseMask(w: number, h: number, r: Rect): Uint8Array {
  const m = new Uint8Array(w * h);
  const rx = Math.abs(r.w) / 2;
  const ry = Math.abs(r.h) / 2;
  if (rx < 0.5 || ry < 0.5) return m;
  const cx = Math.min(r.x, r.x + r.w) + rx;
  const cy = Math.min(r.y, r.y + r.h) + ry;
  const y0 = Math.max(0, Math.floor(cy - ry));
  const y1 = Math.min(h, Math.ceil(cy + ry));
  for (let y = y0; y < y1; y++) {
    const dy = (y + 0.5 - cy) / ry;
    if (dy * dy > 1) continue;
    const half = rx * Math.sqrt(1 - dy * dy);
    const x0 = Math.max(0, Math.round(cx - half));
    const x1 = Math.min(w, Math.round(cx + half));
    if (x1 > x0) m.fill(255, y * w + x0, y * w + x1);
  }
  return m;
}

/** A lasso: the polygon's inside (even-odd rule, sampled at pixel centres). */
export function polygonMask(w: number, h: number, pts: { x: number; y: number }[]): Uint8Array {
  const m = new Uint8Array(w * h);
  if (pts.length < 3) return m;
  let minY = Infinity;
  let maxY = -Infinity;
  for (const p of pts) {
    minY = Math.min(minY, p.y);
    maxY = Math.max(maxY, p.y);
  }
  const xs: number[] = [];
  for (let y = Math.max(0, Math.floor(minY)); y < Math.min(h, Math.ceil(maxY)); y++) {
    const sy = y + 0.5;
    xs.length = 0;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const a = pts[i];
      const b = pts[j];
      if (a.y > sy !== b.y > sy) xs.push(a.x + ((sy - a.y) / (b.y - a.y)) * (b.x - a.x));
    }
    xs.sort((p, q) => p - q);
    for (let k = 0; k + 1 < xs.length; k += 2) {
      const x0 = Math.max(0, Math.round(xs[k]));
      const x1 = Math.min(w, Math.round(xs[k + 1]));
      if (x1 > x0) m.fill(255, y * w + x0, y * w + x1);
    }
  }
  return m;
}

/** Combine a new mask with the current selection (null = nothing selected). */
export function combine(cur: Uint8Array | null, next: Uint8Array, mode: Combine): Uint8Array {
  if (!cur || mode === 'replace') return next;
  const out = new Uint8Array(next.length);
  for (let i = 0; i < next.length; i++) {
    const a = cur[i];
    const b = next[i];
    out[i] = mode === 'add' ? Math.max(a, b) : mode === 'subtract' ? Math.min(a, 255 - b) : Math.min(a, b);
  }
  return out;
}

export function invertMask(m: Uint8Array): Uint8Array {
  const out = new Uint8Array(m.length);
  for (let i = 0; i < m.length; i++) out[i] = 255 - m[i];
  return out;
}

/** Bounding box of the selected pixels (null when nothing is selected). */
export function maskBounds(m: Uint8Array, w: number, h: number, min = 1): Rect | null {
  let x0 = w;
  let y0 = h;
  let x1 = -1;
  let y1 = -1;
  for (let y = 0; y < h; y++) {
    const row = y * w;
    let first = -1;
    for (let x = 0; x < w; x++)
      if (m[row + x] >= min) {
        first = x;
        break;
      }
    if (first < 0) continue;
    let last = first;
    for (let x = w - 1; x > first; x--)
      if (m[row + x] >= min) {
        last = x;
        break;
      }
    if (first < x0) x0 = first;
    if (last > x1) x1 = last;
    if (y < y0) y0 = y;
    y1 = y;
  }
  return x1 < 0 ? null : { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 };
}

/** Move a mask by (dx, dy) whole pixels (what leaves the image is dropped). */
export function translateMask(m: Uint8Array, w: number, h: number, dx: number, dy: number): Uint8Array {
  const out = new Uint8Array(m.length);
  dx = Math.round(dx);
  dy = Math.round(dy);
  for (let y = 0; y < h; y++) {
    const sy = y - dy;
    if (sy < 0 || sy >= h) continue;
    const x0 = Math.max(0, dx);
    const x1 = Math.min(w, w + dx);
    if (x1 > x0) out.set(m.subarray(sy * w + x0 - dx, sy * w + x1 - dx), y * w + x0);
  }
  return out;
}

/**
 * The outline of the selection as line segments [x1, y1, x2, y2, …] on pixel
 * edges between selected (≥ threshold) and unselected pixels; neighbouring
 * unit edges are merged into runs, so a rectangle is four segments.
 */
export function maskEdges(m: Uint8Array, w: number, h: number, threshold = 128): number[] {
  const out: number[] = [];
  const on = (x: number, y: number) => x >= 0 && y >= 0 && x < w && y < h && m[y * w + x] >= threshold;
  // horizontal edges: between row y-1 and row y
  for (let y = 0; y <= h; y++) {
    let start = -1;
    let kind = 0;
    for (let x = 0; x <= w; x++) {
      const k = x < w ? (on(x, y - 1) ? 1 : 0) - (on(x, y) ? 1 : 0) : 0;
      if (k !== kind) {
        if (kind !== 0) out.push(start, y, x, y);
        start = x;
        kind = k;
      }
    }
  }
  // vertical edges: between column x-1 and column x
  for (let x = 0; x <= w; x++) {
    let start = -1;
    let kind = 0;
    for (let y = 0; y <= h; y++) {
      const k = y < h ? (on(x - 1, y) ? 1 : 0) - (on(x, y) ? 1 : 0) : 0;
      if (k !== kind) {
        if (kind !== 0) out.push(x, start, x, y);
        start = y;
        kind = k;
      }
    }
  }
  return out;
}

/** Copy of a mask region (for clipping a stroke to the selection). */
export function cropMask(m: Uint8Array, w: number, r: Rect): Uint8Array {
  const out = new Uint8Array(r.w * r.h);
  for (let y = 0; y < r.h; y++) out.set(m.subarray((r.y + y) * w + r.x, (r.y + y) * w + r.x + r.w), y * r.w);
  return out;
}

/** Resize a mask (nearest neighbour; selections are re-made rather than resampled for quality). */
export function resizeMask(m: Uint8Array, w: number, h: number, nw: number, nh: number): Uint8Array {
  const out = new Uint8Array(nw * nh);
  for (let y = 0; y < nh; y++) {
    const sy = Math.min(h - 1, Math.floor(((y + 0.5) * h) / nh));
    for (let x = 0; x < nw; x++) out[y * nw + x] = m[sy * w + Math.min(w - 1, Math.floor(((x + 0.5) * w) / nw))];
  }
  return out;
}
