// Diagram shapes of the canvas: the catalogue for the shape picker, their outlines as SVG
// path data (the collage engine's outlines where it has them), the area for their text
// and the points where connectors attach.
import { drawShape } from '../../../render/shapes';
import { SvgPath } from '../../../render/pathsink';
import type { DiagramShape, Rect, ShapeItem } from '../types';

/** what the shape tool draws: a plain/rounded rectangle, an ellipse or a diagram shape */
export type ShapeId = 'rect' | 'round' | 'ellipse' | DiagramShape;
/** the outline actually drawn */
export type OutlineKind = 'rect' | 'ellipse' | DiagramShape;

export const SHAPES: { id: ShapeId; label: string; w: number; h: number }[] = [
  { id: 'rect', label: 'Rectangle', w: 240, h: 160 },
  { id: 'round', label: 'Rounded rectangle', w: 240, h: 160 },
  { id: 'ellipse', label: 'Ellipse', w: 200, h: 200 },
  { id: 'diamond', label: 'Diamond (decision)', w: 200, h: 140 },
  { id: 'triangle', label: 'Triangle', w: 180, h: 160 },
  { id: 'parallelogram', label: 'Parallelogram (input / output)', w: 240, h: 130 },
  { id: 'hexagon', label: 'Hexagon (preparation)', w: 220, h: 130 },
  { id: 'cylinder', label: 'Cylinder (database)', w: 160, h: 180 },
  { id: 'document', label: 'Document', w: 220, h: 150 },
  { id: 'cloud', label: 'Cloud', w: 260, h: 170 },
  { id: 'callout', label: 'Speech bubble', w: 240, h: 170 },
  { id: 'star', label: 'Star', w: 180, h: 170 },
];
export const ROUND_RADIUS = 24;
export const DEFAULT_RADIUS = 6;

export function shapeInfo(id: ShapeId) {
  return SHAPES.find((s) => s.id === id) ?? SHAPES[0];
}

export function outlineKind(it: Pick<ShapeItem, 'type' | 'shape'>): OutlineKind {
  return it.shape ?? it.type;
}

/** the picker entry of an item (rounded rectangles have a bigger radius) */
export function shapeIdOf(it: Pick<ShapeItem, 'type' | 'shape' | 'radius'>): ShapeId {
  if (it.shape) return it.shape;
  if (it.type === 'rect' && (it.radius ?? DEFAULT_RADIUS) >= 16) return 'round';
  return it.type;
}

/** fields of a new shape item of picker entry `id` */
export function shapeFields(id: ShapeId): Pick<ShapeItem, 'type'> & Partial<ShapeItem> {
  if (id === 'rect') return { type: 'rect' };
  if (id === 'round') return { type: 'rect', radius: ROUND_RADIUS };
  if (id === 'ellipse') return { type: 'ellipse' };
  return { type: 'rect', shape: id };
}

/** Build the outline of kind `k` filling (x, y, w, h). `r` = corner radius (rect) in px. */
function build(p: SvgPath, k: OutlineKind, w: number, h: number, r: number, x = 0, y = 0) {
  switch (k) {
    case 'rect':
      // shapes.ts takes the radius as a fraction of the shorter side
      drawShape(p, 'rect', w, h, Math.min(w, h) > 0 ? r / Math.min(w, h) : 0, x, y);
      break;
    case 'ellipse':
      drawShape(p, 'circle', w, h, 0, x, y);
      break;
    case 'diamond':
    case 'triangle':
    case 'hexagon':
    case 'cloud':
    case 'star':
      drawShape(p, k, w, h, 0, x, y);
      break;
    case 'parallelogram': {
      const s = Math.min(w * 0.25, h * 0.6);
      p.moveTo(x + s, y);
      p.lineTo(x + w, y);
      p.lineTo(x + w - s, y + h);
      p.lineTo(x, y + h);
      p.closePath();
      break;
    }
    case 'cylinder': {
      const ry = Math.min(h * 0.14, w * 0.25);
      p.moveTo(x, y + ry);
      p.ellipse(x + w / 2, y + ry, w / 2, ry, 0, Math.PI, 0);
      p.lineTo(x + w, y + h - ry);
      p.ellipse(x + w / 2, y + h - ry, w / 2, ry, 0, 0, Math.PI);
      p.closePath();
      break;
    }
    case 'document': {
      const a = Math.min(h * 0.12, 30);
      p.moveTo(x, y);
      p.lineTo(x + w, y);
      p.lineTo(x + w, y + h - a);
      p.bezierCurveTo(x + w * 0.75, y + h - 3 * a, x + w * 0.5, y + h + a, x, y + h - a * 0.6);
      p.closePath();
      break;
    }
    case 'callout': {
      const bh = h * 0.78;
      const rr = Math.min(16, w / 4, bh / 4);
      p.moveTo(x + rr, y);
      p.lineTo(x + w - rr, y);
      p.arcTo(x + w, y, x + w, y + rr, rr);
      p.lineTo(x + w, y + bh - rr);
      p.arcTo(x + w, y + bh, x + w - rr, y + bh, rr);
      p.lineTo(x + w * 0.42, y + bh);
      p.lineTo(x + w * 0.2, y + h);
      p.lineTo(x + w * 0.24, y + bh);
      p.lineTo(x + rr, y + bh);
      p.arcTo(x, y + bh, x, y + bh - rr, rr);
      p.lineTo(x, y + rr);
      p.arcTo(x, y, x + rr, y, rr);
      p.closePath();
      break;
    }
  }
}

const cache = new Map<string, SvgPath>();
/** outline in the local box (0, 0, w, h); memoised (shapes rarely change size) */
export function outline(k: OutlineKind, w: number, h: number, r = DEFAULT_RADIUS): SvgPath {
  const key = `${k}|${Math.round(w * 10)}|${Math.round(h * 10)}|${r}`;
  let p = cache.get(key);
  if (!p) {
    p = new SvgPath();
    build(p, k, Math.max(1, w), Math.max(1, h), r);
    if (cache.size > 2000) cache.clear();
    cache.set(key, p);
  }
  return p;
}

/** SVG path data of the outline, inset by half the stroke so the line stays inside the box */
export function outlinePath(k: OutlineKind, w: number, h: number, stroke: number, r = DEFAULT_RADIUS): string {
  const p = new SvgPath();
  const i = stroke / 2;
  build(p, k, Math.max(1, w - stroke), Math.max(1, h - stroke), Math.max(0, r - i), i, i);
  return p.toString();
}

/** extra lines drawn over the outline (the lid of a cylinder) */
export function outlineExtra(k: OutlineKind, w: number, h: number, stroke: number): string {
  if (k !== 'cylinder') return '';
  const i = stroke / 2;
  const ww = w - stroke,
    hh = h - stroke;
  const ry = Math.min(hh * 0.14, ww * 0.25);
  const p = new SvgPath();
  p.moveTo(i, i + ry);
  p.ellipse(i + ww / 2, i + ry, ww / 2, ry, 0, Math.PI, 0, true);
  return p.toString();
}

/** where the text of a shape goes (local box), so it stays inside the outline */
export function textBox(k: OutlineKind, w: number, h: number): Rect {
  const box = (fx: number, fy: number, fw: number, fh: number): Rect => ({ x: w * fx, y: h * fy, w: w * fw, h: h * fh });
  switch (k) {
    case 'ellipse':
      return box(0.15, 0.15, 0.7, 0.7);
    case 'diamond':
      return box(0.22, 0.22, 0.56, 0.56);
    case 'triangle':
      return box(0.25, 0.45, 0.5, 0.5);
    case 'parallelogram':
      return box(0.2, 0.08, 0.6, 0.84);
    case 'hexagon':
      return box(0.2, 0.08, 0.6, 0.84);
    case 'cylinder':
      return box(0.08, 0.3, 0.84, 0.6);
    case 'document':
      return box(0.06, 0.06, 0.88, 0.72);
    case 'cloud':
      return box(0.18, 0.28, 0.64, 0.56);
    case 'callout':
      return box(0.06, 0.04, 0.88, 0.7);
    case 'star':
      return box(0.3, 0.36, 0.4, 0.38);
    default:
      return box(0.05, 0.05, 0.9, 0.9);
  }
}

/** point where the ray from the centre of `r` towards (dx, dy) leaves the outline polygon */
export function rayExit(poly: [number, number][], cx: number, cy: number, dx: number, dy: number): { x: number; y: number } | null {
  let best = Infinity;
  for (let i = 0; i < poly.length; i++) {
    const [ax, ay] = poly[i];
    const [bx, by] = poly[(i + 1) % poly.length];
    const ex = bx - ax,
      ey = by - ay;
    const den = dx * ey - dy * ex;
    if (Math.abs(den) < 1e-12) continue;
    const t = ((ax - cx) * ey - (ay - cy) * ex) / den;
    const u = ((ax - cx) * dy - (ay - cy) * dx) / den;
    if (t > 1e-9 && u >= -1e-9 && u <= 1 + 1e-9 && t < best) best = t;
  }
  return best < Infinity ? { x: cx + dx * best, y: cy + dy * best } : null;
}

/** exit point from the outline of a shape of kind `k` in rect `r`, along (dx, dy) from its centre */
export function outlineExit(k: OutlineKind, r: Rect, dx: number, dy: number): { x: number; y: number } {
  const cx = r.x + r.w / 2,
    cy = r.y + r.h / 2;
  if (k === 'rect' || k === 'ellipse') {
    const hw = r.w / 2,
      hh = r.h / 2;
    const k2 = k === 'ellipse' ? 1 / Math.sqrt((dx * dx) / (hw * hw) + (dy * dy) / (hh * hh)) : Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity);
    return { x: cx + dx * k2, y: cy + dy * k2 };
  }
  const poly = outline(k, r.w, r.h).outline.map(([x, y]) => [r.x + x, r.y + y] as [number, number]);
  return rayExit(poly, cx, cy, dx, dy) ?? { x: cx, y: cy };
}
