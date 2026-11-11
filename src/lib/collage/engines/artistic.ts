/** Creative engines: photos forming a shape, scattered polaroids, and concentric rings. */
import type { LayoutShape, Rect } from '../../model/types';
import { cssFont, ensureFont, isFontLoaded } from '../../assets/fonts';
import { polaroidHeightFor } from '../../render/frameGeometry';
import { ctx2d, makeCanvas } from '../../render/patterns';
import type { Engine, Placement } from './types';

/* ------------------------------------------------------------------ shapes */

type ShapeFn = (x: number, y: number) => number; // <= 0 means inside; y points up

const STAR_POINTS = (() => {
  const pts: [number, number][] = [];
  for (let i = 0; i < 10; i++) {
    const r = i % 2 === 0 ? 1 : 0.5;
    const a = Math.PI / 2 + (i * Math.PI) / 5;
    pts.push([Math.cos(a) * r, Math.sin(a) * r]);
  }
  return pts;
})();

// Classic parametric heart (x = 16 sin³t, y = 13 cos t − 5 cos 2t − 2 cos 3t − cos 4t):
// its notch between the lobes is deep enough to survive a coarse photo grid.
const HEART_POINTS = (() => {
  const pts: [number, number][] = [];
  for (let i = 0; i < 240; i++) {
    const t = (i / 240) * Math.PI * 2;
    const x = 16 * Math.pow(Math.sin(t), 3);
    const y = 13 * Math.cos(t) - 5 * Math.cos(2 * t) - 2 * Math.cos(3 * t) - Math.cos(4 * t);
    pts.push([x / 16, y / 16]);
  }
  return pts;
})();

const HEX_POINTS = (() => {
  const pts: [number, number][] = [];
  for (let i = 0; i < 6; i++) {
    const a = Math.PI / 2 + (i * Math.PI) / 3;
    pts.push([Math.cos(a), Math.sin(a)]);
  }
  return pts;
})();

function insidePolygon(pts: [number, number][], x: number, y: number): boolean {
  let inside = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i];
    const [xj, yj] = pts[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

export const SHAPE_FNS: Record<LayoutShape, ShapeFn> = {
  heart: (x, y) => (insidePolygon(HEART_POINTS, x, y) ? -1 : 1),
  circle: (x, y) => x * x + y * y - 1,
  diamond: (x, y) => Math.abs(x) + Math.abs(y) - 1,
  star: (x, y) => (insidePolygon(STAR_POINTS, x, y) ? -1 : 1),
  hexagon: (x, y) => (insidePolygon(HEX_POINTS, x, y) ? -1 : 1),
};

/** Bounding box of each shape in its own (y-up) coordinates, found by sampling. */
const SHAPE_BOUNDS: Record<LayoutShape, { x0: number; x1: number; y0: number; y1: number }> = (() => {
  const out = {} as Record<LayoutShape, { x0: number; x1: number; y0: number; y1: number }>;
  for (const key of Object.keys(SHAPE_FNS) as LayoutShape[]) {
    const f = SHAPE_FNS[key];
    let x0 = Infinity;
    let x1 = -Infinity;
    let y0 = Infinity;
    let y1 = -Infinity;
    const N = 240;
    for (let i = 0; i <= N; i++) {
      for (let j = 0; j <= N; j++) {
        const x = -1.6 + (3.2 * i) / N;
        const y = -1.6 + (3.2 * j) / N;
        if (f(x, y) <= 0) {
          x0 = Math.min(x0, x);
          x1 = Math.max(x1, x);
          y0 = Math.min(y0, y);
          y1 = Math.max(y1, y);
        }
      }
    }
    out[key] = { x0, x1, y0, y1 };
  }
  return out;
})();

/** A filled figure in a unit box: `inside(u, v)` with u, v in 0..1 (v grows downwards). */
export interface FigureMask {
  /** Width / height of the figure's bounding box. */
  aspect: number;
  inside(u: number, v: number): boolean;
  /**
   * Thin figures (letters) need cells to be mostly covered, not just hit at
   * their centre; `strict` samples several points per cell.
   */
  strict?: boolean;
}

function implicitMask(shape: LayoutShape): FigureMask {
  const b = SHAPE_BOUNDS[shape];
  const f = SHAPE_FNS[shape];
  return {
    aspect: (b.x1 - b.x0) / (b.y1 - b.y0),
    inside: (u, v) => f(b.x0 + u * (b.x1 - b.x0), b.y1 - v * (b.y1 - b.y0)) <= 0,
  };
}

/**
 * Tile a figure with the largest square cells that yield at least `target`
 * cells, then fit the result inside `area`.
 */
export function maskCells(mask: FigureMask, area: Rect, gap: number, target: number): Rect[] {
  // Fit the figure's bounding box into the area, centred.
  let sw = area.w;
  let sh = sw / mask.aspect;
  if (sh > area.h) {
    sh = area.h;
    sw = sh * mask.aspect;
  }
  const sx = area.x + (area.w - sw) / 2;
  const sy = area.y + (area.h - sh) / 2;
  const hit = (px: number, py: number) => mask.inside((px - sx) / sw, (py - sy) / sh);

  const cellsFor = (s: number): Rect[] => {
    const pitch = s + gap;
    const cols = Math.ceil(sw / pitch) + 2;
    const rows = Math.ceil(sh / pitch) + 2;
    const cx = sx + sw / 2;
    const out: Rect[] = [];
    // Columns symmetric around the vertical axis; rows from the top.
    const colOffset = (cols % 2 === 0 ? 0.5 : 0) * pitch;
    const d = s * 0.3;
    for (let r = 0; r < rows; r++) {
      const py = sy + s / 2 + r * pitch;
      for (let c = -Math.ceil(cols / 2); c <= Math.ceil(cols / 2); c++) {
        const px = cx + c * pitch + colOffset;
        let ok = hit(px, py);
        if (ok && mask.strict) {
          let n = 0;
          for (const [dx, dy] of [[-d, -d], [d, -d], [-d, d], [d, d]]) if (hit(px + dx, py + dy)) n++;
          ok = n >= 2;
        }
        if (ok) out.push({ x: px - s / 2, y: py - s / 2, w: s, h: s });
      }
    }
    return out;
  };

  let lo = Math.min(sw, sh) / 200;
  let hi = Math.min(sw, sh);
  let best = cellsFor(lo);
  for (let i = 0; i < 36; i++) {
    const mid = (lo + hi) / 2;
    const cells = cellsFor(mid);
    if (cells.length >= target) {
      best = cells;
      lo = mid;
    } else {
      hi = mid;
    }
  }
  // Cells whose centres sit near the outline overhang it by half a cell:
  // shrink the figure if needed, then centre it inside the area.
  if (best.length) {
    const minX = Math.min(...best.map((c) => c.x));
    const maxX = Math.max(...best.map((c) => c.x + c.w));
    const minY = Math.min(...best.map((c) => c.y));
    const maxY = Math.max(...best.map((c) => c.y + c.h));
    const k = Math.min(1, area.w / (maxX - minX), area.h / (maxY - minY));
    const fw = (maxX - minX) * k;
    const fh = (maxY - minY) * k;
    const ox = area.x + (area.w - fw) / 2;
    const oy = area.y + (area.h - fh) / 2;
    for (const c of best) {
      c.x = ox + (c.x - minX) * k;
      c.y = oy + (c.y - minY) * k;
      c.w *= k;
      c.h *= k;
    }
  }
  return best;
}

export function shapeCells(shape: LayoutShape, area: Rect, gap: number, target: number): Rect[] {
  return maskCells(implicitMask(shape), area, gap, target);
}

/* ----------------------------------------------------------------- letters */

const textMaskCache = new Map<string, FigureMask>();

/** Rasterise text into a figure mask (needs a canvas; returns null in Node). */
export function textMask(text: string, font: string, weight: number): FigureMask | null {
  // The fallback font is used until the real one loads: key the cache on that too.
  const key = `${font}|${weight}|${isFontLoaded(font, weight) ? 1 : 0}|${text}`;
  const hit = textMaskCache.get(key);
  if (hit) return hit;
  if (typeof OffscreenCanvas === 'undefined' && typeof document === 'undefined') return null;
  const lines = text.split('\n').map((l) => l.trim()).filter((l) => l.length > 0);
  if (!lines.length) return null;
  const size = 160;
  const lineHeight = size * 1.02;
  const pad = size;
  const probe = ctx2d(makeCanvas(8, 8));
  probe.font = cssFont(font, weight, size);
  const widths = lines.map((l) => probe.measureText(l).width);
  const W = Math.ceil(Math.max(...widths) + pad * 2);
  const H = Math.ceil(lines.length * lineHeight + pad * 2);
  const canvas = makeCanvas(W, H);
  const c = ctx2d(canvas);
  c.font = cssFont(font, weight, size);
  c.fillStyle = '#000';
  c.textBaseline = 'middle';
  c.textAlign = 'center';
  lines.forEach((l, i) => c.fillText(l, W / 2, pad + (i + 0.5) * lineHeight));
  const data = c.getImageData(0, 0, W, H).data;
  let x0 = W;
  let x1 = -1;
  let y0 = H;
  let y1 = -1;
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      if (data[(y * W + x) * 4 + 3] > 127) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        if (y > y1) y1 = y;
      }
    }
  }
  if (x1 < 0) return null;
  const bw = x1 - x0 + 1;
  const bh = y1 - y0 + 1;
  const mask: FigureMask = {
    aspect: bw / bh,
    strict: true,
    inside: (u, v) => {
      if (u < 0 || v < 0 || u >= 1 || v >= 1) return false;
      const x = x0 + Math.floor(u * bw);
      const y = y0 + Math.floor(v * bh);
      return data[(y * W + x) * 4 + 3] > 127;
    },
  };
  if (textMaskCache.size > 40) textMaskCache.clear();
  textMaskCache.set(key, mask);
  return mask;
}

export const LETTER_FONTS = ['montserrat', 'poppins', 'fredoka', 'bebas-neue', 'righteous', 'abril-fatface', 'shrikhand', 'oswald'];

export const lettersEngine: Engine = ({ items, area, gap, rng, settings, pageShort }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const text = (settings.text ?? 'LOVE').toUpperCase();
  const font = settings.font ?? 'montserrat';
  const weight = settings.fontWeight ?? 800;
  void ensureFont(font, weight);
  const glyphs = text.replace(/\s/g, '').length || 1;
  const mask = textMask(text, font, weight) ?? { aspect: Math.max(0.6, glyphs * 0.62), inside: () => true };
  // Roughly 60 cells per glyph keep strokes four or five cells wide, so the
  // letterforms stay crisp; photos repeat to fill the cells.
  const target = Math.max(n, glyphs * 60);
  const cells = maskCells(mask, area, gap, target);
  const order = rng.shuffle(items.map((_, i) => i));
  const placements: Placement[] = cells.map((rect, i) => ({ item: order[i % n], rect }));
  return {
    placements,
    style: { mask: { shape: 'rect', radius: Math.min(0.12, (gap / pageShort) * 4) } },
  };
};

export const shapeEngine: Engine = ({ items, area, gap, rng, settings, pageShort }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const shape = settings.shape ?? 'heart';
  const repeat = settings.repeat ?? true;
  // Enough cells for the outline to be recognisable; photos repeat to fill them.
  const minCells = { heart: 40, star: 44, hexagon: 30, circle: 28, diamond: 28 }[shape];
  const target = repeat ? Math.max(n, minCells) : n;
  let cells = shapeCells(shape, area, gap, target);
  if (!repeat && cells.length > n) {
    // Drop the cells closest to the outline first.
    const centre = { x: area.x + area.w / 2, y: area.y + area.h / 2 };
    cells = cells
      .map((c) => ({ c, d: Math.hypot(c.x + c.w / 2 - centre.x, c.y + c.h / 2 - centre.y) }))
      .sort((a, b) => a.d - b.d)
      .slice(0, n)
      .map((e) => e.c)
      .sort((a, b) => a.y - b.y || a.x - b.x);
  }
  const order = rng.shuffle(items.map((_, i) => i));
  const placements: Placement[] = cells.map((rect, i) => ({ item: order[i % n], rect }));
  return {
    placements,
    style: { mask: { shape: 'rect', radius: Math.min(0.12, (gap / pageShort) * 4) } },
  };
};

/* ------------------------------------------------------------- scattered */

export const scatterEngine: Engine = ({ items, area, rng }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const coverage = n <= 3 ? 0.55 : n <= 8 ? 0.85 : 1.15;
  const meanAspect = 1 / 1.16;
  let w = Math.sqrt((area.w * area.h * coverage * meanAspect) / n);
  w = Math.min(w, area.w * 0.62, (area.h / 1.2) * 0.62);
  const cols = Math.max(1, Math.round(Math.sqrt((n * area.w) / area.h)));
  const rows = Math.ceil(n / cols);
  const cw = area.w / cols;
  const ch = area.h / rows;
  const placements: Placement[] = [];
  const order = rng.shuffle(items.map((_, i) => i));
  order.forEach((item, k) => {
    const r = Math.floor(k / cols);
    const c = k % cols;
    const inRow = r === rows - 1 ? n - r * cols : cols;
    const rowOffset = ((cols - inRow) * cw) / 2;
    const photoAspect = Math.min(1.35, Math.max(0.75, items[item].aspect));
    const fw = w * (photoAspect >= 1 ? 1.08 : 0.95);
    const fh = polaroidHeightFor(fw, photoAspect);
    let cx = area.x + rowOffset + (c + 0.5) * cw + rng.range(-0.16, 0.16) * cw;
    let cy = area.y + (r + 0.5) * ch + rng.range(-0.14, 0.14) * ch;
    cx = Math.min(area.x + area.w - fw * 0.42, Math.max(area.x + fw * 0.42, cx));
    cy = Math.min(area.y + area.h - fh * 0.42, Math.max(area.y + fh * 0.42, cy));
    placements.push({
      item,
      rect: { x: cx - fw / 2, y: cy - fh / 2, w: fw, h: fh },
      rotation: rng.range(-11, 11),
    });
  });
  rng.shuffle(placements);
  return {
    placements,
    style: {
      mask: { shape: 'rect', radius: 0 },
      frame: { style: 'polaroid', color: '#ffffff' },
      shadow: { color: 'rgba(20,16,30,0.35)', blur: 0.012, x: 0, y: 0.004 },
    },
  };
};

/* ------------------------------------------------------------------ rings */

export const ringEngine: Engine = ({ items, area, gap }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const cx = area.x + area.w / 2;
  const cy = area.y + area.h / 2;
  const R = Math.min(area.w, area.h) / 2;
  const circle = (x: number, y: number, r: number): Rect => ({ x: x - r, y: y - r, w: 2 * r, h: 2 * r });
  if (n === 1) return { placements: [{ item: 0, rect: circle(cx, cy, R) }], style: { mask: { shape: 'circle', radius: 0 } } };

  const others = n - 1;
  const rc = R * Math.max(0.22, Math.min(0.42, 0.44 - others * 0.008));
  const fit = (r: number): number[] | null => {
    const rings: number[] = [];
    let placed = 0;
    let k = 0;
    while (placed < others) {
      const d = rc + gap + r + k * (2 * r + gap);
      if (d + r > R + 1e-6) return null;
      const s = Math.min(1, (r + gap / 2) / d);
      const cap = Math.max(1, Math.floor(Math.PI / Math.asin(s)));
      const take = Math.min(cap, others - placed);
      rings.push(take);
      placed += take;
      k++;
    }
    return rings;
  };
  let lo = 1;
  let hi = (R - rc - gap) / 2;
  let rings = fit(lo) ?? [others];
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    const f = fit(mid);
    if (f) {
      rings = f;
      lo = mid;
    } else hi = mid;
  }
  const r = lo;
  const placements: Placement[] = [{ item: 0, rect: circle(cx, cy, rc) }];
  let idx = 1;
  rings.forEach((count, k) => {
    const d = rc + gap + r + k * (2 * r + gap);
    for (let j = 0; j < count; j++) {
      const a = -Math.PI / 2 + (j * 2 * Math.PI) / count + (k % 2 ? Math.PI / count : 0);
      placements.push({ item: idx++, rect: circle(cx + Math.cos(a) * d, cy + Math.sin(a) * d, r) });
    }
  });
  return { placements, style: { mask: { shape: 'circle', radius: 0 } } };
};
