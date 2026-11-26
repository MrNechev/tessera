// Layered (Sugiyama) layout of boxes and the links between them, with dagre: used by
// "Tidy up", the Mermaid import and the templates.
import dagre from '@dagrejs/dagre';
import type { Rect } from '../types';
import type { Direction } from './graph';

export interface LayoutNode {
  id: string;
  w: number;
  h: number;
  /** group (cluster) it belongs to */
  parent?: string;
}
export interface LayoutEdge {
  from: string;
  to: string;
  /** label size, so dagre keeps room for it */
  labelW?: number;
  labelH?: number;
}
export interface LayoutGroup {
  id: string;
  parent?: string;
}

export interface LayoutResult {
  /** top-left of each node */
  pos: Map<string, { x: number; y: number }>;
  /** bounds of each group */
  groups: Map<string, Rect>;
}

export function layered(
  nodes: LayoutNode[],
  edges: LayoutEdge[],
  dir: Direction = 'TB',
  opts: { nodesep?: number; ranksep?: number; groups?: LayoutGroup[] } = {},
): LayoutResult {
  const groups = opts.groups ?? [];
  const g = new dagre.graphlib.Graph({ compound: groups.length > 0, multigraph: true });
  g.setGraph({ rankdir: dir, nodesep: opts.nodesep ?? 50, ranksep: opts.ranksep ?? 70, marginx: 0, marginy: 0 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const gr of groups) g.setNode(gr.id, { width: 0, height: 0, paddingTop: 40 });
  for (const n of nodes) g.setNode(n.id, { width: n.w, height: n.h });
  const has = (id: string) => g.hasNode(id);
  for (const gr of groups) if (gr.parent && has(gr.parent)) g.setParent(gr.id, gr.parent);
  for (const n of nodes) if (n.parent && has(n.parent)) g.setParent(n.id, n.parent);
  edges.forEach((e, i) => {
    if (!has(e.from) || !has(e.to) || e.from === e.to) return;
    g.setEdge(e.from, e.to, { width: e.labelW ?? 0, height: e.labelH ?? 0, labelpos: 'c' }, 'e' + i);
  });
  dagre.layout(g);
  const pos = new Map<string, { x: number; y: number }>();
  for (const n of nodes) {
    const v = g.node(n.id);
    if (v && v.x !== undefined && v.y !== undefined) pos.set(n.id, { x: v.x - n.w / 2, y: v.y - n.h / 2 });
  }
  const out = new Map<string, Rect>();
  for (const gr of groups) {
    const v = g.node(gr.id);
    if (v && v.x !== undefined && v.y !== undefined && v.width) out.set(gr.id, { x: v.x - v.width / 2, y: v.y - v.height / 2, w: v.width, h: v.height });
  }
  return { pos, groups: out };
}
