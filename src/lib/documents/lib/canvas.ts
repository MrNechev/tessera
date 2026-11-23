// Canvas (whiteboard) geometry, layout and helpers shared by the editor, the canvas
// presentation and the speaker-notes window.
//
// World coordinates are plain CSS pixels at zoom 1. The view maps world → screen as
// screen = world * z + (tx, ty); it is applied as one CSS transform on the world layer.
import { newId } from './api';
import { resolver } from './resolver.svelte';
import type { ArrowEnd, ArrowItem, BoxItem, CanvasItem, Doc, DocCanvas, PathItem, Rect, Section } from './types';

export interface View {
  tx: number;
  ty: number;
  z: number;
}

export const MIN_Z = 0.03;
export const MAX_Z = 5;
export const GRID = 8;

// Sticky-note backgrounds (light, with dark text in both themes) and stroke/ink colours.
export const PALETTE: { key: string; label: string; paper: string; ink: string }[] = [
  { key: 'yellow', label: 'Yellow', paper: '#fbe6a0', ink: '#d39a17' },
  { key: 'orange', label: 'Orange', paper: '#fbcfa6', ink: '#e07a2f' },
  { key: 'red', label: 'Red', paper: '#f7b9b9', ink: '#dd4f4f' },
  { key: 'pink', label: 'Pink', paper: '#f6c3dc', ink: '#cf4f8d' },
  { key: 'purple', label: 'Purple', paper: '#d8cbf6', ink: '#8a68d8' },
  { key: 'blue', label: 'Blue', paper: '#c3d8f8', ink: '#4a86d8' },
  { key: 'teal', label: 'Teal', paper: '#b5e6de', ink: '#2a9d8d' },
  { key: 'green', label: 'Green', paper: '#cfe8bd', ink: '#5aa043' },
  { key: 'gray', label: 'Gray', paper: '#e1dfe6', ink: '#8a8794' },
];
const byKey = new Map(PALETTE.map((p) => [p.key, p]));

export function paper(key: string) {
  return byKey.get(key)?.paper ?? 'transparent';
}
/** 'ink' = the theme's text colour */
export function ink(key: string) {
  return byKey.get(key)?.ink ?? 'var(--text)';
}

export const NOTE_SIZES = [
  { v: 16, label: 'S' },
  { v: 24, label: 'M' },
  { v: 36, label: 'L' },
  { v: 56, label: 'XL' },
  { v: 96, label: 'XXL' },
];

// ---------- rects ----------

export function intersects(a: Rect, b: Rect) {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

export function containsRect(outer: Rect, inner: Rect) {
  return inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.w <= outer.x + outer.w && inner.y + inner.h <= outer.y + outer.h;
}

export function containsPoint(r: Rect, x: number, y: number) {
  return x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
}

export function union(rects: Rect[]): Rect | null {
  if (!rects.length) return null;
  let x0 = Infinity,
    y0 = Infinity,
    x1 = -Infinity,
    y1 = -Infinity;
  for (const r of rects) {
    x0 = Math.min(x0, r.x);
    y0 = Math.min(y0, r.y);
    x1 = Math.max(x1, r.x + r.w);
    y1 = Math.max(y1, r.y + r.h);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

/** grow a rect by a fraction of its size on every side */
export function expand(r: Rect, f: number): Rect {
  return { x: r.x - r.w * f, y: r.y - r.h * f, w: r.w * (1 + 2 * f), h: r.h * (1 + 2 * f) };
}

export function normRect(x0: number, y0: number, x1: number, y1: number): Rect {
  return { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
}

export function center(r: Rect) {
  return { x: r.x + r.w / 2, y: r.y + r.h / 2 };
}

export const snap = (v: number, on = true) => (on ? Math.round(v / GRID) * GRID : v);

// ---------- view ----------

export function visibleRect(v: View, vw: number, vh: number): Rect {
  return { x: -v.tx / v.z, y: -v.ty / v.z, w: vw / v.z, h: vh / v.z };
}

export function toWorld(v: View, sx: number, sy: number) {
  return { x: (sx - v.tx) / v.z, y: (sy - v.ty) / v.z };
}

export function clampZ(z: number) {
  return Math.min(MAX_Z, Math.max(MIN_Z, z));
}

/** view that shows `r` centred in a vw×vh viewport with `pad` (fraction) around it */
export function fitView(r: Rect, vw: number, vh: number, pad = 0.06, maxZ = 1.5): View {
  const w = Math.max(r.w, 1),
    h = Math.max(r.h, 1);
  const z = clampZ(Math.min((vw * (1 - 2 * pad)) / w, (vh * (1 - 2 * pad)) / h, maxZ));
  return { z, tx: vw / 2 - (r.x + w / 2) * z, ty: vh / 2 - (r.y + h / 2) * z };
}

/** zoom by factor `f` keeping screen point (sx, sy) fixed */
export function zoomAt(v: View, f: number, sx: number, sy: number): View {
  const z = clampZ(v.z * f);
  const k = z / v.z;
  return { z, tx: sx - (sx - v.tx) * k, ty: sy - (sy - v.ty) * k };
}

/**
 * Camera flight from view a to view b (both for the same viewport): the centre moves in a
 * straight line in world space while the zoom follows log-space, with a zoom-out "hop"
 * when the targets are far apart relative to the viewport (Prezi-like).
 */
export function flight(a: View, b: View, vw: number, vh: number) {
  const ca = { x: (vw / 2 - a.tx) / a.z, y: (vh / 2 - a.ty) / a.z };
  const cb = { x: (vw / 2 - b.tx) / b.z, y: (vh / 2 - b.ty) / b.z };
  const la = Math.log(a.z),
    lb = Math.log(b.z);
  const dist = Math.hypot(cb.x - ca.x, cb.y - ca.y);
  // how much of the path is visible at the smaller zoom: >1 means a hop helps
  const zMin = Math.min(a.z, b.z);
  const span = (dist * zMin) / Math.max(vw, vh);
  const hop = span > 0.6 ? Math.min(Math.log(1 + span), 2.2) : 0;
  return (t: number): View => {
    const z = Math.exp(la + (lb - la) * t - hop * Math.sin(Math.PI * t));
    const cx = ca.x + (cb.x - ca.x) * t,
      cy = ca.y + (cb.y - ca.y) * t;
    return { z, tx: vw / 2 - cx * z, ty: vh / 2 - cy * z };
  };
}

export const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2);

// ---------- items ----------

export function isBox(it: CanvasItem): it is BoxItem {
  return it.type !== 'arrow';
}

export function ensureCanvas(doc: Doc): DocCanvas {
  if (!doc.canvas || !Array.isArray(doc.canvas.items)) doc.canvas = { items: [] };
  return doc.canvas;
}

/** Rect + shape of everything an arrow can attach to (visible cards and box items). */
export type Target = { rect: Rect; round: boolean };

export function targets(doc: Doc): Map<string, Target> {
  const m = new Map<string, Target>();
  for (const s of doc.sections) if (s.canvas && !s.canvas.hidden) m.set(s.id, { rect: s.canvas, round: false });
  for (const it of doc.canvas?.items ?? []) if (isBox(it) && it.type !== 'path') m.set(it.id, { rect: it, round: it.type === 'ellipse' });
  return m;
}

/** absolute points of a freehand stroke */
export function pathPoints(p: PathItem): [number, number][] {
  return p.points.map(([u, v]) => [p.x + u * p.w, p.y + v * p.h]);
}

/** smooth SVG path through the points (quadratic midpoints) */
export function smoothPath(pts: [number, number][]) {
  if (!pts.length) return '';
  if (pts.length < 3) return 'M' + pts.map((p) => p.join(',')).join('L');
  let d = `M${pts[0][0]},${pts[0][1]}`;
  for (let i = 1; i < pts.length - 1; i++) {
    const mx = (pts[i][0] + pts[i + 1][0]) / 2,
      my = (pts[i][1] + pts[i + 1][1]) / 2;
    d += `Q${pts[i][0]},${pts[i][1]} ${mx},${my}`;
  }
  const l = pts[pts.length - 1];
  return d + `L${l[0]},${l[1]}`;
}

/** Ramer–Douglas–Peucker: drop points closer than eps to the simplified line */
export function simplify(pts: [number, number][], eps: number): [number, number][] {
  if (pts.length < 3) return pts;
  const [ax, ay] = pts[0],
    [bx, by] = pts[pts.length - 1];
  const len = Math.hypot(bx - ax, by - ay) || 1;
  let max = 0,
    idx = 0;
  for (let i = 1; i < pts.length - 1; i++) {
    const d = Math.abs((by - ay) * pts[i][0] - (bx - ax) * pts[i][1] + bx * ay - by * ax) / len;
    if (d > max) {
      max = d;
      idx = i;
    }
  }
  if (max <= eps) return [pts[0], pts[pts.length - 1]];
  return [...simplify(pts.slice(0, idx + 1), eps).slice(0, -1), ...simplify(pts.slice(idx), eps)];
}

/** a stroke item from absolute points */
export function newPath(abs: [number, number][], color: string, width: number, highlighter = false): PathItem {
  const xs = abs.map((p) => p[0]),
    ys = abs.map((p) => p[1]);
  const pad = width / 2;
  const x = Math.min(...xs) - pad,
    y = Math.min(...ys) - pad;
  const w = Math.max(1, Math.max(...xs) + pad - x),
    h = Math.max(1, Math.max(...ys) + pad - y);
  const r = (v: number) => Math.round(v * 10000) / 10000;
  return { id: newId(), type: 'path', x, y, w, h, color, width, highlighter: highlighter || undefined, points: abs.map(([px, py]) => [r((px - x) / w), r((py - y) / h)]) };
}

/** point where the ray from the target's centre towards (px, py) leaves its outline, plus a gap */
export function boundaryPoint(t: Target, px: number, py: number, gap = 8) {
  const c = center(t.rect);
  const dx = px - c.x,
    dy = py - c.y;
  if (!dx && !dy) return c;
  const hw = t.rect.w / 2,
    hh = t.rect.h / 2;
  let k: number;
  if (t.round) k = 1 / Math.sqrt((dx * dx) / (hw * hw) + (dy * dy) / (hh * hh));
  else k = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity);
  const len = Math.hypot(dx, dy);
  const kk = k + gap / len;
  return { x: c.x + dx * Math.min(kk, 1e6), y: c.y + dy * Math.min(kk, 1e6) };
}

/**
 * Resolved arrow geometry (attached ends follow their targets). Curved arrows bend
 * through a control point offset perpendicular to the line between the two ends.
 */
export function arrowPoints(a: ArrowItem, tg: Map<string, Target>) {
  const ft = a.from.id ? tg.get(a.from.id) : undefined;
  const tt = a.to.id ? tg.get(a.to.id) : undefined;
  const fRef = ft ? center(ft.rect) : a.from;
  const tRef = tt ? center(tt.rect) : a.to;
  const curve = a.curve ?? 0;
  // control point from the centres, so attached ends leave towards the bend
  const mx = (fRef.x + tRef.x) / 2,
    my = (fRef.y + tRef.y) / 2;
  const len = Math.hypot(tRef.x - fRef.x, tRef.y - fRef.y) || 1;
  const nx = -(tRef.y - fRef.y) / len,
    ny = (tRef.x - fRef.x) / len;
  const c = { x: mx + nx * curve, y: my + ny * curve };
  const aim1 = curve ? c : tRef,
    aim2 = curve ? c : fRef;
  const p1 = ft ? boundaryPoint(ft, aim1.x, aim1.y) : { x: a.from.x, y: a.from.y };
  const p2 = tt ? boundaryPoint(tt, aim2.x, aim2.y) : { x: a.to.x, y: a.to.y };
  return { p1, p2, c, curved: !!curve, normal: { x: nx, y: ny }, mid: { x: mx, y: my } };
}

/** SVG path + label position for an arrow */
export function arrowPath(a: ArrowItem, tg: Map<string, Target>) {
  const g = arrowPoints(a, tg);
  const d = g.curved ? `M${g.p1.x},${g.p1.y}Q${g.c.x},${g.c.y} ${g.p2.x},${g.p2.y}` : `M${g.p1.x},${g.p1.y}L${g.p2.x},${g.p2.y}`;
  const label = g.curved
    ? { x: 0.25 * g.p1.x + 0.5 * g.c.x + 0.25 * g.p2.x, y: 0.25 * g.p1.y + 0.5 * g.c.y + 0.25 * g.p2.y }
    : { x: (g.p1.x + g.p2.x) / 2, y: (g.p1.y + g.p2.y) / 2 };
  return { ...g, d, label };
}

export function arrowRect(a: ArrowItem, tg: Map<string, Target>): Rect {
  const g = arrowPoints(a, tg);
  const r = normRect(g.p1.x, g.p1.y, g.p2.x, g.p2.y);
  if (!g.curved) return r;
  const lx = 0.25 * g.p1.x + 0.5 * g.c.x + 0.25 * g.p2.x,
    ly = 0.25 * g.p1.y + 0.5 * g.c.y + 0.25 * g.p2.y;
  return union([r, { x: lx, y: ly, w: 0, h: 0 }])!;
}

/** detach arrow ends that point at removed targets (they stay where they were) */
export function detachArrows(doc: Doc, removed: Set<string>, tg: Map<string, Target>) {
  for (const it of doc.canvas?.items ?? []) {
    if (it.type !== 'arrow') continue;
    const { p1, p2 } = arrowPoints(it, tg);
    if (it.from.id && removed.has(it.from.id)) it.from = { x: p1.x, y: p1.y };
    if (it.to.id && removed.has(it.to.id)) it.to = { x: p2.x, y: p2.y };
  }
}

export function cloneItem<T extends CanvasItem>(it: T, dx: number, dy: number): T {
  const c = structuredClone(it) as T;
  c.id = newId();
  if (c.type === 'arrow') {
    const a = c as ArrowItem;
    a.from = { x: a.from.x + dx, y: a.from.y + dy };
    a.to = { x: a.to.x + dx, y: a.to.y + dy };
  } else {
    (c as BoxItem).x += dx;
    (c as BoxItem).y += dy;
  }
  return c;
}

// ---------- section cards ----------

function plainText(json: any): string {
  if (!json) return '';
  if (typeof json.text === 'string') return json.text;
  if (Array.isArray(json.content)) return json.content.map(plainText).join(json.type === 'doc' ? '\n' : '');
  return '';
}

export function mediaRatio(ref: { libraryId: string; path: string } | undefined) {
  if (!ref) return 16 / 9;
  const f = resolver.cache.get(ref.libraryId + '\u0000' + ref.path)?.file;
  return f && f.width && f.height ? f.width / f.height : 16 / 9;
}

export const CARD_PAD = 12;
export const CARD_HEAD = 46;

/** Natural size of a section card for its layout and content (the user can resize it). */
export function defaultCardSize(s: Section): { w: number; h: number } {
  const n = s.media.length;
  const layout = n === 0 ? 'text-only' : s.layout;
  const text = plainText(s.text).trim();
  const head = s.title ? CARD_HEAD : 0;
  const textH = (w: number) => {
    if (!text) return 0;
    const perLine = Math.max(12, (w - 2 * CARD_PAD) / 8.6);
    const lines = text.split('\n').reduce((a, p) => a + Math.max(1, Math.ceil(p.length / perLine)), 0);
    return Math.min(lines * 27 + 8, 420);
  };
  const r = mediaRatio(s.media[0]);
  switch (layout) {
    case 'text-only': {
      const w = 420;
      return { w, h: Math.max(120, head + textH(w) + 2 * CARD_PAD + (head ? 0 : 6)) };
    }
    case 'media-full': {
      const w = r >= 1 ? 640 : 400;
      return { w, h: Math.round(w / r) };
    }
    case 'media-left':
    case 'media-right': {
      const w = 760;
      const mw = text ? w * 0.56 : w - 2 * CARD_PAD;
      const mh = mw / r;
      return { w, h: Math.round(head + Math.max(mh, text ? Math.min(textH(w * 0.44), 420) : 0) + 2 * CARD_PAD) };
    }
    case 'gallery': {
      const carousel = s.gallery?.mode === 'carousel';
      const cols = carousel ? 1 : s.gallery?.columns || Math.ceil(Math.sqrt(n));
      const rows = carousel ? 1 : Math.ceil(n / cols);
      const w = carousel ? 560 : Math.min(900, 220 * cols + 2 * CARD_PAD);
      const cell = (w - 2 * CARD_PAD - (cols - 1) * 6) / cols;
      const mh = carousel ? cell / r : rows * cell * 0.75 + (rows - 1) * 6;
      return { w, h: Math.round(head + mh + (text ? 10 + textH(w) : 0) + 2 * CARD_PAD) };
    }
    default: {
      // media-top (1 or several media side by side)
      const w = n > 1 ? 720 : r >= 1 ? 560 : 400;
      const cols = Math.min(n, 2);
      const rows = Math.ceil(n / cols);
      const cell = (w - 2 * CARD_PAD - (cols - 1) * 6) / cols;
      const mh = n > 1 ? rows * cell * 0.75 + (rows - 1) * 6 : cell / r;
      return { w, h: Math.round(head + mh + (text ? 10 + textH(w) : 0) + 2 * CARD_PAD) };
    }
  }
}

/** Wait until the resolver knows these refs (media sizes), at most `ms`. */
export async function whenResolved(refs: { libraryId: string; path: string }[], ms = 2000) {
  resolver.prefetch(refs);
  const t0 = performance.now();
  while (performance.now() - t0 < ms) {
    if (refs.every((r) => resolver.cache.has(r.libraryId + '\u0000' + r.path))) return;
    await new Promise((r) => setTimeout(r, 50));
  }
}

/**
 * Give every section without a canvas position one: in reading order, in rows below what
 * is already on the canvas (or from the origin on an empty canvas).
 */
export function autoPlace(doc: Doc, perRow = 4, gap = 80) {
  const unplaced = doc.sections.filter((s) => !s.canvas);
  if (!unplaced.length) return [];
  const placed: Rect[] = doc.sections.filter((s) => s.canvas && !s.canvas.hidden).map((s) => s.canvas!);
  for (const it of doc.canvas?.items ?? []) if (isBox(it)) placed.push(it);
  const bb = union(placed);
  let x0 = bb ? bb.x : 0;
  let y = bb ? bb.y + bb.h + gap * 2 : 0;
  let x = x0,
    rowH = 0,
    col = 0;
  for (const s of unplaced) {
    const { w, h } = defaultCardSize(s);
    if (col === perRow) {
      col = 0;
      x = x0;
      y += rowH + gap;
      rowH = 0;
    }
    s.canvas = { x: snap(x), y: snap(y), w, h };
    x += w + gap;
    rowH = Math.max(rowH, h);
    col++;
  }
  return unplaced;
}

/** Everything that has a box on the canvas (for fit-all). */
export function contentRect(doc: Doc): Rect | null {
  const rs: Rect[] = doc.sections.filter((s) => s.canvas && !s.canvas.hidden).map((s) => s.canvas!);
  const tg = targets(doc);
  for (const it of doc.canvas?.items ?? []) rs.push(isBox(it) ? it : arrowRect(it, tg));
  return union(rs);
}

/** Presentation steps: frames in their order, or (without frames) the slide cards in document order. */
export interface Step {
  id: string;
  rect: Rect;
  title: string;
  notes: string;
  frame: boolean;
}

export function steps(doc: Doc): Step[] {
  const frames = (doc.canvas?.items ?? []).filter((i) => i.type === 'frame');
  if (frames.length)
    return frames.map((f, i) => ({ id: f.id, rect: { x: f.x, y: f.y, w: f.w, h: f.h }, title: f.title || `Frame ${i + 1}`, notes: f.notes || '', frame: true }));
  return doc.sections
    .filter((s) => s.slide && s.canvas && !s.canvas.hidden)
    .map((s, i) => ({ id: s.id, rect: { ...s.canvas! }, title: s.title || `Card ${i + 1}`, notes: s.notes || '', frame: false }));
}

// ---------- undo ----------

/** Snapshot-based undo for canvas edits (positions, sizes, items, hidden flags). */
export class CanvasHistory {
  private past: string[] = [];
  private future: string[] = [];
  constructor(private limit = 150) {}

  private snap(doc: Doc) {
    return JSON.stringify({
      items: doc.canvas?.items ?? [],
      secs: doc.sections.map((s) => [s.id, s.canvas ?? null]),
    });
  }

  private apply(doc: Doc, raw: string) {
    const v = JSON.parse(raw) as { items: CanvasItem[]; secs: [string, Section['canvas'] | null][] };
    ensureCanvas(doc).items = v.items;
    const m = new Map(v.secs);
    for (const s of doc.sections) {
      if (!m.has(s.id)) continue;
      const c = m.get(s.id);
      if (c) s.canvas = c;
      else delete s.canvas;
    }
  }

  /** call before a change */
  push(doc: Doc) {
    const s = this.snap(doc);
    if (this.past[this.past.length - 1] === s) return;
    this.past.push(s);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }

  undo(doc: Doc) {
    const s = this.past.pop();
    if (!s) return false;
    this.future.push(this.snap(doc));
    this.apply(doc, s);
    return true;
  }

  redo(doc: Doc) {
    const s = this.future.pop();
    if (!s) return false;
    this.past.push(this.snap(doc));
    this.apply(doc, s);
    return true;
  }

  clear() {
    this.past = [];
    this.future = [];
  }

  get canUndo() {
    return this.past.length > 0;
  }
}

export function newBox(type: BoxItem['type'], r: Rect, extra: Partial<BoxItem> = {}): BoxItem {
  const base = { id: newId(), x: r.x, y: r.y, w: r.w, h: r.h };
  if (type === 'note') return { ...base, type, text: '', color: 'yellow', size: 24, ...extra } as BoxItem;
  if (type === 'frame') return { ...base, type, title: '', ...extra } as BoxItem;
  return { ...base, type, color: 'ink', ...extra } as BoxItem;
}

export function newArrow(from: ArrowEnd, to: ArrowEnd): ArrowItem {
  return { id: newId(), type: 'arrow', from, to, color: 'ink', head: 'end' };
}

// ---------- align / distribute / arrange ----------

export type Align = 'left' | 'center' | 'right' | 'top' | 'middle' | 'bottom';

export function alignRects(rs: Rect[], how: Align) {
  const u = union(rs);
  if (!u) return;
  for (const r of rs) {
    if (how === 'left') r.x = u.x;
    else if (how === 'right') r.x = u.x + u.w - r.w;
    else if (how === 'center') r.x = u.x + (u.w - r.w) / 2;
    else if (how === 'top') r.y = u.y;
    else if (how === 'bottom') r.y = u.y + u.h - r.h;
    else r.y = u.y + (u.h - r.h) / 2;
  }
}

/** equal gaps between the boxes along an axis (outer boxes stay put) */
export function distributeRects(rs: Rect[], axis: 'x' | 'y') {
  if (rs.length < 3) return;
  const sz = axis === 'x' ? 'w' : 'h';
  const sorted = [...rs].sort((a, b) => a[axis] - b[axis]);
  const first = sorted[0],
    last = sorted[sorted.length - 1];
  const total = sorted.reduce((a, r) => a + r[sz], 0);
  const gap = (last[axis] + last[sz] - first[axis] - total) / (sorted.length - 1);
  let pos = first[axis];
  for (const r of sorted) {
    r[axis] = pos;
    pos += r[sz] + gap;
  }
}

/** lay boxes out in rows (reading order of the array), starting at (x0, y0) */
export function gridLayout(rs: Rect[], x0: number, y0: number, perRow = 0, gap = 60) {
  const n = rs.length;
  if (!n) return;
  perRow = perRow || Math.max(1, Math.round(Math.sqrt(n * 1.6)));
  let x = x0,
    y = y0,
    rowH = 0;
  rs.forEach((r, i) => {
    if (i && i % perRow === 0) {
      x = x0;
      y += rowH + gap;
      rowH = 0;
    }
    r.x = snap(x);
    r.y = snap(y);
    x += r.w + gap;
    rowH = Math.max(rowH, r.h);
  });
}

// ---------- clipboard ----------

export const CLIP_PREFIX = 'letopis-canvas:';

export interface ClipPayload {
  v: 1;
  items: CanvasItem[];
  sections: Section[];
}

export function parseClip(text: string | undefined | null): ClipPayload | null {
  if (!text?.startsWith(CLIP_PREFIX)) return null;
  try {
    const p = JSON.parse(text.slice(CLIP_PREFIX.length));
    return p && p.v === 1 && Array.isArray(p.items) && Array.isArray(p.sections) ? p : null;
  } catch {
    return null;
  }
}

/** give pasted content new ids (arrows re-attach to the copies) */
export function remapClip(p: ClipPayload): ClipPayload {
  const ids = new Map<string, string>();
  const groups = new Map<string, string>();
  const g = (x?: string) => (x ? (groups.get(x) ?? (groups.set(x, newId()), groups.get(x)!)) : undefined);
  const sections = p.sections.map((s) => {
    const n = { ...structuredClone(s), id: newId() };
    ids.set(s.id, n.id);
    if (n.canvas) n.canvas = { ...n.canvas, hidden: undefined, group: g(n.canvas.group) };
    return n;
  });
  const items = p.items.map((it) => {
    const n = { ...structuredClone(it), id: newId() } as CanvasItem;
    ids.set(it.id, n.id);
    (n as any).group = g((it as any).group);
    return n;
  });
  for (const it of items)
    if (it.type === 'arrow') {
      it.from = it.from.id && ids.has(it.from.id) ? { ...it.from, id: ids.get(it.from.id) } : { x: it.from.x, y: it.from.y };
      it.to = it.to.id && ids.has(it.to.id) ? { ...it.to, id: ids.get(it.to.id) } : { x: it.to.x, y: it.to.y };
    }
  return { v: 1, items, sections };
}

/** move pasted content so its bounding box is centred on (cx, cy) */
export function offsetClip(p: ClipPayload, cx: number, cy: number) {
  const rs: Rect[] = [...p.sections.filter((s) => s.canvas).map((s) => s.canvas!), ...(p.items.filter(isBox) as BoxItem[])];
  const pts = p.items.filter((i) => i.type === 'arrow').flatMap((a) => [(a as ArrowItem).from, (a as ArrowItem).to].filter((e) => !e.id));
  const u = union([...rs, ...pts.map((e) => ({ x: e.x, y: e.y, w: 0, h: 0 }))]);
  if (!u) return;
  const dx = snap(cx - (u.x + u.w / 2)),
    dy = snap(cy - (u.y + u.h / 2));
  for (const r of rs) {
    r.x += dx;
    r.y += dy;
  }
  for (const e of pts) {
    e.x += dx;
    e.y += dy;
  }
}
