/** Small geometry helpers for Paint's tools and viewport (pure). */
import type { Rect } from './mask';

export interface Pt {
  x: number;
  y: number;
}

/** Every pixel on the line from a to b (Bresenham), for the hard pencil. */
export function linePixels(x0: number, y0: number, x1: number, y1: number): Pt[] {
  x0 = Math.floor(x0);
  y0 = Math.floor(y0);
  x1 = Math.floor(x1);
  y1 = Math.floor(y1);
  const out: Pt[] = [];
  const dx = Math.abs(x1 - x0);
  const dy = -Math.abs(y1 - y0);
  const sx = x0 < x1 ? 1 : -1;
  const sy = y0 < y1 ? 1 : -1;
  let err = dx + dy;
  for (;;) {
    out.push({ x: x0, y: y0 });
    if (x0 === x1 && y0 === y1) break;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      x0 += sx;
    }
    if (e2 <= dx) {
      err += dx;
      y0 += sy;
    }
  }
  return out;
}

/** Shift: lines snap to multiples of 45°. */
export function snapAngle(a: Pt, b: Pt, step = Math.PI / 4): Pt {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy);
  const ang = Math.round(Math.atan2(dy, dx) / step) * step;
  return { x: a.x + Math.cos(ang) * len, y: a.y + Math.sin(ang) * len };
}

/** Shift: boxes become squares (circles), keeping the drag direction. */
export function squareBox(a: Pt, b: Pt): Pt {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const s = Math.max(Math.abs(dx), Math.abs(dy));
  return { x: a.x + (dx < 0 ? -s : s), y: a.y + (dy < 0 ? -s : s) };
}

/** The normalised rectangle between two points. */
export function boxOf(a: Pt, b: Pt): Rect {
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), w: Math.abs(b.x - a.x), h: Math.abs(b.y - a.y) };
}

/** Outline of an arrow from a to b (shaft width `w`), as a closed polygon. */
export function arrowPolygon(a: Pt, b: Pt, w: number): Pt[] {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len = Math.hypot(dx, dy);
  if (len < 1e-6) return [];
  const ux = dx / len;
  const uy = dy / len;
  const nx = -uy;
  const ny = ux;
  const head = Math.min(len * 0.6, Math.max(w * 4, 12));
  const hw = Math.max(w * 2.2, head * 0.45);
  const sw = w / 2;
  const bx = b.x - ux * head;
  const by = b.y - uy * head;
  return [
    { x: a.x + nx * sw, y: a.y + ny * sw },
    { x: bx + nx * sw, y: by + ny * sw },
    { x: bx + nx * hw, y: by + ny * hw },
    { x: b.x, y: b.y },
    { x: bx - nx * hw, y: by - ny * hw },
    { x: bx - nx * sw, y: by - ny * sw },
    { x: a.x - nx * sw, y: a.y - ny * sw },
  ];
}

export interface View {
  /** Screen pixels per image pixel. */
  zoom: number;
  /** Screen position (CSS px, relative to the viewport element) of image pixel (0, 0). */
  x: number;
  y: number;
}

export const MIN_ZOOM = 0.02;
export const MAX_ZOOM = 64;

/** Zoom to `zoom`, keeping the image point under screen point (sx, sy) in place. */
export function zoomAt(v: View, zoom: number, sx: number, sy: number): View {
  const z = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom));
  const ix = (sx - v.x) / v.zoom;
  const iy = (sy - v.y) / v.zoom;
  return { zoom: z, x: sx - ix * z, y: sy - iy * z };
}

/** The view that fits a w × h image into a vw × vh viewport (with a margin), never above 100 % unless `upscale`. */
export function fitView(w: number, h: number, vw: number, vh: number, margin = 24, upscale = false): View {
  const z0 = Math.min((vw - 2 * margin) / w, (vh - 2 * margin) / h);
  const zoom = Math.max(MIN_ZOOM, upscale ? z0 : Math.min(1, z0));
  return { zoom, x: Math.round((vw - w * zoom) / 2), y: Math.round((vh - h * zoom) / 2) };
}

/** Image coordinates of a screen point. */
export function toImage(v: View, sx: number, sy: number): Pt {
  return { x: (sx - v.x) / v.zoom, y: (sy - v.y) / v.zoom };
}

/** Next zoom step in or out (the usual 12.5, 25, 33, 50, 66, 100, 150, 200 … ladder). */
export function zoomStep(zoom: number, dir: 1 | -1): number {
  const steps = [0.02, 0.05, 0.0833, 0.125, 0.1667, 0.25, 0.3333, 0.5, 0.6667, 1, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64];
  if (dir > 0) return steps.find((s) => s > zoom * 1.001) ?? MAX_ZOOM;
  return [...steps].reverse().find((s) => s < zoom / 1.001) ?? MIN_ZOOM;
}

/** Canvas size after resizing by `anchor` (0..2 for x and y: left/centre/right, top/middle/bottom). */
export function anchorOffset(ow: number, oh: number, nw: number, nh: number, ax: number, ay: number): Pt {
  return { x: Math.round(((nw - ow) * ax) / 2), y: Math.round(((nh - oh) * ay) / 2) };
}

/** Fit (w, h) inside `max` pixels in total (area), keeping the aspect ratio. */
export function capArea(w: number, h: number, max: number): { w: number; h: number; scaled: boolean } {
  if (w * h <= max) return { w, h, scaled: false };
  const s = Math.sqrt(max / (w * h));
  return { w: Math.max(1, Math.floor(w * s)), h: Math.max(1, Math.floor(h * s)), scaled: true };
}
