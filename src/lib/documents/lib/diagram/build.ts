// DiagramGraph → canvas items: shapes with their text, connectors attached to them and
// frames for the groups. Graphs without positions are laid out with dagre.
import { newId } from '../api';
import type { ArrowItem, CanvasItem, FrameItem, NoteItem, Rect, Route, ShapeItem } from '../types';
import type { DiagramGraph, GraphNode } from './graph';
import { layered } from './layout';

export interface BuildOptions {
  /** connector route when the graph doesn't say (default elbow) */
  route?: Route;
  /** outline / fill of nodes without their own colours (palette keys or #hex) */
  stroke?: string;
  fill?: string;
  /** text size of nodes without one */
  size?: number;
  /** scale for positioned graphs (draw.io pixels are small next to canvas items) */
  scale?: number;
  nodesep?: number;
  ranksep?: number;
}

const ROUNDED = 16;

/** a size that fits the label (about 10 px per character at text size 20) */
export function nodeSize(n: Pick<GraphNode, 'label' | 'shape'>, size = 20): { w: number; h: number } {
  const lines = (n.label || ' ').split('\n');
  const cw = size * 0.55;
  const longest = Math.max(...lines.map((l) => l.length));
  let w = Math.min(Math.max(longest * cw + size * 2.4, size * 6), size * 16);
  const perLine = Math.max(4, Math.floor((w - size * 2) / cw));
  const rows = lines.reduce((a, l) => a + Math.max(1, Math.ceil(l.length / perLine)), 0);
  let h = Math.max(size * 3.2, rows * size * 1.3 + size * 1.6);
  switch (n.shape) {
    case 'diamond':
      w *= 1.5;
      h *= 1.6;
      break;
    case 'ellipse':
    case 'cloud':
      w *= 1.25;
      h *= 1.3;
      break;
    case 'hexagon':
    case 'parallelogram':
      w *= 1.25;
      break;
    case 'cylinder':
      h += size * 1.6;
      break;
    case 'triangle':
    case 'star':
      w *= 1.6;
      h *= 2;
      break;
    case 'callout':
    case 'document':
      h *= 1.3;
      break;
  }
  return { w: Math.round(w), h: Math.round(h) };
}

function shapeOf(n: GraphNode, rect: Rect, o: BuildOptions): ShapeItem | NoteItem {
  const size = n.fontSize ?? o.size ?? 20;
  if (n.shape === 'text') {
    const note: NoteItem = { id: newId(), type: 'note', ...rect, text: n.label, color: 'none', size, align: 'center' };
    if (n.fontColor) note.look = { font: 'inter', fill: n.fontColor };
    return note;
  }
  const it: ShapeItem = { id: newId(), type: 'rect', ...rect, color: n.stroke ?? o.stroke ?? 'ink', text: n.label || undefined, size };
  if (n.shape === 'ellipse') it.type = 'ellipse';
  else if (n.shape !== 'rect') it.shape = n.shape;
  else if (n.pill) it.radius = Math.round(rect.h / 2);
  else if (n.rounded) it.radius = ROUNDED;
  const fill = n.fill ?? o.fill;
  if (fill) it.fillColor = fill;
  it.strokeWidth = n.strokeWidth ?? 2;
  if (n.dash && n.dash !== 'solid') it.dash = n.dash;
  if (n.fontColor) it.look = { font: 'inter', fill: n.fontColor };
  return it;
}

/** Items for a graph, with the diagram's top-left at (0, 0). */
export function buildDiagram(g: DiagramGraph, o: BuildOptions = {}): { items: CanvasItem[]; bounds: Rect } {
  const size = o.size ?? 20;
  const rects = new Map<string, Rect>();
  const groupRects = new Map<string, Rect>();
  if (g.positioned) {
    const k = o.scale ?? 1;
    for (const n of g.nodes) rects.set(n.id, { x: (n.x ?? 0) * k, y: (n.y ?? 0) * k, w: Math.max(8, (n.w ?? 120) * k), h: Math.max(8, (n.h ?? 60) * k) });
    for (const gr of g.groups) if (gr.w && gr.h) groupRects.set(gr.id, { x: (gr.x ?? 0) * k, y: (gr.y ?? 0) * k, w: gr.w * k, h: gr.h * k });
  } else {
    const sizes = new Map(g.nodes.map((n) => [n.id, nodeSize(n, n.fontSize ?? size)]));
    const res = layered(
      g.nodes.map((n) => ({ id: n.id, ...sizes.get(n.id)!, parent: n.parent })),
      g.edges.filter((e) => e.from && e.to).map((e) => ({ from: e.from!, to: e.to!, labelW: e.label ? e.label.length * 9 + 16 : 0, labelH: e.label ? 26 : 0 })),
      g.direction,
      { groups: g.groups, nodesep: o.nodesep, ranksep: o.ranksep },
    );
    for (const n of g.nodes) {
      const p = res.pos.get(n.id) ?? { x: 0, y: 0 };
      const s = sizes.get(n.id)!;
      rects.set(n.id, { x: Math.round(p.x), y: Math.round(p.y), w: s.w, h: s.h });
    }
    for (const [id, r] of res.groups) groupRects.set(id, { x: Math.round(r.x - 10), y: Math.round(r.y - 10), w: Math.round(r.w + 20), h: Math.round(r.h + 20) });
  }

  const ids = new Map<string, string>();
  const frames: FrameItem[] = [];
  // outer groups first: frames are presented in order
  const depth = (id: string, seen = 0): number => {
    const p = g.groups.find((x) => x.id === id)?.parent;
    return p && seen < 20 ? 1 + depth(p, seen + 1) : 0;
  };
  for (const gr of [...g.groups].sort((a, b) => depth(a.id) - depth(b.id))) {
    const r = groupRects.get(gr.id);
    if (!r) continue;
    const f: FrameItem = { id: newId(), type: 'frame', ...r, title: gr.label || 'Group' };
    ids.set(gr.id, f.id);
    frames.push(f);
  }
  const shapes: (ShapeItem | NoteItem)[] = [];
  for (const n of g.nodes) {
    const it = shapeOf(n, rects.get(n.id)!, o);
    ids.set(n.id, it.id);
    shapes.push(it);
  }
  const k = g.positioned ? (o.scale ?? 1) : 1;
  const arrows: ArrowItem[] = [];
  for (const e of g.edges) {
    const fid = e.from ? ids.get(e.from) : undefined;
    const tid = e.to ? ids.get(e.to) : undefined;
    const fp = e.fromPoint ? { x: e.fromPoint.x * k, y: e.fromPoint.y * k } : undefined;
    const tp = e.toPoint ? { x: e.toPoint.x * k, y: e.toPoint.y * k } : undefined;
    if ((!fid && !fp) || (!tid && !tp)) continue;
    const centre = (id?: string) => {
      const r = id ? (rects.get(id) ?? groupRects.get(id)) : undefined;
      return r ? { x: r.x + r.w / 2, y: r.y + r.h / 2 } : { x: 0, y: 0 };
    };
    const from = fid ? { ...centre(e.from), id: fid } : fp!;
    const to = tid ? { ...centre(e.to), id: tid } : tp!;
    const a: ArrowItem = { id: newId(), type: 'arrow', from, to, color: e.stroke ?? 'ink', start: e.start ?? 'none', end: e.end ?? 'arrow' };
    const route = e.route ?? o.route ?? 'elbow';
    if (route !== 'straight') a.route = route;
    if (e.dash && e.dash !== 'solid') a.dash = e.dash;
    if (e.width && e.width !== 3) a.width = e.width;
    if (e.label) a.text = e.label;
    arrows.push(a);
  }

  const all: Rect[] = [...rects.values(), ...groupRects.values()];
  const x0 = Math.min(0, ...all.map((r) => r.x)),
    y0 = Math.min(0, ...all.map((r) => r.y));
  const x1 = Math.max(0, ...all.map((r) => r.x + r.w)),
    y1 = Math.max(0, ...all.map((r) => r.y + r.h));
  const items: CanvasItem[] = [...frames, ...shapes, ...arrows];
  // move to (0, 0)
  for (const it of items) {
    if (it.type === 'arrow') {
      it.from = { ...it.from, x: it.from.x - x0, y: it.from.y - y0 };
      it.to = { ...it.to, x: it.to.x - x0, y: it.to.y - y0 };
    } else {
      it.x -= x0;
      it.y -= y0;
    }
  }
  return { items, bounds: { x: 0, y: 0, w: x1 - x0, h: y1 - y0 } };
}
