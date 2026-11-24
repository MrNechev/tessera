/**
 * Pure helpers that turn the segmentation model's small, soft mask into the
 * alpha channel of a cut-out: resize to the photo, threshold, feather, the
 * erase/restore brush, cropping and compositing. No DOM, so they run (and are
 * tested) anywhere; the dialog calls them at preview size while you refine
 * and once more at full size when saving.
 */

/** One brush dab in image-independent coordinates (fractions of width/height). */
export interface Dab {
  x: number;
  y: number;
  /** Radius as a fraction of the image's longer side. */
  r: number;
  mode: 'erase' | 'restore';
}

export interface MaskOptions {
  /** 0..1: mask values below this become transparent. */
  threshold: number;
  /** 0..1: width of the soft ramp around the threshold (0 = hard edge). */
  softness: number;
  /** Feather radius as a fraction of the longer side (0 = none). */
  feather: number;
  dabs?: Dab[];
}

export const DEFAULT_MASK: MaskOptions = { threshold: 0.5, softness: 0.5, feather: 0.0015 };

/** Stretch the model's output to the full 0..1 range (U²-Net's raw range varies per image). */
export function normalizeMask(m: Float32Array): Float32Array {
  let lo = Infinity;
  let hi = -Infinity;
  for (const v of m) {
    if (v < lo) lo = v;
    if (v > hi) hi = v;
  }
  const out = new Float32Array(m.length);
  const span = hi - lo;
  if (!(span > 1e-6)) return out;
  for (let i = 0; i < m.length; i++) out[i] = (m[i] - lo) / span;
  return out;
}

/** Bilinear resize of a single-channel float image (pixel centres aligned). */
export function resizeMask(src: Float32Array, sw: number, sh: number, dw: number, dh: number): Float32Array {
  const out = new Float32Array(dw * dh);
  const fx = sw / dw;
  const fy = sh / dh;
  for (let y = 0; y < dh; y++) {
    const sy = Math.min(sh - 1, Math.max(0, (y + 0.5) * fy - 0.5));
    const y0 = Math.floor(sy);
    const y1 = Math.min(sh - 1, y0 + 1);
    const ty = sy - y0;
    const r0 = y0 * sw;
    const r1 = y1 * sw;
    for (let x = 0; x < dw; x++) {
      const sx = Math.min(sw - 1, Math.max(0, (x + 0.5) * fx - 0.5));
      const x0 = Math.floor(sx);
      const x1 = Math.min(sw - 1, x0 + 1);
      const tx = sx - x0;
      const top = src[r0 + x0] + (src[r0 + x1] - src[r0 + x0]) * tx;
      const bot = src[r1 + x0] + (src[r1 + x1] - src[r1 + x0]) * tx;
      out[y * dw + x] = top + (bot - top) * ty;
    }
  }
  return out;
}

/**
 * Threshold with a smooth ramp: values well above `threshold` become opaque,
 * well below transparent, and a band of width `softness` in between keeps
 * the model's soft edge (hair, fur). In place; returns `m`.
 */
export function applyLevels(m: Float32Array, threshold: number, softness: number): Float32Array {
  const half = Math.max(1e-3, softness / 2);
  const lo = threshold - half;
  const span = 2 * half;
  for (let i = 0; i < m.length; i++) {
    const t = Math.min(1, Math.max(0, (m[i] - lo) / span));
    m[i] = t * t * (3 - 2 * t);
  }
  return m;
}

/** Horizontal then vertical running-sum box blur of radius `r` px, once. */
function boxBlur(m: Float32Array, w: number, h: number, r: number): void {
  const tmp = new Float32Array(Math.max(w, h));
  const n = 2 * r + 1;
  for (let y = 0; y < h; y++) {
    const row = y * w;
    let acc = 0;
    for (let k = -r; k <= r; k++) acc += m[row + Math.min(w - 1, Math.max(0, k))];
    for (let x = 0; x < w; x++) {
      tmp[x] = acc / n;
      acc += m[row + Math.min(w - 1, x + r + 1)] - m[row + Math.max(0, x - r)];
    }
    m.set(tmp.subarray(0, w), row);
  }
  for (let x = 0; x < w; x++) {
    let acc = 0;
    for (let k = -r; k <= r; k++) acc += m[Math.min(h - 1, Math.max(0, k)) * w + x];
    for (let y = 0; y < h; y++) {
      tmp[y] = acc / n;
      acc += m[Math.min(h - 1, y + r + 1) * w + x] - m[Math.max(0, y - r) * w + x];
    }
    for (let y = 0; y < h; y++) m[y * w + x] = tmp[y];
  }
}

/**
 * Soften the edge a little so the cut-out doesn't look jagged when the
 * 320 px mask is stretched over a large photo. Two box passes ≈ a tent
 * filter. In place; returns `m`.
 */
export function feather(m: Float32Array, w: number, h: number, radius: number): Float32Array {
  const r = Math.round(radius);
  if (r < 1 || w < 2 || h < 2) return m;
  boxBlur(m, w, h, r);
  boxBlur(m, w, h, r);
  return m;
}

/**
 * Apply erase/restore brush dabs. Each dab has a soft rim (the outer 30 %)
 * and only ever lowers (erase) or raises (restore) alpha, so overlapping
 * dabs don't add up into hard blotches. In place; returns `m`.
 */
export function paintDabs(m: Float32Array, w: number, h: number, dabs: Dab[]): Float32Array {
  const long = Math.max(w, h);
  for (const d of dabs) {
    const cx = d.x * w;
    const cy = d.y * h;
    const r = Math.max(0.5, d.r * long);
    const inner = r * 0.7;
    const x0 = Math.max(0, Math.floor(cx - r));
    const x1 = Math.min(w - 1, Math.ceil(cx + r));
    const y0 = Math.max(0, Math.floor(cy - r));
    const y1 = Math.min(h - 1, Math.ceil(cy + r));
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        const dist = Math.hypot(x + 0.5 - cx, y + 0.5 - cy);
        if (dist >= r) continue;
        const s = dist <= inner ? 1 : 1 - (dist - inner) / (r - inner);
        const i = y * w + x;
        m[i] = d.mode === 'erase' ? Math.min(m[i], 1 - s) : Math.max(m[i], s);
      }
    }
  }
  return m;
}

/** The whole pipeline: model mask (any size) → alpha (0..1) at `w` × `h`. */
export function buildAlpha(raw: Float32Array, rw: number, rh: number, w: number, h: number, o: MaskOptions): Float32Array {
  const m = resizeMask(raw, rw, rh, w, h);
  applyLevels(m, o.threshold, o.softness);
  feather(m, w, h, o.feather * Math.max(w, h));
  if (o.dabs?.length) paintDabs(m, w, h, o.dabs);
  return m;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * Bounding box of the visible part (alpha above `min`), grown by `pad` (a
 * fraction of the box's longer side) and clamped to the image, so stickers
 * aren't mostly empty space. Null if nothing is visible.
 */
export function alphaBounds(a: Float32Array, w: number, h: number, min = 0.03, pad = 0.02): Rect | null {
  let x0 = w;
  let y0 = h;
  let x1 = -1;
  let y1 = -1;
  for (let y = 0; y < h; y++) {
    const row = y * w;
    for (let x = 0; x < w; x++) {
      if (a[row + x] > min) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        y1 = y;
      }
    }
  }
  if (x1 < 0) return null;
  const p = Math.round(Math.max(x1 - x0 + 1, y1 - y0 + 1) * pad);
  x0 = Math.max(0, x0 - p);
  y0 = Math.max(0, y0 - p);
  x1 = Math.min(w - 1, x1 + p);
  y1 = Math.min(h - 1, y1 + p);
  return { x: x0, y: y0, w: x1 - x0 + 1, h: y1 - y0 + 1 };
}

/**
 * Straight (non-premultiplied) RGBA of `rect` with `alpha` as its alpha
 * channel. Fully transparent pixels are zeroed so the PNG compresses well.
 */
export function composite(rgba: Uint8ClampedArray, alpha: Float32Array, w: number, rect: Rect): Uint8ClampedArray<ArrayBuffer> {
  const out = new Uint8ClampedArray(rect.w * rect.h * 4);
  for (let y = 0; y < rect.h; y++) {
    for (let x = 0; x < rect.w; x++) {
      const si = (rect.y + y) * w + rect.x + x;
      const di = (y * rect.w + x) * 4;
      const a = Math.round(alpha[si] * 255);
      if (a === 0) continue;
      out[di] = rgba[si * 4];
      out[di + 1] = rgba[si * 4 + 1];
      out[di + 2] = rgba[si * 4 + 2];
      out[di + 3] = a;
    }
  }
  return out;
}

/** Intersection over union of two masks binarised at `t` (for tests and evaluation). */
export function iou(a: ArrayLike<number>, b: ArrayLike<number>, t = 0.5): number {
  let inter = 0;
  let union = 0;
  for (let i = 0; i < a.length; i++) {
    const p = a[i] > t;
    const q = b[i] > t;
    if (p && q) inter++;
    if (p || q) union++;
  }
  return union ? inter / union : 1;
}
