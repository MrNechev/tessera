/**
 * Photo looks for stories, the canvas, presentations and exports — the same
 * toolkit as the collage editor (`render/look.ts` is the shared pipeline).
 *
 * A look is stored on each media reference; the original file is never
 * touched. On screen there are two ways to show it:
 * - an SVG filter (`feColorMatrix` + tone-curve tables + duotone + vignette),
 *   exact twins of the pixel pipeline's first stages, registered once per
 *   distinct look and referenced by CSS `filter: url(#…)`. Works on videos.
 * - for images whose look needs real pixels (vibrance, clarity, sharpen,
 *   blur, grain, colour pop, hidden areas): the thumbnail is run through the
 *   pixel pipeline once and shown as a blob URL (`lookedUrl`).
 */
import { neutralAdjust } from '../model/factory';
import type { HiddenArea, MaskShape, PhotoAdjust } from '../model/types';
import { adjustKey } from '../render/adjust';
import { feColorMatrixValues } from '../render/adjustMatrix';
import { applyLookPixels, changesPixels, hasMatrix, hexRgb, pixelOnly, toneCurve } from '../render/look';

export type FrameStyle = 'polaroid' | 'instant' | 'mat' | 'film' | 'tape';

export interface Look {
  adjust?: PhotoAdjust;
  /** The filter preset the adjustment started from (only for the UI). */
  filter?: string;
  flipX?: boolean;
  flipY?: boolean;
  /** Quarter turns, clockwise. */
  rotate?: 0 | 90 | 180 | 270;
  /** Small rotation in degrees (−45…45), zoomed so no corners show. */
  straighten?: number;
  hideAreas?: HiddenArea[];
  mask?: MaskShape;
  frame?: FrameStyle;
  frameColor?: string;
  shadow?: boolean;
  /** Border width as a fraction of the photo's shorter side, and its colour. */
  border?: { width: number; color: string };
}

export type Crop = { x: number; y: number; w: number; h: number };

export function fullAdjust(a: Partial<PhotoAdjust> | undefined): PhotoAdjust {
  return { ...neutralAdjust(), ...(a ?? {}) };
}

/** True if the look changes how the media looks at all. */
export function hasLook(l: Look | null | undefined): boolean {
  if (!l) return false;
  return (
    changesPixels(l.adjust) ||
    (l.adjust?.vignette ?? 0) > 0 ||
    !!l.hideAreas?.length ||
    !!l.flipX ||
    !!l.flipY ||
    !!l.rotate ||
    !!l.straighten ||
    (!!l.mask && l.mask !== 'rect') ||
    !!l.frame ||
    !!l.shadow ||
    !!l.border
  );
}

/** Does showing this image need the pixel pipeline (not just an SVG filter)? */
export function needsPixels(l: Look | null | undefined): boolean {
  return !!l && pixelOnly(l.adjust, l.hideAreas);
}

/* ------------------------------------------------------------ SVG filters */

type Def = {
  matrix: string | null;
  curve: string | null;
  duotone: { r: string; g: string; b: string; amount: number } | null;
  vignette: number;
};
const defs = new Map<string, Def>();
let flushing = false;
/**
 * Every look used so far, for LookFilters. Filters are registered while
 * styles are computed (inside deriveds), so the reactive list is updated in a
 * microtask.
 */
export const lookDefs = $state<{ list: [string, Def][] }>({ list: [] });

function register(id: string, d: Def) {
  defs.set(id, d);
  if (flushing) return;
  flushing = true;
  queueMicrotask(() => {
    flushing = false;
    lookDefs.list = [...defs];
  });
}

function hash(s: string): string {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return (h >>> 0).toString(36);
}

/** CSS `filter` value for an adjustment's SVG-capable stages ('' when nothing changes). */
export function lookFilter(a: PhotoAdjust | undefined): string {
  if (!a) return '';
  const curve = toneCurve(a);
  const duo = a.duotone && a.duotone.amount > 0 ? a.duotone : null;
  const vig = Math.max(0, Math.min(1, a.vignette || 0));
  if (!hasMatrix(a) && !curve && !duo && !vig) return '';
  const id = 'tlook-' + hash(adjustKey(a) + '|' + vig);
  if (!defs.has(id)) {
    let duotone: Def['duotone'] = null;
    if (duo) {
      const [dr, dg, db] = hexRgb(duo.dark);
      const [lr, lg, lb] = hexRgb(duo.light);
      const t = (x: number, y: number) => `${(x / 255).toFixed(4)} ${(y / 255).toFixed(4)}`;
      duotone = { r: t(dr, lr), g: t(dg, lg), b: t(db, lb), amount: Math.min(1, duo.amount) };
    }
    register(id, {
      matrix: hasMatrix(a) ? feColorMatrixValues(a) : null,
      curve: curve ? curve.map((v) => v.toFixed(4)).join(' ') : null,
      duotone,
      vignette: vig,
    });
  }
  return `url(#${id})`;
}

/** Vignette as an image for `feImage`: white centre to grey edges (multiplied in). */
export function vignetteImage(v: number): string {
  // Same falloff as the collage renderer: clear inside 25 % of the short side, black at
  // 70 %·v in the corners. In a unit square the corners are at r = √2/2.
  const g = Math.round(255 * (1 - 0.7 * v));
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" preserveAspectRatio="none">` +
    `<radialGradient id="g" cx="50" cy="50" r="70.71" fx="50" fy="50" gradientUnits="userSpaceOnUse">` +
    `<stop offset="0.354" stop-color="#fff"/><stop offset="1" stop-color="rgb(${g},${g},${g})"/></radialGradient>` +
    `<rect width="100" height="100" fill="url(#g)"/></svg>`;
  return 'data:image/svg+xml,' + encodeURIComponent(svg);
}

/* ------------------------------------------------------------ geometry */

/** Width/height of the media as shown: crop, then quarter turns. */
export function shownAspect(w: number, h: number, crop: Crop | null | undefined, rotate = 0): number {
  const cw = w * (crop?.w ?? 1);
  const ch = h * (crop?.h ?? 1);
  const r = cw / Math.max(1, ch);
  return rotate === 90 || rotate === 270 ? 1 / r : r;
}

/** Scale that keeps a rectangle of aspect `r` covered after a small rotation. */
export function straightenScale(deg: number, r: number): number {
  const t = (Math.abs(deg) * Math.PI) / 180;
  return Math.cos(t) + Math.sin(t) * Math.max(r, 1 / r);
}

/** CSS transform of the media element: quarter turns, straighten, flips. */
export function lookTransform(l: Look | null | undefined, aspect: number): string {
  if (!l) return '';
  const parts: string[] = [];
  if (l.rotate) parts.push(`rotate(${l.rotate}deg)`);
  if (l.straighten) parts.push(`rotate(${l.straighten}deg) scale(${straightenScale(l.straighten, aspect).toFixed(4)})`);
  if (l.flipX || l.flipY) parts.push(`scale(${l.flipX ? -1 : 1},${l.flipY ? -1 : 1})`);
  return parts.join(' ');
}

/** Inline style (filter + flips/straighten, no quarter turns) for a plain element. */
export function lookStyle(l: Look | null | undefined, aspect = 1): string {
  if (!l) return '';
  const parts: string[] = [];
  const f = lookFilter(l.adjust);
  if (f) parts.push(`filter:${f}`);
  const t = lookTransform({ ...l, rotate: 0 }, aspect);
  if (t) parts.push(`transform:${t}`);
  return parts.join(';');
}

const OVB = typeof CSS !== 'undefined' && CSS.supports('object-view-box', 'inset(0% 0% 0% 0%)');

/**
 * Style for a plain <img>/<video> showing a story's media reference: the
 * look plus the crop (object-view-box, where the browser supports it).
 */
export function mediaStyle(m: Look & { crop?: Crop }): string | undefined {
  const parts: string[] = [];
  const c = m.crop;
  if (OVB && c && c.w > 0 && c.h > 0 && (c.w < 0.999 || c.h < 0.999))
    parts.push(`object-view-box:inset(${c.y * 100}% ${(1 - c.x - c.w) * 100}% ${(1 - c.y - c.h) * 100}% ${c.x * 100}%)`);
  const l = lookStyle(m);
  if (l) parts.push(l);
  if (m.rotate) parts.push(`rotate:${m.rotate}deg`);
  return parts.join(';') || undefined;
}

/* ------------------------------------------------------------ masks */

const maskUrls = new Map<string, string>();

/** A mask shape as a PNG data URL at the given aspect (for CSS mask-image). */
export function maskImage(shape: MaskShape, aspect: number): string {
  const key = shape + '|' + aspect.toFixed(3);
  const hit = maskUrls.get(key);
  if (hit) return hit;
  const W = aspect >= 1 ? 512 : Math.round(512 * aspect);
  const H = aspect >= 1 ? Math.round(512 / aspect) : 512;
  const c = document.createElement('canvas');
  c.width = Math.max(1, W);
  c.height = Math.max(1, H);
  const g = c.getContext('2d')!;
  g.fillStyle = '#000';
  // lazy import keeps shapes.ts out of modules that never mask
  g.fill(maskPathFor(shape, c.width, c.height));
  const url = c.toDataURL('image/png');
  maskUrls.set(key, url);
  return url;
}

let shapes: typeof import('../render/shapes') | null = null;
void import('../render/shapes').then((m) => (shapes = m));
function maskPathFor(shape: MaskShape, w: number, h: number): Path2D {
  if (!shapes) {
    const p = new Path2D();
    p.rect(0, 0, w, h);
    return p;
  }
  return shapes.maskPath({ shape, radius: 0.12 }, w, h);
}

/* ------------------------------------------------------------ pixel path */

const pixelCache = new Map<string, Promise<string>>();
const PIXEL_CACHE_MAX = 80;

async function loadImage(src: string): Promise<HTMLImageElement> {
  const img = new Image();
  img.decoding = 'async';
  img.src = src;
  await img.decode();
  return img;
}

/**
 * The image at `src` with the look's pixel stages applied (whole image, no
 * crop), as a blob URL. Cached; the oldest entries are released.
 */
export function lookedUrl(src: string, l: Look): Promise<string> {
  const a = fullAdjust(l.adjust);
  const key = src + '|' + adjustKey(a, l.hideAreas);
  const hit = pixelCache.get(key);
  if (hit) {
    pixelCache.delete(key);
    pixelCache.set(key, hit);
    return hit;
  }
  const p = (async () => {
    const img = await loadImage(src);
    const c = document.createElement('canvas');
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const g = c.getContext('2d', { willReadFrequently: true })!;
    g.drawImage(img, 0, 0);
    const px = g.getImageData(0, 0, c.width, c.height);
    // vignette stays an SVG/overlay stage; everything else happens here
    applyLookPixels(px.data, c.width, c.height, { ...a, vignette: 0 }, l.hideAreas);
    g.putImageData(px, 0, 0);
    const blob = await new Promise<Blob>((res, rej) => c.toBlob((b) => (b ? res(b) : rej(new Error('encode failed'))), 'image/jpeg', 0.92));
    return URL.createObjectURL(blob);
  })();
  pixelCache.set(key, p);
  p.catch(() => pixelCache.delete(key));
  while (pixelCache.size > PIXEL_CACHE_MAX) {
    const [k, old] = pixelCache.entries().next().value!;
    pixelCache.delete(k);
    void old.then((u) => URL.revokeObjectURL(u)).catch(() => undefined);
  }
  return p;
}

/** Hidden areas (source fractions) in the coordinates of a crop. */
export function areasInCrop(areas: HiddenArea[] | undefined, cr: Crop): HiddenArea[] {
  return (areas ?? []).map((h) => ({ ...h, x: (h.x - cr.x) / cr.w, y: (h.y - cr.y) / cr.h, w: h.w / cr.w, h: h.h / cr.h }));
}

/** Draw `img` with crop, look, hidden areas, rotation and flips into a canvas (exports, edited copies). */
export function bakeLook(
  img: CanvasImageSource & { naturalWidth?: number; naturalHeight?: number; videoWidth?: number; videoHeight?: number },
  crop: Crop | undefined,
  look: Look | null | undefined,
  maxW: number,
): HTMLCanvasElement {
  const iw = img.naturalWidth || img.videoWidth || 1;
  const ih = img.naturalHeight || img.videoHeight || 1;
  const cr = crop && crop.w > 0 && crop.h > 0 ? crop : { x: 0, y: 0, w: 1, h: 1 };
  const sw = iw * cr.w;
  const sh = ih * cr.h;
  const k = Math.min(1, maxW / Math.max(sw, sh));
  const dw = Math.max(1, Math.round(sw * k));
  const dh = Math.max(1, Math.round(sh * k));
  // 1. the cropped photo with its pixel look (source orientation)
  const a0 = document.createElement('canvas');
  a0.width = dw;
  a0.height = dh;
  const ga = a0.getContext('2d', { willReadFrequently: true })!;
  ga.drawImage(img, iw * cr.x, ih * cr.y, sw, sh, 0, 0, dw, dh);
  const a = look?.adjust ? fullAdjust(look.adjust) : null;
  if ((a && changesPixels(a)) || look?.hideAreas?.length) {
    const px = ga.getImageData(0, 0, dw, dh);
    applyLookPixels(px.data, dw, dh, { ...(a ?? neutralAdjust()), vignette: 0 }, areasInCrop(look?.hideAreas, cr));
    ga.putImageData(px, 0, 0);
  }
  // 2. quarter turns, straighten, flips
  const quarter = look?.rotate === 90 || look?.rotate === 270;
  const c = document.createElement('canvas');
  c.width = quarter ? dh : dw;
  c.height = quarter ? dw : dh;
  const g = c.getContext('2d')!;
  g.save();
  g.translate(c.width / 2, c.height / 2);
  if (look?.rotate) g.rotate((look.rotate * Math.PI) / 180);
  if (look?.straighten) {
    g.rotate((look.straighten * Math.PI) / 180);
    const s = straightenScale(look.straighten, dw / dh);
    g.scale(s, s);
  }
  g.scale(look?.flipX ? -1 : 1, look?.flipY ? -1 : 1);
  g.drawImage(a0, -dw / 2, -dh / 2);
  g.restore();
  // 3. vignette on top, like the collage renderer
  if (a && a.vignette > 0) {
    const grd = g.createRadialGradient(c.width / 2, c.height / 2, Math.min(c.width, c.height) * 0.25, c.width / 2, c.height / 2, Math.hypot(c.width, c.height) / 2);
    grd.addColorStop(0, 'rgba(0,0,0,0)');
    grd.addColorStop(1, `rgba(0,0,0,${0.7 * a.vignette})`);
    g.fillStyle = grd;
    g.fillRect(0, 0, c.width, c.height);
  }
  return c;
}
