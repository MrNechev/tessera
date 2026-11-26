// Look of diagram items: fills, text colour on fills, arrowhead markers, and the
// copy / paste style clipboard.
import { ink, paper, PALETTE } from '../canvas';
import type { ArrowItem, CanvasItem, Dash, Head, IconItem, NoteItem, PathItem, Route, ShapeItem } from '../types';
import type { TextLook } from '../../../text/styles';

/** fill of a shape: its fillColor, the old tint (`fill`), or none */
export function fillOf(it: Pick<ShapeItem, 'fill' | 'fillColor' | 'color'>): string {
  if (it.fillColor) return it.fillColor === 'none' ? 'none' : paper(it.fillColor);
  if (it.fill) return `color-mix(in srgb, ${ink(it.color)} 22%, transparent)`;
  return 'none';
}

/** relative luminance of '#rrggbb' (0 dark … 1 light) */
export function luminance(hex: string): number {
  const m = /^#([0-9a-f]{6})/i.exec(hex);
  if (!m) return 0.5;
  const n = parseInt(m[1], 16);
  const c = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}

/** text colour inside a shape: dark on light fills, white on dark ones, else the theme's */
export function shapeTextColor(it: Pick<ShapeItem, 'fill' | 'fillColor' | 'color'>): string {
  const f = it.fillColor;
  if (!f || f === 'none') return 'var(--text)';
  const hex = f.startsWith('#') ? f : PALETTE.find((p) => p.key === f)?.paper;
  if (!hex) return 'var(--text)';
  return luminance(hex) > 0.35 ? '#1d1b22' : '#ffffff';
}

// ---------- arrowheads ----------

/** marker drawing for a head in a 10 × 10 box; `fill` = filled shape, else stroked */
export const HEAD_SHAPES: Record<Exclude<Head, 'none'>, { d: string; fill: boolean; refX: number }> = {
  arrow: { d: 'M0,0L10,5L0,10z', fill: true, refX: 8.5 },
  open: { d: 'M1.5,1L9,5L1.5,9', fill: false, refX: 8.5 },
  dot: { d: 'M5,1.5a3.5,3.5 0 1 0 0.01,0z', fill: true, refX: 5 },
  diamond: { d: 'M0.5,5L5,1.5L9.5,5L5,8.5z', fill: true, refX: 9 },
  bar: { d: 'M5,0L5,10', fill: false, refX: 5 },
};

/** marker size in stroke widths: heads grow slower than the line */
export function markerScale(w: number) {
  const px = w <= 3 ? 5 * w : 15 + (w - 3) * 2.5;
  return px / w;
}

export const markerId = (head: Head, color: string, w: number) => `mk-${head}-${color.replace(/[^a-z0-9]/gi, '')}-${String(w).replace('.', '_')}`;

// ---------- copy / paste style ----------

export interface StyleClip {
  color?: string;
  fillColor?: string;
  strokeWidth?: number;
  dash?: Dash;
  opacity?: number;
  radius?: number;
  look?: TextLook;
  size?: number;
  route?: Route;
  start?: Head;
  end?: Head;
  /** note background */
  paper?: string;
}

export function copyStyle(it: CanvasItem): StyleClip | null {
  const o = (it as { opacity?: number }).opacity;
  switch (it.type) {
    case 'rect':
    case 'ellipse':
      return { color: it.color, fillColor: it.fillColor ?? (it.fill ? undefined : 'none'), strokeWidth: it.strokeWidth, dash: it.dash, opacity: o, radius: it.radius, look: it.look, size: it.size };
    case 'arrow':
      return { color: it.color, strokeWidth: it.width, dash: it.dash ?? (it.dashed ? 'dashed' : undefined), opacity: o, route: it.route, start: it.start, end: it.end };
    case 'note':
      return { paper: it.color, look: it.look, size: it.size, opacity: o };
    case 'path':
      return { color: it.color, opacity: o };
    case 'icon':
      return { color: it.color, opacity: o };
    case 'sticker':
      return { opacity: o };
    default:
      return null;
  }
}

function put<T extends object>(o: T, k: keyof T, v: unknown) {
  if (v === undefined) delete o[k];
  else (o as Record<keyof T, unknown>)[k] = structuredClone(v);
}

/** paste a copied style onto an item (only what makes sense for its kind) */
export function pasteStyle(it: CanvasItem, s: StyleClip) {
  put(it as { opacity?: number }, 'opacity', s.opacity);
  switch (it.type) {
    case 'rect':
    case 'ellipse': {
      const sh = it as ShapeItem;
      if (s.color) sh.color = s.color;
      if (s.fillColor !== undefined || s.paper) {
        put(sh, 'fillColor', s.fillColor ?? s.paper);
        delete sh.fill;
      }
      put(sh, 'strokeWidth', s.strokeWidth);
      put(sh, 'dash', s.dash);
      if (sh.type === 'rect' && !sh.shape) put(sh, 'radius', s.radius);
      if (s.look || s.size) {
        put(sh, 'look', s.look);
        if (s.size) sh.size = s.size;
      }
      break;
    }
    case 'arrow': {
      const a = it as ArrowItem;
      if (s.color) a.color = s.color;
      put(a, 'width', s.strokeWidth);
      put(a, 'dash', s.dash);
      delete a.dashed;
      if (s.route) a.route = s.route;
      if (s.start || s.end) {
        put(a, 'start', s.start);
        put(a, 'end', s.end);
        delete a.head;
      }
      break;
    }
    case 'note': {
      const n = it as NoteItem;
      if (s.paper) n.color = s.paper;
      else if (s.fillColor && s.fillColor !== 'none') n.color = s.fillColor;
      if (s.look) n.look = structuredClone(s.look);
      if (s.size) n.size = s.size;
      break;
    }
    case 'path':
      if (s.color) (it as PathItem).color = s.color;
      break;
    case 'icon':
      if (s.color) (it as IconItem).color = s.color;
      break;
  }
}
