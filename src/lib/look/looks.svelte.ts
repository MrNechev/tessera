/**
 * Non-destructive photo looks for documents and the canvas: the collage
 * editor's adjustments and filter presets, plus flips. Stored on each media
 * reference; the original file is never touched.
 *
 * On screen a look is an SVG filter (`feColorMatrix` = the exact colour math
 * of the collage renderer, plus an optional vignette), registered once per
 * distinct look and referenced by CSS `filter: url(#…)`. Works on images,
 * GIF-videos and videos alike. `bakeLook` draws the same look into a canvas
 * for exports that need pixels (PowerPoint).
 */
import { neutralAdjust } from '../model/factory';
import type { PhotoAdjust } from '../model/types';
import { applyAdjustPixels, hasPixelAdjust } from '../render/adjust';
import { feColorMatrixValues } from '../render/adjustMatrix';

export interface Look {
  adjust?: PhotoAdjust;
  /** The filter preset the adjustment started from (only for the UI). */
  filter?: string;
  flipX?: boolean;
  flipY?: boolean;
}

/** hasPixelAdjust without the type guard (its false branch would narrow to never). */
function changesPixels(a: PhotoAdjust | null | undefined): boolean {
  return hasPixelAdjust(a);
}

export function hasLook(l: Look | null | undefined): boolean {
  return !!l && (changesPixels(l.adjust) || (l.adjust?.vignette ?? 0) > 0 || !!l.flipX || !!l.flipY);
}

export function fullAdjust(a: Partial<PhotoAdjust> | undefined): PhotoAdjust {
  return { ...neutralAdjust(), ...(a ?? {}) };
}

type Def = { matrix: string | null; vignette: number };
const defs = new Map<string, Def>();
let flushing = false;
/**
 * Every look used so far (id → matrix values, vignette strength), for
 * LookFilters. Filters are registered while styles are computed (inside
 * deriveds), so the reactive list is updated in a microtask.
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

function keyOf(a: PhotoAdjust): string {
  return [a.brightness, a.contrast, a.saturation, a.warmth, a.fade, a.vignette, a.grayscale, a.sepia].map((v) => Math.round(v * 1000)).join('_').replace(/-/g, 'm');
}

/** CSS `filter` value for an adjustment ('' when it changes nothing). */
export function lookFilter(a: PhotoAdjust | undefined): string {
  if (!a) return '';
  const pix = changesPixels(a);
  if (!pix && !(a.vignette > 0)) return '';
  const id = 'tlook-' + keyOf(a);
  if (!defs.has(id)) register(id, { matrix: pix ? feColorMatrixValues(a) : null, vignette: Math.max(0, Math.min(1, a.vignette)) });
  return `url(#${id})`;
}

/** Inline style (filter + flip) for the element that shows the media. */
export function lookStyle(l: Look | null | undefined): string {
  if (!l) return '';
  const parts: string[] = [];
  const f = lookFilter(l.adjust);
  if (f) parts.push(`filter:${f}`);
  if (l.flipX || l.flipY) parts.push(`transform:scale(${l.flipX ? -1 : 1},${l.flipY ? -1 : 1})`);
  return parts.join(';');
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

/** Draw `img` with crop, look and flips into a canvas (for exports). */
export function bakeLook(
  img: CanvasImageSource & { naturalWidth?: number; naturalHeight?: number; videoWidth?: number; videoHeight?: number },
  crop: { x: number; y: number; w: number; h: number } | undefined,
  look: Look | null | undefined,
  maxW: number,
): HTMLCanvasElement {
  const iw = img.naturalWidth || img.videoWidth || 1;
  const ih = img.naturalHeight || img.videoHeight || 1;
  const cr = crop && crop.w > 0 && crop.h > 0 ? crop : { x: 0, y: 0, w: 1, h: 1 };
  const sw = iw * cr.w;
  const sh = ih * cr.h;
  const k = Math.min(1, maxW / sw);
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(sw * k));
  c.height = Math.max(1, Math.round(sh * k));
  const g = c.getContext('2d', { willReadFrequently: true })!;
  g.save();
  if (look?.flipX || look?.flipY) {
    g.translate(look.flipX ? c.width : 0, look.flipY ? c.height : 0);
    g.scale(look.flipX ? -1 : 1, look.flipY ? -1 : 1);
  }
  g.drawImage(img, iw * cr.x, ih * cr.y, sw, sh, 0, 0, c.width, c.height);
  g.restore();
  const a = look?.adjust;
  if (a && hasPixelAdjust(a)) {
    const px = g.getImageData(0, 0, c.width, c.height);
    applyAdjustPixels(px.data, a);
    g.putImageData(px, 0, 0);
  }
  if (a && a.vignette > 0) {
    const grd = g.createRadialGradient(c.width / 2, c.height / 2, Math.min(c.width, c.height) * 0.25, c.width / 2, c.height / 2, Math.hypot(c.width, c.height) / 2);
    grd.addColorStop(0, 'rgba(0,0,0,0)');
    grd.addColorStop(1, `rgba(0,0,0,${0.7 * a.vignette})`);
    g.fillStyle = grd;
    g.fillRect(0, 0, c.width, c.height);
  }
  return c;
}

const OVB = typeof CSS !== 'undefined' && CSS.supports('object-view-box', 'inset(0% 0% 0% 0%)');

/**
 * Style for a plain <img>/<video> showing a document's media reference: the
 * look plus the crop (object-view-box, where the browser supports it).
 */
export function mediaStyle(m: Look & { crop?: { x: number; y: number; w: number; h: number } }): string | undefined {
  const parts: string[] = [];
  const c = m.crop;
  if (OVB && c && c.w > 0 && c.h > 0 && (c.w < 0.999 || c.h < 0.999))
    parts.push(`object-view-box:inset(${c.y * 100}% ${(1 - c.x - c.w) * 100}% ${(1 - c.y - c.h) * 100}% ${c.x * 100}%)`);
  const l = lookStyle(m);
  if (l) parts.push(l);
  return parts.join(';') || undefined;
}
