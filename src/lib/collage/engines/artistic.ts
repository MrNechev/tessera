/** Creative engines: photos forming a shape, scattered polaroids, and concentric rings. */
import type { LayoutShape, Rect } from '../../model/types';
import { polaroidHeightFor } from '../../render/frameGeometry';
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
  // Classic implicit heart curve: (x² + y² − 1)³ − x²y³ = 0
  heart: (x, y) => {
    const a = x * x + y * y - 1;
    return a * a * a - x * x * y * y * y;
  },
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

export function shapeCells(shape: LayoutShape, area: Rect, gap: number, target: number): Rect[] {
  const b = SHAPE_BOUNDS[shape];
  const f = SHAPE_FNS[shape];
  const shapeAspect = (b.x1 - b.x0) / (b.y1 - b.y0);
  // Fit the shape's bounding box into the area, centred.
  let sw = area.w;
  let sh = sw / shapeAspect;
  if (sh > area.h) {
    sh = area.h;
    sw = sh * shapeAspect;
  }
  const sx = area.x + (area.w - sw) / 2;
  const sy = area.y + (area.h - sh) / 2;
  const toShape = (px: number, py: number): [number, number] => [
    b.x0 + ((px - sx) / sw) * (b.x1 - b.x0),
    b.y1 - ((py - sy) / sh) * (b.y1 - b.y0),
  ];

  const cellsFor = (s: number): Rect[] => {
    const pitch = s + gap;
    const cols = Math.ceil(sw / pitch) + 2;
    const rows = Math.ceil(sh / pitch) + 2;
    const cx = sx + sw / 2;
    const out: Rect[] = [];
    // Columns symmetric around the vertical axis; rows from the top.
    const colOffset = (cols % 2 === 0 ? 0.5 : 0) * pitch;
    for (let r = 0; r < rows; r++) {
      const py = sy + s / 2 + r * pitch;
      for (let c = -Math.ceil(cols / 2); c <= Math.ceil(cols / 2); c++) {
        const px = cx + c * pitch + colOffset;
        const [x, y] = toShape(px, py);
        if (f(x, y) <= 0) out.push({ x: px - s / 2, y: py - s / 2, w: s, h: s });
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
  // Recentre the resulting figure vertically/horizontally inside the area.
  if (best.length) {
    const minX = Math.min(...best.map((c) => c.x));
    const maxX = Math.max(...best.map((c) => c.x + c.w));
    const minY = Math.min(...best.map((c) => c.y));
    const maxY = Math.max(...best.map((c) => c.y + c.h));
    const dx = area.x + (area.w - (maxX - minX)) / 2 - minX;
    const dy = area.y + (area.h - (maxY - minY)) / 2 - minY;
    for (const c of best) {
      c.x += dx;
      c.y += dy;
    }
  }
  return best;
}

export const shapeEngine: Engine = ({ items, area, gap, rng, settings, pageShort }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const shape = settings.shape ?? 'heart';
  const repeat = settings.repeat ?? true;
  const minCells = shape === 'circle' || shape === 'diamond' ? 13 : 18;
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
