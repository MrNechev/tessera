import type { CollageElement, Rect } from '../model/types';

export interface Pt {
  x: number;
  y: number;
}

export function rotatePoint(p: Pt, c: Pt, deg: number): Pt {
  const a = (deg * Math.PI) / 180;
  const cos = Math.cos(a);
  const sin = Math.sin(a);
  const dx = p.x - c.x;
  const dy = p.y - c.y;
  return { x: c.x + dx * cos - dy * sin, y: c.y + dx * sin + dy * cos };
}

export function center(el: Rect): Pt {
  return { x: el.x + el.w / 2, y: el.y + el.h / 2 };
}

/** Page-space point → element-local point (origin top-left of the unrotated box). */
export function toLocal(el: CollageElement, p: Pt): Pt {
  const c = center(el);
  const r = rotatePoint(p, c, -el.rotation);
  return { x: r.x - el.x, y: r.y - el.y };
}

export function hitElement(el: CollageElement, p: Pt, slop = 0): boolean {
  const l = toLocal(el, p);
  return l.x >= -slop && l.y >= -slop && l.x <= el.w + slop && l.y <= el.h + slop;
}

export function corners(el: CollageElement): Pt[] {
  const c = center(el);
  return [
    { x: el.x, y: el.y },
    { x: el.x + el.w, y: el.y },
    { x: el.x + el.w, y: el.y + el.h },
    { x: el.x, y: el.y + el.h },
  ].map((p) => rotatePoint(p, c, el.rotation));
}

export function boundsOf(els: CollageElement[]): Rect | null {
  if (!els.length) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const el of els) {
    for (const p of corners(el)) {
      x0 = Math.min(x0, p.x);
      y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x);
      y1 = Math.max(y1, p.y);
    }
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

export function rectsIntersect(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

export function normRect(a: Pt, b: Pt): Rect {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(a.x - b.x), h: Math.abs(a.y - b.y) };
}

/** Handle ids: corners (nw, ne, se, sw) and sides (n, e, s, w). */
export type Handle = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w';

export const HANDLES: { id: Handle; fx: number; fy: number }[] = [
  { id: 'nw', fx: 0, fy: 0 },
  { id: 'n', fx: 0.5, fy: 0 },
  { id: 'ne', fx: 1, fy: 0 },
  { id: 'e', fx: 1, fy: 0.5 },
  { id: 'se', fx: 1, fy: 1 },
  { id: 's', fx: 0.5, fy: 1 },
  { id: 'sw', fx: 0, fy: 1 },
  { id: 'w', fx: 0, fy: 0.5 },
];

/** Page-space position of a handle (fx, fy are fractions of the box). */
export function handlePoint(el: CollageElement, fx: number, fy: number): Pt {
  return rotatePoint({ x: el.x + el.w * fx, y: el.y + el.h * fy }, center(el), el.rotation);
}

/**
 * Resize `el` by dragging handle (fx, fy) to page point `p`, keeping the
 * opposite handle fixed in page space. Works for rotated elements.
 */
export function resizeBox(
  start: { x: number; y: number; w: number; h: number; rotation: number },
  fx: number,
  fy: number,
  p: Pt,
  keepAspect: boolean,
  minSize = 8,
): Rect {
  const c0 = { x: start.x + start.w / 2, y: start.y + start.h / 2 };
  const anchorLocal = { x: start.x + start.w * (1 - fx), y: start.y + start.h * (1 - fy) };
  const anchor = rotatePoint(anchorLocal, c0, start.rotation);
  // Pointer in the element's unrotated frame, relative to the anchor.
  const pl = rotatePoint(p, anchor, -start.rotation);
  let dx = pl.x - anchor.x;
  let dy = pl.y - anchor.y;
  const sx = fx === 0.5 ? 0 : fx === 1 ? 1 : -1;
  const sy = fy === 0.5 ? 0 : fy === 1 ? 1 : -1;
  let w = sx === 0 ? start.w : Math.max(minSize, dx * sx);
  let h = sy === 0 ? start.h : Math.max(minSize, dy * sy);
  if (keepAspect && sx !== 0 && sy !== 0) {
    const ratio = start.w / start.h;
    if (w / h > ratio) h = w / ratio;
    else w = h * ratio;
  }
  // Rebuild the box from the anchor.
  dx = sx === 0 ? 0 : sx * w;
  dy = sy === 0 ? 0 : sy * h;
  // Centre of the new box in the unrotated anchor frame.
  const cxLocal = sx === 0 ? anchor.x + 0 : anchor.x + dx / 2;
  const cyLocal = sy === 0 ? anchor.y + 0 : anchor.y + dy / 2;
  let cLocal = { x: cxLocal, y: cyLocal };
  // For side handles the anchor sits on the side's midpoint; shift the centre perpendicular only.
  if (sx === 0) cLocal = { x: anchor.x, y: cLocal.y };
  if (sy === 0) cLocal = { x: cLocal.x, y: anchor.y };
  const c = rotatePoint(cLocal, anchor, start.rotation);
  return { x: c.x - w / 2, y: c.y - h / 2, w, h };
}
