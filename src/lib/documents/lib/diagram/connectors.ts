// Connector geometry: connection points (side ports), straight / curved / elbow routes,
// arrowheads and positions along the line. Elbow routes are orthogonal and go around the
// shapes at both ends (and other boxes near the route) with as few bends as possible.
import type { ArrowItem, Dash, Head, Rect, Side } from '../types';

export interface Pt {
  x: number;
  y: number;
}

export const SIDES: Side[] = ['top', 'right', 'bottom', 'left'];
export const SIDE_DIR: Record<Side, Pt> = { top: { x: 0, y: -1 }, right: { x: 1, y: 0 }, bottom: { x: 0, y: 1 }, left: { x: -1, y: 0 } };

export const HEADS: { id: Head; label: string }[] = [
  { id: 'none', label: 'None' },
  { id: 'arrow', label: 'Arrow' },
  { id: 'open', label: 'Open arrow' },
  { id: 'dot', label: 'Dot' },
  { id: 'diamond', label: 'Diamond' },
  { id: 'bar', label: 'Bar' },
];

/** arrowheads of an arrow (old `head` values when start/end are unset) */
export function headsOf(a: Pick<ArrowItem, 'head' | 'start' | 'end'>): { start: Head; end: Head } {
  return { start: a.start ?? (a.head === 'both' ? 'arrow' : 'none'), end: a.end ?? (a.head === 'none' ? 'none' : 'arrow') };
}

export function dashOf(x: { dash?: Dash; dashed?: boolean }): Dash {
  return x.dash ?? (x.dashed ? 'dashed' : 'solid');
}

/** SVG stroke-dasharray for a dash style at line width w */
export function dashArray(d: Dash, w: number): string | undefined {
  if (d === 'dashed') return `${Math.max(6, w * 3.3)} ${Math.max(5, w * 2.7)}`;
  if (d === 'dotted') return `0.01 ${Math.max(5, w * 2)}`;
  return undefined;
}

/** the side of `r` that faces point p (by the larger relative offset) */
export function autoSide(r: Rect, p: Pt): Side {
  const dx = (p.x - (r.x + r.w / 2)) / Math.max(1, r.w),
    dy = (p.y - (r.y + r.h / 2)) / Math.max(1, r.h);
  if (Math.abs(dx) >= Math.abs(dy)) return dx >= 0 ? 'right' : 'left';
  return dy >= 0 ? 'bottom' : 'top';
}

/** the nearest side port of `r` within `max` of p (ports = middles of the sides) */
export function nearestSide(ports: Record<Side, Pt>, p: Pt, max: number): Side | undefined {
  let best: Side | undefined,
    bd = max;
  for (const s of SIDES) {
    const d = Math.hypot(ports[s].x - p.x, ports[s].y - p.y);
    if (d < bd) {
      bd = d;
      best = s;
    }
  }
  return best;
}

// ---------- polylines ----------

export function polyLength(pts: Pt[]) {
  let l = 0;
  for (let i = 1; i < pts.length; i++) l += Math.hypot(pts[i].x - pts[i - 1].x, pts[i].y - pts[i - 1].y);
  return l;
}

/** point at fraction t (0–1) of the length of a polyline */
export function pointAt(pts: Pt[], t: number): Pt {
  if (pts.length < 2) return pts[0] ?? { x: 0, y: 0 };
  let left = polyLength(pts) * Math.min(1, Math.max(0, t));
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1],
      b = pts[i];
    const l = Math.hypot(b.x - a.x, b.y - a.y);
    if (left <= l || i === pts.length - 1) {
      const k = l ? Math.min(1, left / l) : 0;
      return { x: a.x + (b.x - a.x) * k, y: a.y + (b.y - a.y) * k };
    }
    left -= l;
  }
  return pts[pts.length - 1];
}

/** fraction (0–1) along the polyline of the point nearest to p */
export function nearestT(pts: Pt[], p: Pt): number {
  const total = polyLength(pts) || 1;
  let best = Infinity,
    bestT = 0.5,
    acc = 0;
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1],
      b = pts[i];
    const dx = b.x - a.x,
      dy = b.y - a.y;
    const l2 = dx * dx + dy * dy;
    const k = l2 ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / l2)) : 0;
    const qx = a.x + dx * k,
      qy = a.y + dy * k;
    const d = Math.hypot(p.x - qx, p.y - qy);
    if (d < best) {
      best = d;
      bestT = (acc + Math.sqrt(l2) * k) / total;
    }
    acc += Math.sqrt(l2);
  }
  return bestT;
}

/** drop repeated points and middle points of straight runs */
export function simplifyOrtho(pts: Pt[]): Pt[] {
  const out: Pt[] = [];
  for (const p of pts) {
    const l = out[out.length - 1];
    if (l && Math.abs(l.x - p.x) < 0.01 && Math.abs(l.y - p.y) < 0.01) continue;
    out.push(p);
    while (out.length >= 3) {
      const [a, b, c] = out.slice(-3);
      const col = Math.abs((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) < 0.01;
      if (!col) break;
      out.splice(out.length - 2, 1);
    }
  }
  return out;
}

/** SVG path through the points with rounded corners (radius r, smaller on short segments) */
export function roundedPath(pts: Pt[], r = 10): string {
  if (!pts.length) return '';
  const f = (v: number) => Math.round(v * 100) / 100;
  let d = `M${f(pts[0].x)},${f(pts[0].y)}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const a = pts[i - 1],
      b = pts[i],
      c = pts[i + 1];
    const l1 = Math.hypot(b.x - a.x, b.y - a.y),
      l2 = Math.hypot(c.x - b.x, c.y - b.y);
    const rr = Math.min(r, l1 / 2, l2 / 2);
    if (rr < 0.5) {
      d += `L${f(b.x)},${f(b.y)}`;
      continue;
    }
    const p1 = { x: b.x + ((a.x - b.x) / l1) * rr, y: b.y + ((a.y - b.y) / l1) * rr };
    const p2 = { x: b.x + ((c.x - b.x) / l2) * rr, y: b.y + ((c.y - b.y) / l2) * rr };
    d += `L${f(p1.x)},${f(p1.y)}Q${f(b.x)},${f(b.y)} ${f(p2.x)},${f(p2.y)}`;
  }
  const l = pts[pts.length - 1];
  return d + `L${f(l.x)},${f(l.y)}`;
}

/** cubic Bézier sampled into a polyline */
export function sampleCubic(p0: Pt, c1: Pt, c2: Pt, p3: Pt, n = 24): Pt[] {
  const out: Pt[] = [];
  for (let i = 0; i <= n; i++) {
    const t = i / n,
      u = 1 - t;
    const a = u * u * u,
      b = 3 * u * u * t,
      c = 3 * u * t * t,
      d = t * t * t;
    out.push({ x: a * p0.x + b * c1.x + c * c2.x + d * p3.x, y: a * p0.y + b * c1.y + c * c2.y + d * p3.y });
  }
  return out;
}

/** control points of a curved connector leaving p1 along d1 and entering p2 against d2 */
export function curveControls(p1: Pt, d1: Pt | null, p2: Pt, d2: Pt | null): [Pt, Pt] {
  const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y);
  const k = Math.max(30, dist * 0.45);
  const toward = (a: Pt, b: Pt): Pt => {
    const l = Math.hypot(b.x - a.x, b.y - a.y) || 1;
    return { x: (b.x - a.x) / l, y: (b.y - a.y) / l };
  };
  const a = d1 ?? toward(p1, p2),
    b = d2 ?? toward(p2, p1);
  return [
    { x: p1.x + a.x * k, y: p1.y + a.y * k },
    { x: p2.x + b.x * k, y: p2.y + b.y * k },
  ];
}

// ---------- elbow routing ----------

/** a direction from a free end towards a point: the axis with the bigger distance */
function axisDir(from: Pt, to: Pt): Pt {
  const dx = to.x - from.x,
    dy = to.y - from.y;
  if (Math.abs(dx) >= Math.abs(dy)) return { x: Math.sign(dx) || 1, y: 0 };
  return { x: 0, y: Math.sign(dy) || 1 };
}

const DIRS: Pt[] = [
  { x: 1, y: 0 },
  { x: 0, y: 1 },
  { x: -1, y: 0 },
  { x: 0, y: -1 },
];
const dirIndex = (d: Pt) => DIRS.findIndex((v) => v.x === Math.sign(d.x) && v.y === Math.sign(d.y));

class Heap {
  private a: [number, number][] = [];
  push(v: [number, number]) {
    const a = this.a;
    a.push(v);
    let i = a.length - 1;
    while (i > 0) {
      const p = (i - 1) >> 1;
      if (a[p][0] <= a[i][0]) break;
      [a[p], a[i]] = [a[i], a[p]];
      i = p;
    }
  }
  pop(): [number, number] | undefined {
    const a = this.a;
    if (!a.length) return undefined;
    const top = a[0];
    const last = a.pop()!;
    if (a.length) {
      a[0] = last;
      let i = 0;
      for (;;) {
        const l = 2 * i + 1,
          r = l + 1;
        let m = i;
        if (l < a.length && a[l][0] < a[m][0]) m = l;
        if (r < a.length && a[r][0] < a[m][0]) m = r;
        if (m === i) break;
        [a[m], a[i]] = [a[i], a[m]];
        i = m;
      }
    }
    return top;
  }
  get size() {
    return this.a.length;
  }
}

export interface ElbowOptions {
  /** clearance around obstacles */
  margin?: number;
  /** straight piece out of an attached end before the first bend */
  stub?: number;
  /** cost of a bend in px of length */
  bend?: number;
}

/**
 * Orthogonal route from p1 (leaving along d1; null = free end) to p2 (entered against d2,
 * i.e. d2 points out of the target). Obstacles are avoided with `margin` clearance; the
 * search runs over the lines through the obstacles' edges and both ends (a sparse grid),
 * minimising length plus a cost per bend. Returns the polyline including p1 and p2.
 */
export function routeElbow(p1: Pt, d1: Pt | null, p2: Pt, d2: Pt | null, obstacles: Rect[], opts: ElbowOptions = {}): Pt[] {
  const margin = opts.margin ?? 14,
    stub = opts.stub ?? 18,
    BEND = opts.bend ?? 24;
  const a = d1 ?? axisDir(p1, p2);
  const b = d2 ?? axisDir(p2, p1);
  const s = d1 ? { x: p1.x + a.x * stub, y: p1.y + a.y * stub } : p1;
  const e = d2 ? { x: p2.x + b.x * stub, y: p2.y + b.y * stub } : p2;
  const obs = obstacles.map((o) => ({ x0: o.x - margin, y0: o.y - margin, x1: o.x + o.w + margin, y1: o.y + o.h + margin }));
  const uniq = (v: number[]) => [...new Set(v.map((x) => Math.round(x * 100) / 100))].sort((m, n) => m - n);
  const xs = uniq([s.x, e.x, (s.x + e.x) / 2, ...obs.flatMap((o) => [o.x0, o.x1])]);
  const ys = uniq([s.y, e.y, (s.y + e.y) / 2, ...obs.flatMap((o) => [o.y0, o.y1])]);
  const inside = (x: number, y: number) => obs.some((o) => x > o.x0 + 0.01 && x < o.x1 - 0.01 && y > o.y0 + 0.01 && y < o.y1 - 0.01);
  const nx = xs.length,
    ny = ys.length;
  const si = xs.indexOf(Math.round(s.x * 100) / 100),
    sj = ys.indexOf(Math.round(s.y * 100) / 100);
  const ei = xs.indexOf(Math.round(e.x * 100) / 100),
    ej = ys.indexOf(Math.round(e.y * 100) / 100);
  const ok = (i: number, j: number) => (i === si && j === sj) || (i === ei && j === ej) || !inside(xs[i], ys[j]);
  const blocked = (i: number, j: number, i2: number, j2: number) => inside((xs[i] + xs[i2]) / 2, (ys[j] + ys[j2]) / 2);

  const key = (i: number, j: number, d: number) => (j * nx + i) * 4 + d;
  const dist = new Float64Array(nx * ny * 4).fill(Infinity);
  const prev = new Int32Array(nx * ny * 4).fill(-1);
  const heap = new Heap();
  const d0 = dirIndex(a);
  const endDir = dirIndex({ x: -b.x, y: -b.y });
  dist[key(si, sj, d0)] = 0;
  heap.push([0, key(si, sj, d0)]);
  let goal = -1,
    goalCost = Infinity;
  while (heap.size) {
    const [c, k] = heap.pop()!;
    if (c > dist[k] || c >= goalCost) continue;
    const d = k % 4,
      cell = (k - d) / 4;
    const i = cell % nx,
      j = (cell - i) / nx;
    if (i === ei && j === ej) {
      const turn = d === endDir ? 0 : (d + 2) % 4 === endDir ? 2 * BEND : BEND;
      if (c + turn < goalCost) {
        goalCost = c + turn;
        goal = k;
      }
      continue;
    }
    for (let nd = 0; nd < 4; nd++) {
      if ((nd + 2) % 4 === d) continue; // no U-turns in place
      const i2 = i + DIRS[nd].x,
        j2 = j + DIRS[nd].y;
      if (i2 < 0 || j2 < 0 || i2 >= nx || j2 >= ny || !ok(i2, j2) || blocked(i, j, i2, j2)) continue;
      const len = Math.abs(xs[i2] - xs[i]) + Math.abs(ys[j2] - ys[j]);
      const nc = c + len + (nd === d ? 0 : BEND);
      const k2 = key(i2, j2, nd);
      if (nc < dist[k2]) {
        dist[k2] = nc;
        prev[k2] = k;
        heap.push([nc, k2]);
      }
    }
  }
  let mid: Pt[];
  if (goal >= 0) {
    mid = [];
    for (let k = goal; k >= 0; k = prev[k]) {
      const cell = (k - (k % 4)) / 4;
      const i = cell % nx;
      mid.push({ x: xs[i], y: ys[(cell - i) / nx] });
    }
    mid.reverse();
  } else {
    // no way around: a plain three-segment elbow
    mid = a.x ? [s, { x: (s.x + e.x) / 2, y: s.y }, { x: (s.x + e.x) / 2, y: e.y }, e] : [s, { x: s.x, y: (s.y + e.y) / 2 }, { x: e.x, y: (s.y + e.y) / 2 }, e];
  }
  return simplifyOrtho([p1, ...mid, p2]);
}
