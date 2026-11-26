// Diagram templates for the canvas: flowchart, mind map, org chart, timeline and a blank
// frame. They are ordinary shapes and connectors (fully editable), at (0, 0).
import { newId } from '../api';
import type { ArrowItem, CanvasItem, NoteItem, Rect, ShapeItem } from '../types';
import { buildDiagram } from './build';
import type { DiagramGraph } from './graph';
import { parseMermaid } from './mermaid';

export type DiagramTemplateId = 'flowchart' | 'mindmap' | 'orgchart' | 'timeline' | 'blank';

export const DIAGRAM_TEMPLATES: { id: DiagramTemplateId; name: string; icon: string; description: string }[] = [
  { id: 'flowchart', name: 'Flowchart', icon: 'activity', description: 'Start, steps, a decision and a loop back.' },
  { id: 'mindmap', name: 'Mind map', icon: 'share', description: 'A main idea with branches (Tab adds more).' },
  { id: 'orgchart', name: 'Org chart', icon: 'users', description: 'A team in levels, joined by elbow lines.' },
  { id: 'timeline', name: 'Timeline', icon: 'calendar', description: 'Milestones along a line.' },
  { id: 'blank', name: 'Blank frame', icon: 'frame', description: 'An empty 16:9 frame to draw in.' },
];

const FLOW = `flowchart TD
  start([Start]) --> input[/Get what you need/]
  input --> work[Do the work]
  work --> check{All good?}
  check -->|Yes| done([Done])
  check -->|No| fix[Fix it]
  fix --> work`;

function orgGraph(): DiagramGraph {
  const n = (id: string, label: string, fill: string) => ({ id, label, shape: 'rect' as const, rounded: true, fill, stroke: fill });
  const nodes = [
    n('ceo', 'Director\nName', 'purple'),
    n('a', 'Team lead\nName', 'blue'),
    n('b', 'Team lead\nName', 'blue'),
    n('c', 'Team lead\nName', 'blue'),
    n('a1', 'Name', 'teal'),
    n('a2', 'Name', 'teal'),
    n('b1', 'Name', 'teal'),
    n('b2', 'Name', 'teal'),
    n('c1', 'Name', 'teal'),
  ];
  const e = (from: string, to: string) => ({ from, to, start: 'none' as const, end: 'none' as const });
  return {
    direction: 'TB',
    nodes,
    edges: [e('ceo', 'a'), e('ceo', 'b'), e('ceo', 'c'), e('a', 'a1'), e('a', 'a2'), e('b', 'b1'), e('b', 'b2'), e('c', 'c1')],
    groups: [],
  };
}

function shape(r: Rect, extra: Partial<ShapeItem>): ShapeItem {
  return { id: newId(), type: 'rect', ...r, color: 'ink', strokeWidth: 2, size: 22, ...extra };
}
function link(from: ShapeItem, to: ShapeItem, extra: Partial<ArrowItem> = {}): ArrowItem {
  return {
    id: newId(),
    type: 'arrow',
    from: { x: from.x + from.w / 2, y: from.y + from.h / 2, id: from.id },
    to: { x: to.x + to.w / 2, y: to.y + to.h / 2, id: to.id },
    color: 'ink',
    start: 'none',
    end: 'none',
    ...extra,
  };
}

function mindmap(): CanvasItem[] {
  const items: CanvasItem[] = [];
  const centre = shape({ x: 620, y: 330, w: 280, h: 130 }, { type: 'ellipse', text: 'Main idea', size: 30, color: 'purple', fillColor: 'purple', strokeWidth: 3 });
  items.push(centre);
  const colors = ['orange', 'green', 'blue', 'pink'];
  const branches = [
    { x: 1060, y: 160, left: false },
    { x: 1060, y: 500, left: false },
    { x: 220, y: 160, left: true },
    { x: 220, y: 500, left: true },
  ];
  branches.forEach((b, i) => {
    const c = colors[i];
    const br = shape({ x: b.x, y: b.y, w: 240, h: 72 }, { text: `Branch ${i + 1}`, radius: 36, color: c, fillColor: c });
    items.push(br, link(centre, br, { color: c, width: 4, route: 'curved', from: { x: 0, y: 0, id: centre.id, side: b.left ? 'left' : 'right' }, to: { x: 0, y: 0, id: br.id, side: b.left ? 'right' : 'left' } }));
    for (let k = 0; k < 2; k++) {
      const sx = b.left ? b.x - 300 : b.x + 320;
      const leaf = shape({ x: sx, y: b.y - 50 + k * 110, w: 200, h: 60 }, { text: 'Idea', radius: 30, color: c, size: 20 });
      items.push(
        leaf,
        link(br, leaf, { color: c, width: 2, route: 'curved', from: { x: 0, y: 0, id: br.id, side: b.left ? 'left' : 'right' }, to: { x: 0, y: 0, id: leaf.id, side: b.left ? 'right' : 'left' } }),
      );
    }
  });
  return items;
}

function timeline(): CanvasItem[] {
  const items: CanvasItem[] = [];
  const y = 300;
  const title: NoteItem = { id: newId(), type: 'note', x: 0, y: 0, w: 600, h: 60, text: 'Timeline', color: 'none', size: 44, look: { font: 'inter', weight: 700 } };
  items.push(title);
  items.push({ id: newId(), type: 'arrow', from: { x: 0, y }, to: { x: 1700, y }, color: 'ink', start: 'none', end: 'arrow', width: 4 });
  const colors = ['blue', 'teal', 'green', 'orange', 'red'];
  for (let i = 0; i < 5; i++) {
    const cx = 160 + i * 340;
    const dot = shape({ x: cx - 16, y: y - 16, w: 32, h: 32 }, { type: 'ellipse', color: colors[i], fillColor: colors[i], strokeWidth: 3 });
    const up = i % 2 === 0;
    const card = shape({ x: cx - 130, y: up ? y - 230 : y + 110, w: 260, h: 120 }, { text: `Milestone ${i + 1}\nDate`, radius: 14, color: colors[i], fillColor: colors[i], size: 20 });
    items.push(dot, card, link(dot, card, { color: colors[i], width: 2, dash: 'dotted' }));
  }
  return items;
}

/** Items of a template with their top-left at (0, 0) and their bounds. */
export function diagramTemplate(id: DiagramTemplateId): { items: CanvasItem[]; bounds: Rect } {
  if (id === 'flowchart') return buildDiagram(parseMermaid(FLOW), { stroke: 'blue', fill: 'blue', route: 'elbow' });
  if (id === 'orgchart') return buildDiagram(orgGraph(), { route: 'elbow', ranksep: 80 });
  if (id === 'blank') return { items: [{ id: newId(), type: 'frame', x: 0, y: 0, w: 1600, h: 900, title: 'Diagram' }], bounds: { x: 0, y: 0, w: 1600, h: 900 } };
  const items = id === 'mindmap' ? mindmap() : timeline();
  const rs = items.filter((i) => i.type !== 'arrow') as Rect[];
  const x1 = Math.max(...rs.map((r) => r.x + r.w)),
    y1 = Math.max(...rs.map((r) => r.y + r.h));
  return { items, bounds: { x: 0, y: 0, w: x1, h: y1 } };
}
