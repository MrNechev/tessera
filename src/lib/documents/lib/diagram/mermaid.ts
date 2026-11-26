// Mermaid flowchart importer: turns the text of a `flowchart` / `graph` diagram into a
// DiagramGraph. Only the flowchart subset is understood (shapes, links, subgraphs);
// styling statements (classDef, style, linkStyle, click …) are skipped.
import type { Dash, Head } from '../types';
import type { DiagramGraph, Direction, GraphEdge, GraphGroup, GraphNode, NodeShape } from './graph';

interface ShapeDef {
  shape: NodeShape;
  rounded?: boolean;
  pill?: boolean;
}

interface Link {
  start: Head;
  end: Head;
  dash?: Dash;
  width?: number;
  label?: string;
  /** `~~~` only spaces nodes out in Mermaid: no edge */
  hidden?: boolean;
}

interface Cursor {
  s: string;
  i: number;
}

const NOT_FLOWCHART = 'Not a Mermaid flowchart (it should start with “flowchart TD” or “graph LR”)';
const HEADER = /^(?:graph|flowchart(?:-elk)?)(?:\s+(TD|TB|LR|RL|BT))?$/i;
// Letters, digits and _; single dashes may join parts (node-1) but never start a link (A-->B).
const ID = /[\p{L}\p{N}_]+(?:-[\p{L}\p{N}_]+)*/uy;
const SKIPPED = /^(?:classDef|class|style|linkStyle|click|direction|accTitle|accDescr)(?=[\s:{]|$)/;

const RECT: ShapeDef = { shape: 'rect' };
// Bracket pairs, longest opener first so `((` wins over `(`.
const SHAPES: [string, string[], ShapeDef][] = [
  ['(((', [')))'], { shape: 'ellipse' }],
  ['((', ['))'], { shape: 'ellipse' }],
  ['([', ['])'], { shape: 'rect', pill: true }],
  ['(', [')'], { shape: 'rect', rounded: true }],
  ['[[', [']]'], RECT],
  ['[(', [')]'], { shape: 'cylinder' }],
  ['[/', ['/]', '\\]'], { shape: 'parallelogram' }],
  ['[\\', ['\\]', '/]'], { shape: 'parallelogram' }],
  ['[', [']'], RECT],
  ['{{', ['}}'], { shape: 'hexagon' }],
  ['{', ['}'], { shape: 'diamond' }],
  ['>', [']'], { shape: 'callout' }],
];

// Names used by the newer `A@{ shape: … }` syntax.
const AT_SHAPES: Record<string, ShapeDef> = {
  rect: RECT,
  rectangle: RECT,
  proc: RECT,
  process: RECT,
  rounded: { shape: 'rect', rounded: true },
  event: { shape: 'rect', rounded: true },
  stadium: { shape: 'rect', pill: true },
  pill: { shape: 'rect', pill: true },
  terminal: { shape: 'rect', pill: true },
  circle: { shape: 'ellipse' },
  circ: { shape: 'ellipse' },
  'sm-circ': { shape: 'ellipse' },
  'small-circle': { shape: 'ellipse' },
  'dbl-circ': { shape: 'ellipse' },
  'double-circle': { shape: 'ellipse' },
  diam: { shape: 'diamond' },
  diamond: { shape: 'diamond' },
  decision: { shape: 'diamond' },
  question: { shape: 'diamond' },
  hex: { shape: 'hexagon' },
  hexagon: { shape: 'hexagon' },
  prepare: { shape: 'hexagon' },
  cyl: { shape: 'cylinder' },
  cylinder: { shape: 'cylinder' },
  database: { shape: 'cylinder' },
  db: { shape: 'cylinder' },
  doc: { shape: 'document' },
  document: { shape: 'document' },
  cloud: { shape: 'cloud' },
  tri: { shape: 'triangle' },
  triangle: { shape: 'triangle' },
  'lean-r': { shape: 'parallelogram' },
  'lean-l': { shape: 'parallelogram' },
  'in-out': { shape: 'parallelogram' },
  'out-in': { shape: 'parallelogram' },
  'trap-t': { shape: 'parallelogram' },
  'trap-b': { shape: 'parallelogram' },
  odd: { shape: 'callout' },
  text: { shape: 'text' },
};

const ENTITIES: Record<string, string> = { quot: '"', amp: '&', lt: '<', gt: '>', apos: "'", nbsp: ' ' };

function decodeEntity(m: string, name: string): string {
  if (name[0] === '#') {
    const code = name[1] === 'x' || name[1] === 'X' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
    return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : m;
  }
  return ENTITIES[name.toLowerCase()] ?? m;
}

/** label text: quotes stripped, <br> → newline, #quot; / &amp; style entities decoded */
function decodeLabel(raw: string): string {
  let t = raw.trim();
  if (t.length >= 2 && t.startsWith('"') && t.endsWith('"')) t = t.slice(1, -1).trim();
  // markdown strings "`…`"
  if (t.length >= 2 && t.startsWith('`') && t.endsWith('`')) t = t.slice(1, -1);
  t = t.replace(/<br\s*\/?>/gi, '\n');
  // the HTML entity form (&amp; &#35;) and Mermaid's own (#quot; #35;)
  t = t.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);|#(\d+|[a-z]+);/gi, (m, html: string | undefined, own: string | undefined) =>
    html ? decodeEntity(m, html) : decodeEntity(m, /^\d/.test(own!) ? '#' + own : own!),
  );
  return t
    .split('\n')
    .map((l) => l.trim())
    .join('\n')
    .trim();
}

/** split one line on `;` that are not inside quotes, brackets or |edge labels| */
function splitStatements(line: string): string[] {
  const out: string[] = [];
  let quote = false;
  let pipe = false;
  let depth = 0;
  let from = 0;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '"') quote = !quote;
    else if (quote) continue;
    else if (ch === '|' && depth === 0) pipe = !pipe;
    else if (pipe) continue;
    else if (ch === '[' || ch === '(' || ch === '{') depth++;
    else if (ch === ']' || ch === ')' || ch === '}') depth = Math.max(0, depth - 1);
    else if (ch === ';' && depth === 0) {
      out.push(line.slice(from, i));
      from = i + 1;
    }
  }
  out.push(line.slice(from));
  return out;
}

function statements(text: string): string[] {
  let lines = text.replace(/\r\n?/g, '\n').split('\n');
  // YAML front matter (--- title: … ---)
  const first = lines.findIndex((l) => l.trim() !== '');
  if (first >= 0 && lines[first].trim() === '---') {
    const close = lines.findIndex((l, i) => i > first && l.trim() === '---');
    if (close > 0) lines = lines.slice(close + 1);
  }
  const out: string[] = [];
  for (const line of lines) {
    if (line.trim().startsWith('%%')) continue;
    for (const s of splitStatements(line)) if (s.trim()) out.push(s.trim());
  }
  return out;
}

function skipWs(c: Cursor) {
  while (c.i < c.s.length && /\s/.test(c.s[c.i])) c.i++;
}

/** the label inside a shape opener; moves the cursor past the closer */
function readShapeText(c: Cursor, closes: string[]): string | null {
  const { s } = c;
  let j = c.i;
  while (j < s.length && s[j] === ' ') j++;
  // quoted text may hold brackets: A["f(x)"]
  if (s[j] === '"') {
    const q = s.indexOf('"', j + 1);
    if (q > 0) {
      let k = q + 1;
      while (k < s.length && s[k] === ' ') k++;
      const close = closes.find((cl) => s.startsWith(cl, k));
      if (close) {
        c.i = k + close.length;
        return s.slice(j, q + 1);
      }
    }
  }
  let best = -1;
  let bestLen = 0;
  for (const cl of closes) {
    const at = s.indexOf(cl, c.i);
    if (at >= 0 && (best < 0 || at < best)) {
      best = at;
      bestLen = cl.length;
    }
  }
  if (best < 0) return null;
  const text = s.slice(c.i, best);
  c.i = best + bestLen;
  return text;
}

/** `@{ shape: …, label: "…" }` metadata of the newer syntax */
function readAtBlock(c: Cursor): { def?: ShapeDef; label?: string } {
  const { s } = c;
  let j = c.i + 2;
  let quote = false;
  while (j < s.length && (quote || s[j] !== '}')) {
    if (s[j] === '"') quote = !quote;
    j++;
  }
  const body = s.slice(c.i + 2, j);
  c.i = Math.min(j + 1, s.length);
  const shape = /\bshape\s*:\s*"?([\w-]+)/.exec(body)?.[1]?.toLowerCase();
  const label = /\blabel\s*:\s*(?:"([^"]*)"|([^,}]+))/.exec(body);
  return { def: shape ? AT_SHAPES[shape] : undefined, label: label ? decodeLabel(label[1] ?? label[2]) : undefined };
}

const HEADS: Record<string, Head> = { '<': 'arrow', '>': 'arrow', o: 'dot', x: 'bar' };

function headOf(ch: string | undefined): Head {
  return (ch && HEADS[ch]) || 'none';
}

/** a link at the cursor (--> --- -.-> ==> -- text --> …) with an optional |label| */
function readLink(c: Cursor): Link | null {
  skipWs(c);
  const rest = c.s.slice(c.i);
  let link: Link | null = null;
  let m: RegExpExecArray | null;
  if ((m = /^~{3,}/.exec(rest))) {
    c.i += m[0].length;
    link = { start: 'none', end: 'none', hidden: true };
  } else if ((m = /^([<ox])?-(\.+)-([>ox])?/.exec(rest))) {
    c.i += m[0].length;
    link = { start: headOf(m[1]), end: headOf(m[3]), dash: 'dotted' };
  } else if ((m = /^([<ox])?-\./.exec(rest))) {
    // dotted with text: -. label .->
    const t = /\.+-([>ox])?/.exec(rest.slice(m[0].length));
    if (!t) return null;
    link = { start: headOf(m[1]), end: headOf(t[1]), dash: 'dotted', label: decodeLabel(rest.slice(m[0].length, m[0].length + t.index)) };
    c.i += m[0].length + t.index + t[0].length;
  } else if ((m = /^([<ox])?(-{2,}|={2,})([>ox])?/.exec(rest))) {
    const thick = m[2][0] === '=';
    if (m[3] || m[2].length > 2) {
      c.i += m[0].length;
      link = { start: headOf(m[1]), end: headOf(m[3]) };
    } else {
      // two dashes / equals then text: -- label --> / == label ==>
      const t = (thick ? /={2,}[>ox]|={3,}/ : /-{2,}[>ox]|-{3,}/).exec(rest.slice(m[0].length));
      if (!t) return null;
      const last = t[0][t[0].length - 1];
      link = { start: headOf(m[1]), end: headOf(last), label: decodeLabel(rest.slice(m[0].length, m[0].length + t.index)) };
      c.i += m[0].length + t.index + t[0].length;
    }
    if (thick) link.width = 5;
  }
  if (!link) return null;
  // |label| after the link
  const p = /^\s*\|([^|]*)\|/.exec(c.s.slice(c.i));
  if (p) {
    link.label = decodeLabel(p[1]);
    c.i += p[0].length;
  }
  if (link.label === '') delete link.label;
  return link;
}

/**
 * Parse a Mermaid flowchart. Nodes get the shape of their first definition with
 * brackets (a bare later mention keeps it); a node belongs to the first subgraph that
 * mentions it. Edges to a subgraph id are dropped.
 */
export function parseMermaid(text: string): DiagramGraph {
  const list = statements(text);
  const header = list.length ? HEADER.exec(list[0]) : null;
  if (!header) throw new Error(NOT_FLOWCHART);
  const dir = (header[1] ?? 'TB').toUpperCase();
  const direction: Direction = dir === 'TD' ? 'TB' : (dir as Direction);

  const nodes = new Map<string, GraphNode>();
  const edges: GraphEdge[] = [];
  const groups: GraphGroup[] = [];
  const stack: string[] = [];

  const touch = (id: string, def?: ShapeDef, label?: string) => {
    let n = nodes.get(id);
    if (!n) {
      n = { id, label: id, shape: 'rect' };
      nodes.set(id, n);
    }
    if (stack.length && !n.parent) n.parent = stack[stack.length - 1];
    if (def) {
      n.shape = def.shape;
      delete n.rounded;
      delete n.pill;
      if (def.rounded) n.rounded = true;
      if (def.pill) n.pill = true;
    }
    if (label !== undefined) n.label = label;
  };

  // one node with its optional shape, @{…} block and :::class
  const readNode = (c: Cursor): string | null => {
    skipWs(c);
    ID.lastIndex = c.i;
    const m = ID.exec(c.s);
    if (!m) return null;
    const id = m[0];
    c.i += id.length;
    let def: ShapeDef | undefined;
    let label: string | undefined;
    if (c.s.startsWith('@{', c.i)) {
      ({ def, label } = readAtBlock(c));
    } else {
      // shape brackets, allowing spaces before them (but `A >` is never a callout)
      let j = c.i;
      while (j < c.s.length && c.s[j] === ' ') j++;
      for (const [open, closes, shape] of SHAPES) {
        if (!c.s.startsWith(open, j) || (open === '>' && j !== c.i)) continue;
        const save = c.i;
        c.i = j + open.length;
        const t = readShapeText(c, closes);
        if (t === null) {
          c.i = save;
          continue;
        }
        def = shape;
        label = decodeLabel(t);
        break;
      }
    }
    const cls = /^:::[\w-]+/.exec(c.s.slice(c.i));
    if (cls) c.i += cls[0].length;
    touch(id, def, label);
    return id;
  };

  // A & B & C
  const readGroup = (c: Cursor): string[] | null => {
    const first = readNode(c);
    if (!first) return null;
    const ids = [first];
    for (;;) {
      const save = c.i;
      skipWs(c);
      if (c.s[c.i] !== '&') {
        c.i = save;
        return ids;
      }
      c.i++;
      const next = readNode(c);
      if (!next) return ids;
      ids.push(next);
    }
  };

  const readChain = (s: string) => {
    const c: Cursor = { s, i: 0 };
    let from = readGroup(c);
    while (from) {
      const link = readLink(c);
      if (!link) return;
      const to = readGroup(c);
      if (!to) return;
      if (!link.hidden) {
        for (const a of from)
          for (const b of to) {
            const e: GraphEdge = { from: a, to: b, start: link.start, end: link.end };
            if (link.label) e.label = link.label;
            if (link.dash) e.dash = link.dash;
            if (link.width) e.width = link.width;
            edges.push(e);
          }
      }
      from = to;
    }
  };

  for (const s of list.slice(1)) {
    if (s === 'end') {
      stack.pop();
      continue;
    }
    if (/^subgraph(?:\s|$)/.test(s)) {
      const rest = s.slice(8).trim();
      let id = '';
      let label = '';
      const m = /^([\p{L}\p{N}_-]+)\s*\[(.*)\]$/u.exec(rest);
      if (m) {
        id = m[1];
        label = decodeLabel(m[2]);
      } else if (/^[\p{L}\p{N}_-]+$/u.test(rest)) {
        id = label = rest;
      } else {
        label = decodeLabel(rest);
      }
      if (!id) id = `subgraph-${groups.length + 1}`;
      // ids stay unique
      const base = id;
      for (let k = 2; groups.some((g) => g.id === id); k++) id = `${base}-${k}`;
      const g: GraphGroup = { id, label };
      if (stack.length) g.parent = stack[stack.length - 1];
      groups.push(g);
      stack.push(id);
      continue;
    }
    if (SKIPPED.test(s)) continue;
    readChain(s);
  }

  // subgraph ids mentioned like nodes are not nodes
  const groupIds = new Set(groups.map((g) => g.id));
  return {
    direction,
    nodes: [...nodes.values()].filter((n) => !groupIds.has(n.id)),
    edges: edges.filter((e) => !groupIds.has(e.from!) && !groupIds.has(e.to!)),
    groups,
  };
}
