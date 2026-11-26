// draw.io / diagrams.net importer: reads the first page of an .drawio file (inline or
// compressed) or a bare mxGraphModel and returns a positioned DiagramGraph.
// Has its own small XML reader so it runs without a DOM (tests, workers).
import { inflateSync, strFromU8 } from 'fflate';
import type { Dash, Head, Route } from '../types';
import type { DiagramGraph, GraphEdge, GraphGroup, GraphNode, NodeShape } from './graph';

interface XNode {
  name: string;
  attrs: Record<string, string>;
  children: XNode[];
  text: string;
}

const NOT_DRAWIO = 'Not a draw.io diagram (no mxGraphModel found)';
const XML_ENTITIES: Record<string, string> = { lt: '<', gt: '>', amp: '&', quot: '"', apos: "'" };

function decodeXml(s: string): string {
  if (!s.includes('&')) return s;
  return s.replace(/&(#x[0-9a-fA-F]+|#\d+|lt|gt|amp|quot|apos);/g, (m, e: string) => {
    if (e[0] !== '#') return XML_ENTITIES[e] ?? m;
    const code = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
    return Number.isFinite(code) && code >= 0 && code <= 0x10ffff ? String.fromCodePoint(code) : m;
  });
}

/** tolerant XML reader: elements, attributes and text; skips <? ?>, comments, doctype */
function parseXml(src: string): XNode {
  const doc: XNode = { name: '#document', attrs: {}, children: [], text: '' };
  const stack = [doc];
  const NAME = /[^\s/>=]+/y;
  const n = src.length;
  let i = 0;
  while (i < n) {
    const top = stack[stack.length - 1];
    const lt = src.indexOf('<', i);
    if (lt < 0) {
      top.text += decodeXml(src.slice(i));
      break;
    }
    if (lt > i) top.text += decodeXml(src.slice(i, lt));
    const skipTo = (end: string, from: number) => {
      const e = src.indexOf(end, from);
      return e < 0 ? n : e + end.length;
    };
    if (src.startsWith('<!--', lt)) {
      i = skipTo('-->', lt + 4);
      continue;
    }
    if (src.startsWith('<![CDATA[', lt)) {
      const e = src.indexOf(']]>', lt + 9);
      top.text += src.slice(lt + 9, e < 0 ? n : e);
      i = e < 0 ? n : e + 3;
      continue;
    }
    if (src.startsWith('<?', lt)) {
      i = skipTo('?>', lt + 2);
      continue;
    }
    if (src.startsWith('<!', lt)) {
      i = skipTo('>', lt + 2);
      continue;
    }
    if (src[lt + 1] === '/') {
      const e = src.indexOf('>', lt);
      const name = src.slice(lt + 2, e < 0 ? n : e).trim();
      for (let k = stack.length - 1; k > 0; k--)
        if (stack[k].name === name) {
          stack.length = k;
          break;
        }
      i = e < 0 ? n : e + 1;
      continue;
    }
    NAME.lastIndex = lt + 1;
    const nm = NAME.exec(src);
    if (!nm) {
      top.text += '<';
      i = lt + 1;
      continue;
    }
    const el: XNode = { name: nm[0], attrs: {}, children: [], text: '' };
    let j = lt + 1 + nm[0].length;
    let selfClose = false;
    while (j < n) {
      while (j < n && /\s/.test(src[j])) j++;
      if (src[j] === '>') {
        j++;
        break;
      }
      if (src[j] === '/' && src[j + 1] === '>') {
        selfClose = true;
        j += 2;
        break;
      }
      NAME.lastIndex = j;
      const an = NAME.exec(src);
      if (!an) {
        j++;
        continue;
      }
      j += an[0].length;
      while (j < n && /\s/.test(src[j])) j++;
      let value = '';
      if (src[j] === '=') {
        j++;
        while (j < n && /\s/.test(src[j])) j++;
        const q = src[j];
        if (q === '"' || q === "'") {
          const e = src.indexOf(q, j + 1);
          value = src.slice(j + 1, e < 0 ? n : e);
          j = e < 0 ? n : e + 1;
        } else {
          const start = j;
          while (j < n && !/[\s>]/.test(src[j]) && !(src[j] === '/' && src[j + 1] === '>')) j++;
          value = src.slice(start, j);
        }
      }
      el.attrs[an[0]] = decodeXml(value);
    }
    top.children.push(el);
    if (!selfClose) stack.push(el);
    i = j;
  }
  return doc;
}

/** first element with this name, depth first */
function find(node: XNode, name: string): XNode | undefined {
  for (const c of node.children) {
    if (c.name === name) return c;
    const f = find(c, name);
    if (f) return f;
  }
  return undefined;
}

/** a compressed <diagram> body: base64 → raw deflate → URI-encoded XML */
function unpack(text: string): string {
  try {
    const bin = atob(text.replace(/\s+/g, ''));
    const bytes = new Uint8Array(bin.length);
    for (let k = 0; k < bin.length; k++) bytes[k] = bin.charCodeAt(k);
    const raw = strFromU8(inflateSync(bytes));
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  } catch {
    throw new Error('Could not unpack the compressed draw.io diagram');
  }
}

function findModel(doc: XNode, nested = false): XNode {
  const file = find(doc, 'mxfile');
  if (file) {
    const diagram = find(file, 'diagram');
    if (!diagram) throw new Error(NOT_DRAWIO);
    const inline = find(diagram, 'mxGraphModel');
    if (inline) return inline;
    const body = diagram.text.trim();
    if (!body) throw new Error(NOT_DRAWIO);
    const model = find(parseXml(body.startsWith('<') ? body : unpack(body)), 'mxGraphModel');
    if (!model) throw new Error(NOT_DRAWIO);
    return model;
  }
  const model = find(doc, 'mxGraphModel');
  if (model) return model;
  // an editable .drawio.svg keeps the whole file in its content attribute
  const svg = find(doc, 'svg');
  if (svg?.attrs.content && !nested) return findModel(parseXml(svg.attrs.content), true);
  throw new Error(NOT_DRAWIO);
}

interface Style {
  /** first token without '=' (ellipse, rhombus, text, swimlane …) */
  name?: string;
  map: Record<string, string>;
}

function parseStyle(s: string): Style {
  const map: Record<string, string> = {};
  let name: string | undefined;
  for (const tok of s.split(';')) {
    const t = tok.trim();
    if (!t) continue;
    const eq = t.indexOf('=');
    if (eq < 0) name ??= t;
    else map[t.slice(0, eq).trim()] = t.slice(eq + 1).trim();
  }
  return { name, map };
}

interface Cell {
  id: string;
  value: string;
  style: Style;
  vertex: boolean;
  edge: boolean;
  parent?: string;
  source?: string;
  target?: string;
  geo?: XNode;
}

const ENTITIES: Record<string, string> = { nbsp: ' ', amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };

/** cell text; html=1 labels lose their tags (<br>, <div>, <p> become line breaks) */
function labelOf(value: string, style: Style): string {
  let t = value;
  if (style.map.html === '1') {
    t = t
      .replace(/<br\s*\/?>/gi, '\n')
      .replace(/<\/?(?:div|p)\b[^>]*>/gi, '\u0000')
      .replace(/<[^>]*>/g, '')
      .replace(/\u0000+/g, (_m, at: number) => (at === 0 ? '' : '\n'))
      .replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e: string) => {
        if (e[0] !== '#') return ENTITIES[e.toLowerCase()] ?? m;
        const code = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
        return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : m;
      });
  }
  return t
    .replace(/ /g, ' ')
    .split('\n')
    .map((l) => l.trim())
    .join('\n')
    .trim();
}

function num(v: string | undefined): number | undefined {
  if (v === undefined || v.trim() === '') return undefined;
  const n = Number(v);
  return Number.isFinite(n) ? n : undefined;
}

/** '#rgb' / '#rrggbb' → lowercase '#rrggbb'; anything else (none, default, light-dark…) is ignored */
function color(v: string | undefined): string | undefined {
  if (!v) return undefined;
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v.trim());
  if (!m) return undefined;
  const h = m[1].toLowerCase();
  return '#' + (h.length === 3 ? [...h].map((c) => c + c).join('') : h);
}

function dashOf(map: Record<string, string>): Dash | undefined {
  if (map.dashed !== '1') return undefined;
  const first = num(map.dashPattern?.trim().split(/\s+/)[0]);
  return first !== undefined && first <= 1 ? 'dotted' : 'dashed';
}

// Shape names by their last dotted part (mxgraph.basic.star → star).
const SHAPE_NAMES: Record<string, NodeShape> = {
  ellipse: 'ellipse',
  doubleellipse: 'ellipse',
  rhombus: 'diamond',
  decision: 'diamond',
  triangle: 'triangle',
  parallelogram: 'parallelogram',
  trapezoid: 'parallelogram',
  hexagon: 'hexagon',
  cylinder: 'cylinder',
  cylinder3: 'cylinder',
  datastore: 'cylinder',
  database: 'cylinder',
  document: 'document',
  cloud: 'cloud',
  callout: 'callout',
  star: 'star',
  text: 'text',
};

const GROUP_NAMES = new Set(['swimlane', 'group']);

function shapeOf(style: Style, label: string): Pick<GraphNode, 'shape' | 'rounded'> {
  const raw = style.map.shape ?? style.name ?? '';
  const key = raw.split('.').pop()!.toLowerCase();
  const shape = SHAPE_NAMES[key];
  if (shape) return { shape };
  if (style.map.strokeColor === 'none' && style.map.fillColor === 'none' && label) return { shape: 'text' };
  return style.map.rounded === '1' ? { shape: 'rect', rounded: true } : { shape: 'rect' };
}

function headOf(v: string | undefined, fallback: Head): Head {
  if (v === undefined) return fallback;
  switch (v) {
    case '':
    case 'none':
      return 'none';
    case 'open':
    case 'openThin':
    case 'openAsync':
      return 'open';
    case 'oval':
    case 'circle':
    case 'circlePlus':
      return 'dot';
    case 'diamond':
    case 'diamondThin':
      return 'diamond';
    case 'dash':
    case 'baseDash':
    case 'ERone':
    case 'ERmandOne':
    case 'cross':
      return 'bar';
    default:
      return 'arrow';
  }
}

const ELBOW_STYLES = new Set([
  'orthogonalEdgeStyle',
  'elbowEdgeStyle',
  'entityRelationEdgeStyle',
  'isometricEdgeStyle',
  'segmentEdgeStyle',
  'sideToSideEdgeStyle',
  'topToBottomEdgeStyle',
]);

function routeOf(map: Record<string, string>): Route {
  if (map.curved === '1') return 'curved';
  return map.edgeStyle && ELBOW_STYLES.has(map.edgeStyle) ? 'elbow' : 'straight';
}

function point(geo: XNode | undefined, as: string): { x: number; y: number } | undefined {
  const p = geo?.children.find((c) => c.name === 'mxPoint' && c.attrs.as === as);
  return p ? { x: num(p.attrs.x) ?? 0, y: num(p.attrs.y) ?? 0 } : undefined;
}

/** read the cells under <root>; wrapper <object> / <UserObject> lend their id and label */
function readCells(model: XNode): { cells: Cell[]; alias: Map<string, string> } {
  const root = find(model, 'root') ?? model;
  const cells: Cell[] = [];
  const alias = new Map<string, string>();
  for (const el of root.children) {
    let cellEl: XNode | undefined;
    let id: string | undefined;
    let value: string;
    if (el.name === 'mxCell') {
      cellEl = el;
      id = el.attrs.id;
      value = el.attrs.value ?? '';
    } else {
      cellEl = el.children.find((c) => c.name === 'mxCell');
      if (!cellEl) continue;
      id = el.attrs.id ?? cellEl.attrs.id;
      value = el.attrs.label ?? el.attrs.value ?? cellEl.attrs.value ?? '';
      if (cellEl.attrs.id && id && cellEl.attrs.id !== id) alias.set(cellEl.attrs.id, id);
    }
    if (!id) continue;
    const a = cellEl.attrs;
    cells.push({
      id,
      value,
      style: parseStyle(a.style ?? ''),
      vertex: a.vertex === '1',
      edge: a.edge === '1',
      parent: a.parent,
      source: a.source,
      target: a.target,
      geo: cellEl.children.find((c) => c.name === 'mxGeometry'),
    });
  }
  return { cells, alias };
}

/** Parse a draw.io / diagrams.net file (first page only) or a bare mxGraphModel. */
export function parseDrawio(xmlText: string): DiagramGraph {
  const model = findModel(parseXml(xmlText));
  const { cells, alias } = readCells(model);
  const resolve = (id: string | undefined) => (id === undefined ? undefined : (alias.get(id) ?? id));
  for (const c of cells) {
    c.parent = resolve(c.parent);
    c.source = resolve(c.source);
    c.target = resolve(c.target);
  }

  const byId = new Map(cells.map((c) => [c.id, c]));
  const vertexParent = (c: Cell) => {
    const p = c.parent === undefined ? undefined : byId.get(c.parent);
    return p?.vertex ? p : undefined;
  };
  const edgeParent = (c: Cell) => {
    const p = c.parent === undefined ? undefined : byId.get(c.parent);
    return p?.edge ? p : undefined;
  };
  const hasChildren = new Set<string>();
  for (const c of cells) {
    const p = c.vertex ? vertexParent(c) : undefined;
    if (p) hasChildren.add(p.id);
  }

  // absolute position: geometry is relative to a parent vertex (containers, swimlanes)
  const absCache = new Map<string, { x: number; y: number }>();
  const busy = new Set<string>();
  const abs = (c: Cell): { x: number; y: number } => {
    const hit = absCache.get(c.id);
    if (hit) return hit;
    const g = c.geo?.attrs ?? {};
    let x = num(g.x) ?? 0;
    let y = num(g.y) ?? 0;
    const p = vertexParent(c);
    if (p && !busy.has(c.id)) {
      busy.add(c.id);
      const pa = abs(p);
      busy.delete(c.id);
      if (g.relative === '1') {
        // fractions of the parent's size, plus an offset
        const off = point(c.geo, 'offset');
        x = x * (num(p.geo?.attrs.width) ?? 0) + (off?.x ?? 0);
        y = y * (num(p.geo?.attrs.height) ?? 0) + (off?.y ?? 0);
      }
      x += pa.x;
      y += pa.y;
    }
    const r = { x, y };
    absCache.set(c.id, r);
    return r;
  };

  const nodes: GraphNode[] = [];
  const groups: GraphGroup[] = [];
  const edgeLabels = new Map<string, string>();

  for (const c of cells) {
    if (!c.vertex) continue;
    const label = labelOf(c.value, c.style);
    const owner = edgeParent(c);
    if (owner) {
      // a label sitting on an edge
      if (label && !edgeLabels.has(owner.id)) edgeLabels.set(owner.id, label);
      continue;
    }
    const pos = abs(c);
    const g = c.geo?.attrs ?? {};
    const w = num(g.width);
    const h = num(g.height);
    const parent = vertexParent(c)?.id;
    const shapeName = (c.style.map.shape ?? c.style.name ?? '').toLowerCase();
    if (GROUP_NAMES.has(shapeName) || hasChildren.has(c.id)) {
      const grp: GraphGroup = { id: c.id, label, x: pos.x, y: pos.y };
      if (w !== undefined) grp.w = w;
      if (h !== undefined) grp.h = h;
      if (parent) grp.parent = parent;
      groups.push(grp);
      continue;
    }
    const m = c.style.map;
    const node: GraphNode = { id: c.id, label, ...shapeOf(c.style, label), x: pos.x, y: pos.y };
    if (w !== undefined) node.w = w;
    if (h !== undefined) node.h = h;
    if (parent) node.parent = parent;
    const fill = color(m.fillColor);
    const stroke = color(m.strokeColor);
    const fontColor = color(m.fontColor);
    const strokeWidth = num(m.strokeWidth);
    const fontSize = num(m.fontSize);
    const dash = dashOf(m);
    if (fill) node.fill = fill;
    if (stroke) node.stroke = stroke;
    if (fontColor) node.fontColor = fontColor;
    if (strokeWidth !== undefined) node.strokeWidth = strokeWidth;
    if (dash) node.dash = dash;
    if (fontSize !== undefined) node.fontSize = fontSize;
    nodes.push(node);
  }

  const known = new Set([...nodes.map((n) => n.id), ...groups.map((g) => g.id)]);
  const edges: GraphEdge[] = [];
  for (const c of cells) {
    if (!c.edge) continue;
    const m = c.style.map;
    const base = vertexParent(c) ? abs(vertexParent(c)!) : { x: 0, y: 0 };
    const shift = (p: { x: number; y: number } | undefined) => (p ? { x: p.x + base.x, y: p.y + base.y } : undefined);
    const e: GraphEdge = { id: c.id };
    if (c.source && known.has(c.source)) e.from = c.source;
    else {
      const p = shift(point(c.geo, 'sourcePoint'));
      if (!p) continue;
      e.fromPoint = p;
    }
    if (c.target && known.has(c.target)) e.to = c.target;
    else {
      const p = shift(point(c.geo, 'targetPoint'));
      if (!p) continue;
      e.toPoint = p;
    }
    const label = labelOf(c.value, c.style) || edgeLabels.get(c.id);
    if (label) e.label = label;
    e.start = headOf(m.startArrow, 'none');
    e.end = headOf(m.endArrow, 'arrow');
    const dash = dashOf(m);
    if (dash) e.dash = dash;
    const width = num(m.strokeWidth);
    if (width !== undefined) e.width = width;
    const stroke = color(m.strokeColor);
    if (stroke) e.stroke = stroke;
    e.route = routeOf(m);
    edges.push(e);
  }

  const positioned = nodes.every((n) => n.x !== undefined && n.y !== undefined && n.w !== undefined && n.h !== undefined);
  return { direction: 'TB', nodes, edges, groups, positioned };
}
