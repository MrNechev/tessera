/**
 * Tessera's photo look, the one pixel pipeline behind every editor.
 *
 * The collage renderer, story photos, the "edit a copy" tool and exports all
 * run `applyLookPixels`, so a look is identical everywhere. The first stages
 * (colour matrix, tone curve, duotone) are also expressible as an SVG filter
 * (`looks.svelte.ts`), which is what videos use; the remaining "pixel-only"
 * stages (vibrance, clarity, sharpen, blur, grain, colour pop, hidden areas)
 * need real pixels.
 *
 * Order: colour matrix → tone curve → vibrance → colour pop → duotone →
 * clarity/sharpen/blur → hidden areas → grain. (Vignette is drawn on top by
 * whoever draws the photo.)
 */
import type { PhotoAdjust, HiddenArea } from '../model/types';
import { adjustAffine } from './adjustMatrix';

const LR = 0.2126;
const LG = 0.7152;
const LB = 0.0722;

/** Number of samples of the tone curve (shared with feComponentTransfer tables). */
export const CURVE_SAMPLES = 33;

/**
 * Highlights/shadows as a monotonic per-channel curve, sampled at
 * CURVE_SAMPLES points (linear interpolation in between, like SVG tables).
 */
export function toneCurve(a: Pick<PhotoAdjust, 'highlights' | 'shadows'>): number[] | null {
  const s = a.shadows ?? 0;
  const h = a.highlights ?? 0;
  if (!s && !h) return null;
  const out: number[] = [];
  for (let i = 0; i < CURVE_SAMPLES; i++) {
    const x = i / (CURVE_SAMPLES - 1);
    // bumps centred on the dark and bright thirds; 0 at black and white
    const ws = 4 * x * (1 - x) * (1 - x);
    const wh = 4 * x * x * (1 - x);
    out.push(Math.min(1, Math.max(0, x + 0.35 * (s * ws + h * wh))));
  }
  return out;
}

function curveLut(curve: number[]): Uint8ClampedArray {
  const lut = new Uint8ClampedArray(256);
  const n = curve.length - 1;
  for (let v = 0; v < 256; v++) {
    const t = (v / 255) * n;
    const i = Math.min(n - 1, Math.floor(t));
    const f = t - i;
    lut[v] = Math.round((curve[i] * (1 - f) + curve[i + 1] * f) * 255);
  }
  return lut;
}

export function hexRgb(hex: string): [number, number, number] {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return [0, 0, 0];
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** True if the colour matrix changes anything. */
export function hasMatrix(a: PhotoAdjust): boolean {
  return !!(a.brightness || a.contrast || a.saturation || a.warmth || a.fade || a.grayscale || a.sepia || a.exposure || a.tint);
}

/** Stages an SVG filter can't do (images then go through the pixel pipeline). */
export function pixelOnly(a: PhotoAdjust | null | undefined, hidden?: HiddenArea[] | null): boolean {
  if (hidden?.length) return true;
  if (!a) return false;
  return !!(a.vibrance || a.clarity || a.sharpen || a.blur || a.grain || (a.pop && a.pop.amount > 0));
}

/** Anything that changes pixels (vignette excluded: it is drawn on top). */
export function changesPixels(a: PhotoAdjust | null | undefined, hidden?: HiddenArea[] | null): boolean {
  if (hidden?.length) return true;
  if (!a) return false;
  return hasMatrix(a) || !!toneCurve(a) || !!(a.duotone && a.duotone.amount > 0) || pixelOnly(a);
}

/* ------------------------------------------------------------ blur helpers */

/** In-place separable box blur of one channel-interleaved RGBA buffer (3 passes ≈ gaussian). */
function boxBlurRGBA(src: Uint8ClampedArray, w: number, h: number, r: number): Uint8ClampedArray {
  if (r < 1) return src.slice();
  let a = src.slice();
  let b = new Uint8ClampedArray(src.length);
  for (let pass = 0; pass < 3; pass++) {
    blurH(a, b, w, h, r);
    blurV(b, a, w, h, r);
  }
  return a;
  function blurH(i: Uint8ClampedArray, o: Uint8ClampedArray, w: number, h: number, r: number) {
    const d = 2 * r + 1;
    for (let y = 0; y < h; y++) {
      const row = y * w * 4;
      for (let c = 0; c < 3; c++) {
        let acc = 0;
        for (let k = -r; k <= r; k++) acc += i[row + Math.min(w - 1, Math.max(0, k)) * 4 + c];
        for (let x = 0; x < w; x++) {
          o[row + x * 4 + c] = acc / d;
          const add = Math.min(w - 1, x + r + 1);
          const sub = Math.max(0, x - r);
          acc += i[row + add * 4 + c] - i[row + sub * 4 + c];
        }
      }
      for (let x = 0; x < w; x++) o[row + x * 4 + 3] = i[row + x * 4 + 3];
    }
  }
  function blurV(i: Uint8ClampedArray, o: Uint8ClampedArray, w: number, h: number, r: number) {
    const d = 2 * r + 1;
    for (let x = 0; x < w; x++) {
      for (let c = 0; c < 3; c++) {
        let acc = 0;
        for (let k = -r; k <= r; k++) acc += i[(Math.min(h - 1, Math.max(0, k)) * w + x) * 4 + c];
        for (let y = 0; y < h; y++) {
          o[(y * w + x) * 4 + c] = acc / d;
          const add = Math.min(h - 1, y + r + 1);
          const sub = Math.max(0, y - r);
          acc += i[(add * w + x) * 4 + c] - i[(sub * w + x) * 4 + c];
        }
      }
      for (let y = 0; y < h; y++) o[(y * w + x) * 4 + 3] = i[(y * w + x) * 4 + 3];
    }
  }
}

/** Deterministic noise, so a look renders the same every time. */
function rng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return ((s >>> 0) / 4294967296) * 2 - 1;
  };
}

/* ------------------------------------------------------------ the pipeline */

/**
 * Apply a look to RGBA pixels of a w × h image in place. Sizes (blur radius,
 * grain, pixelation) are relative to the image's shorter side, so a 400 px
 * preview and a 6000 px export look alike.
 */
export function applyLookPixels(d: Uint8ClampedArray, w: number, h: number, a: PhotoAdjust, hidden?: HiddenArea[] | null): void {
  const short = Math.max(1, Math.min(w, h));
  const n = w * h * 4;
  // 1. colour matrix (exact twin of the SVG feColorMatrix)
  if (hasMatrix(a)) {
    const m = adjustAffine(a);
    for (let i = 0; i < n; i += 4) {
      const r = d[i] / 255;
      const g = d[i + 1] / 255;
      const b = d[i + 2] / 255;
      d[i] = (m[0] * r + m[1] * g + m[2] * b + m[3]) * 255;
      d[i + 1] = (m[4] * r + m[5] * g + m[6] * b + m[7]) * 255;
      d[i + 2] = (m[8] * r + m[9] * g + m[10] * b + m[11]) * 255;
    }
  }
  // 2. tone curve (twin of feComponentTransfer type="table")
  const curve = toneCurve(a);
  if (curve) {
    const lut = curveLut(curve);
    for (let i = 0; i < n; i += 4) {
      d[i] = lut[d[i]];
      d[i + 1] = lut[d[i + 1]];
      d[i + 2] = lut[d[i + 2]];
    }
  }
  // 3. vibrance: saturate the dull colours more than the vivid ones
  if (a.vibrance) {
    const v = a.vibrance;
    for (let i = 0; i < n; i += 4) {
      const r = d[i];
      const g = d[i + 1];
      const b = d[i + 2];
      const mx = Math.max(r, g, b);
      const mn = Math.min(r, g, b);
      const sat = mx ? (mx - mn) / mx : 0;
      const k = 1 + v * (1 - sat) * (v > 0 ? 1 : 0.8);
      const l = LR * r + LG * g + LB * b;
      d[i] = l + (r - l) * k;
      d[i + 1] = l + (g - l) * k;
      d[i + 2] = l + (b - l) * k;
    }
  }
  // 4. colour pop: keep one hue, the rest goes towards grey
  if (a.pop && a.pop.amount > 0) {
    const { hue, range, amount } = a.pop;
    const half = Math.max(5, range) / 2;
    for (let i = 0; i < n; i += 4) {
      const r = d[i] / 255;
      const g = d[i + 1] / 255;
      const b = d[i + 2] / 255;
      const mx = Math.max(r, g, b);
      const mn = Math.min(r, g, b);
      let keep = 0;
      if (mx - mn > 0.06) {
        let hh: number;
        const c = mx - mn;
        if (mx === r) hh = ((g - b) / c) % 6;
        else if (mx === g) hh = (b - r) / c + 2;
        else hh = (r - g) / c + 4;
        hh = (hh * 60 + 360) % 360;
        const dist = Math.min(Math.abs(hh - hue), 360 - Math.abs(hh - hue));
        keep = dist <= half ? 1 : dist >= half * 1.6 ? 0 : 1 - (dist - half) / (half * 0.6);
        keep *= Math.min(1, (mx - mn) / 0.2);
      }
      const k = amount * (1 - keep);
      const l = (LR * r + LG * g + LB * b) * 255;
      d[i] += (l - d[i]) * k;
      d[i + 1] += (l - d[i + 1]) * k;
      d[i + 2] += (l - d[i + 2]) * k;
    }
  }
  // 5. duotone (twin of the SVG luminance → table → arithmetic blend)
  if (a.duotone && a.duotone.amount > 0) {
    const [dr, dg, db] = hexRgb(a.duotone.dark);
    const [lr, lg, lb] = hexRgb(a.duotone.light);
    const k = Math.min(1, a.duotone.amount);
    for (let i = 0; i < n; i += 4) {
      const l = (LR * d[i] + LG * d[i + 1] + LB * d[i + 2]) / 255;
      d[i] = d[i] * (1 - k) + (dr + (lr - dr) * l) * k;
      d[i + 1] = d[i + 1] * (1 - k) + (dg + (lg - dg) * l) * k;
      d[i + 2] = d[i + 2] * (1 - k) + (db + (lb - db) * l) * k;
    }
  }
  // 6. clarity (large-radius unsharp mask), sharpen (small radius), blur
  if (a.clarity) {
    const blurred = boxBlurRGBA(d, w, h, Math.max(2, Math.round(short * 0.02)));
    const k = a.clarity * 0.8;
    for (let i = 0; i < n; i += 4) for (let c = 0; c < 3; c++) d[i + c] = d[i + c] + (d[i + c] - blurred[i + c]) * k;
  }
  if (a.sharpen) {
    const blurred = boxBlurRGBA(d, w, h, Math.max(1, Math.round(short * 0.0012)));
    const k = a.sharpen * 1.6;
    for (let i = 0; i < n; i += 4) for (let c = 0; c < 3; c++) d[i + c] = d[i + c] + (d[i + c] - blurred[i + c]) * k;
  }
  if (a.blur) {
    const r = Math.round(short * 0.02 * a.blur);
    if (r >= 1) d.set(boxBlurRGBA(d, w, h, r));
  }
  // 7. hidden areas: blur or pixelate rectangles (faces, number plates)
  for (const area of hidden ?? []) hideArea(d, w, h, area);
  // 8. grain
  if (a.grain) {
    const next = rng(w * 7919 + h);
    const amp = a.grain * 38;
    for (let i = 0; i < n; i += 4) {
      const g = next() * amp;
      d[i] += g;
      d[i + 1] += g;
      d[i + 2] += g;
    }
  }
}

function hideArea(d: Uint8ClampedArray, w: number, h: number, area: HiddenArea) {
  const x0 = Math.max(0, Math.floor(area.x * w));
  const y0 = Math.max(0, Math.floor(area.y * h));
  const x1 = Math.min(w, Math.ceil((area.x + area.w) * w));
  const y1 = Math.min(h, Math.ceil((area.y + area.h) * h));
  const rw = x1 - x0;
  const rh = y1 - y0;
  if (rw < 2 || rh < 2) return;
  if (area.mode === 'pixelate') {
    const cell = Math.max(2, Math.round(Math.min(w, h) * 0.025));
    for (let cy = y0; cy < y1; cy += cell)
      for (let cx = x0; cx < x1; cx += cell) {
        const ex = Math.min(x1, cx + cell);
        const ey = Math.min(y1, cy + cell);
        let r = 0;
        let g = 0;
        let b = 0;
        let c = 0;
        for (let y = cy; y < ey; y++)
          for (let x = cx; x < ex; x++) {
            const i = (y * w + x) * 4;
            r += d[i];
            g += d[i + 1];
            b += d[i + 2];
            c++;
          }
        for (let y = cy; y < ey; y++)
          for (let x = cx; x < ex; x++) {
            const i = (y * w + x) * 4;
            d[i] = r / c;
            d[i + 1] = g / c;
            d[i + 2] = b / c;
          }
      }
    return;
  }
  // blur: cut the rectangle out, blur it strongly, put it back
  const sub = new Uint8ClampedArray(rw * rh * 4);
  for (let y = 0; y < rh; y++) sub.set(d.subarray(((y0 + y) * w + x0) * 4, ((y0 + y) * w + x1) * 4), y * rw * 4);
  const bl = boxBlurRGBA(sub, rw, rh, Math.max(2, Math.round(Math.min(w, h) * 0.02)));
  for (let y = 0; y < rh; y++) d.set(bl.subarray(y * rw * 4, (y + 1) * rw * 4), ((y0 + y) * w + x0) * 4);
}

/* ------------------------------------------------------------ auto enhance */

/**
 * One-click "Auto": stretch the tones between the 1st and 99th percentile,
 * balance the colour cast (grey world), and give dull photos a little
 * vibrance. Returns adjustments to merge into the current ones.
 */
export function autoAdjust(d: Uint8ClampedArray): Partial<PhotoAdjust> {
  const hist = new Uint32Array(256);
  let sr = 0;
  let sg = 0;
  let sb = 0;
  let sat = 0;
  let count = 0;
  for (let i = 0; i < d.length; i += 16) {
    const r = d[i];
    const g = d[i + 1];
    const b = d[i + 2];
    hist[Math.round(LR * r + LG * g + LB * b)]++;
    sr += r;
    sg += g;
    sb += b;
    const mx = Math.max(r, g, b);
    sat += mx ? (mx - Math.min(r, g, b)) / mx : 0;
    count++;
  }
  if (!count) return {};
  const pct = (p: number) => {
    let acc = 0;
    for (let v = 0; v < 256; v++) {
      acc += hist[v];
      if (acc >= count * p) return v;
    }
    return 255;
  };
  const lo = pct(0.01) / 255;
  const hi = pct(0.99) / 255;
  const mid = pct(0.5) / 255;
  const span = Math.max(0.2, hi - lo);
  const clamp = (v: number, a = -1, b = 1) => Math.round(Math.min(b, Math.max(a, v)) * 100) / 100;
  // contrast c maps the span to ~0.95; exposure moves the middle towards 0.5
  const contrast = clamp(0.95 / span - 1, -0.3, 0.6);
  const exposure = clamp(Math.log2(0.48 / Math.max(0.05, mid)) * 0.6, -1, 1);
  const mr = sr / count;
  const mg = sg / count;
  const mb = sb / count;
  const warmth = clamp(((mb - mr) / 255) * 3, -0.6, 0.6);
  const tint = clamp(((mg - (mr + mb) / 2) / 255) * 3, -0.6, 0.6);
  const vibrance = clamp(0.35 - sat / count, 0, 0.35);
  return { contrast, exposure, warmth, tint, vibrance };
}
