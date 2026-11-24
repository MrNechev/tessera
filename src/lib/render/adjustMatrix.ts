/**
 * The photo adjustments of `applyAdjustPixels` as one affine colour matrix.
 *
 * Every step there (brightness, contrast, saturation, warmth, grayscale,
 * sepia, fade) is linear in r, g, b plus a constant, and nothing is clamped
 * until the end, so the whole chain is a single 3×4 matrix. That lets the
 * browser apply exactly the same look with an SVG `feColorMatrix` — to
 * images *and* videos in documents, without making copies of the pixels.
 */
import type { PhotoAdjust } from '../model/types';

/** Row-major 3×4: [r-row (4), g-row (4), b-row (4)], last column = offset (0..1). */
export type Affine = number[];

const ID: Affine = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0];

/** `m2 ∘ m1` (apply m1 first). */
function compose(m2: Affine, m1: Affine): Affine {
  const out = new Array(12).fill(0);
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 4; c++) {
      let v = 0;
      for (let k = 0; k < 3; k++) v += m2[r * 4 + k] * m1[k * 4 + c];
      if (c === 3) v += m2[r * 4 + 3];
      out[r * 4 + c] = v;
    }
  }
  return out;
}

const LR = 0.2126;
const LG = 0.7152;
const LB = 0.0722;

/** x → lum + (x − lum)·s, i.e. mix towards luminance by (1 − s). */
function saturate(s: number): Affine {
  const k = 1 - s;
  return [LR * k + s, LG * k, LB * k, 0, LR * k, LG * k + s, LB * k, 0, LR * k, LG * k, LB * k + s, 0];
}

export function adjustAffine(a: PhotoAdjust): Affine {
  let m = ID;
  if (a.exposure) {
    const e = Math.pow(2, a.exposure);
    m = compose([e, 0, 0, 0, 0, e, 0, 0, 0, 0, e, 0], m);
  }
  const bright = 1 + a.brightness * 0.6;
  m = compose([bright, 0, 0, 0, 0, bright, 0, 0, 0, 0, bright, 0], m);
  const c = 1 + a.contrast;
  const o = 0.5 - 0.5 * c;
  m = compose([c, 0, 0, o, 0, c, 0, o, 0, 0, c, o], m);
  m = compose(saturate(1 + a.saturation), m);
  const warm = a.warmth * 0.09;
  m = compose([1, 0, 0, warm, 0, 1, 0, warm * 0.25, 0, 0, 1, -warm], m);
  if (a.tint) {
    const t = a.tint * 0.09;
    m = compose([1, 0, 0, t * 0.5, 0, 1, 0, -t, 0, 0, 1, t * 0.5], m);
  }
  if (a.grayscale > 0) m = compose(saturate(1 - a.grayscale), m);
  if (a.sepia > 0) {
    const p = a.sepia;
    const q = 1 - p;
    m = compose(
      [q + 0.393 * p, 0.769 * p, 0.189 * p, 0, 0.349 * p, q + 0.686 * p, 0.168 * p, 0, 0.272 * p, 0.534 * p, q + 0.131 * p, 0],
      m,
    );
  }
  const fs = 1 - a.fade * 0.22;
  const fl = a.fade * 0.14;
  m = compose([fs, 0, 0, fl, 0, fs, 0, fl, 0, 0, fs, fl], m);
  return m;
}

/** The `values` of an SVG `<feColorMatrix type="matrix">` (4×5, alpha unchanged). */
export function feColorMatrixValues(a: PhotoAdjust): string {
  const m = adjustAffine(a);
  const row = (r: number) => [m[r * 4], m[r * 4 + 1], m[r * 4 + 2], 0, m[r * 4 + 3]];
  return [...row(0), ...row(1), ...row(2), 0, 0, 0, 1, 0].map((v) => +v.toFixed(5)).join(' ');
}
