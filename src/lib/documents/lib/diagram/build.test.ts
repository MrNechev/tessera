import { describe, expect, it } from 'vitest';
import type { ArrowItem, FrameItem, ShapeItem } from '../types';
import { buildDiagram, nodeSize } from './build';
import { parseMermaid } from './mermaid';
import { layered } from './layout';
import { diagramTemplate } from './templates';
import { copyStyle, luminance, pasteStyle, shapeTextColor } from './style';

describe('layered layout', () => {
  it('puts a chain in ranks top-down and left-right', () => {
    const nodes = ['a', 'b', 'c'].map((id) => ({ id, w: 100, h: 50 }));
    const edges = [
      { from: 'a', to: 'b' },
      { from: 'b', to: 'c' },
    ];
    const tb = layered(nodes, edges, 'TB');
    expect(tb.pos.get('a')!.y).toBeLessThan(tb.pos.get('b')!.y);
    expect(tb.pos.get('b')!.y).toBeLessThan(tb.pos.get('c')!.y);
    const lr = layered(nodes, edges, 'LR');
    expect(lr.pos.get('a')!.x).toBeLessThan(lr.pos.get('b')!.x);
  });
});

describe('buildDiagram', () => {
  it('turns a Mermaid flowchart into shapes, connectors and frames', () => {
    const g = parseMermaid('flowchart TD\n A([Start]) --> B{Ok?}\n B -->|yes| C[(DB)]\n subgraph S [Store]\n C\n end');
    const { items, bounds } = buildDiagram(g, { fill: 'purple' });
    const shapes = items.filter((i) => i.type === 'rect' || i.type === 'ellipse') as ShapeItem[];
    const arrows = items.filter((i) => i.type === 'arrow') as ArrowItem[];
    const frames = items.filter((i) => i.type === 'frame') as FrameItem[];
    expect(shapes.map((s) => s.text)).toEqual(['Start', 'Ok?', 'DB']);
    expect(shapes[1].shape).toBe('diamond');
    expect(shapes[2].shape).toBe('cylinder');
    expect(shapes[0].radius).toBe(Math.round(shapes[0].h / 2)); // stadium
    expect(arrows).toHaveLength(2);
    expect(arrows[1].text).toBe('yes');
    expect(arrows[0].from.id).toBe(shapes[0].id);
    expect(arrows[0].to.id).toBe(shapes[1].id);
    expect(arrows[0].route).toBe('elbow');
    expect(frames).toHaveLength(1);
    expect(frames[0].title).toBe('Store');
    // the frame holds its node
    const c = shapes[2];
    expect(c.x).toBeGreaterThanOrEqual(frames[0].x);
    expect(c.x + c.w).toBeLessThanOrEqual(frames[0].x + frames[0].w);
    // top-down: B below A; everything from (0, 0)
    expect(shapes[1].y).toBeGreaterThan(shapes[0].y);
    expect(Math.min(...shapes.map((s) => s.x), frames[0].x)).toBeGreaterThanOrEqual(0);
    expect(bounds.w).toBeGreaterThan(0);
  });
  it('keeps positions of positioned graphs (scaled)', () => {
    const { items } = buildDiagram(
      {
        direction: 'TB',
        positioned: true,
        nodes: [
          { id: 'a', label: 'A', shape: 'rect', x: 10, y: 10, w: 100, h: 40, fill: '#dae8fc' },
          { id: 'b', label: 'B', shape: 'text', x: 200, y: 10, w: 50, h: 20 },
        ],
        edges: [{ from: 'a', to: 'b', end: 'open' }, { from: 'a', toPoint: { x: 50, y: 200 } }],
        groups: [],
      },
      { scale: 2 },
    );
    const a = items.find((i) => i.type === 'rect') as ShapeItem;
    expect(a.w).toBe(200);
    expect(a.fillColor).toBe('#dae8fc');
    expect(items.find((i) => i.type === 'note')).toBeTruthy();
    const arrows = items.filter((i) => i.type === 'arrow') as ArrowItem[];
    expect(arrows[0].end).toBe('open');
    expect(arrows[1].to.id).toBeUndefined();
  });
  it('sizes nodes to their labels', () => {
    const s = nodeSize({ label: 'short', shape: 'rect' });
    const l = nodeSize({ label: 'a much longer label that needs a wider box', shape: 'rect' });
    expect(l.w).toBeGreaterThan(s.w);
    expect(nodeSize({ label: 'x', shape: 'diamond' }).w).toBeGreaterThan(s.w);
  });
});

describe('templates', () => {
  it('builds every template', () => {
    for (const id of ['flowchart', 'mindmap', 'orgchart', 'timeline', 'blank'] as const) {
      const t = diagramTemplate(id);
      expect(t.items.length).toBeGreaterThan(0);
      expect(t.bounds.w).toBeGreaterThan(0);
      const ids = new Set(t.items.map((i) => i.id));
      for (const a of t.items) if (a.type === 'arrow') for (const e of [a.from, a.to]) if (e.id) expect(ids.has(e.id)).toBe(true);
    }
  });
});

describe('style', () => {
  it('chooses readable text on fills', () => {
    expect(luminance('#ffffff')).toBeCloseTo(1);
    expect(shapeTextColor({ color: 'ink', fillColor: '#1e3a5f' })).toBe('#ffffff');
    expect(shapeTextColor({ color: 'ink', fillColor: 'yellow' })).toBe('#1d1b22');
    expect(shapeTextColor({ color: 'ink' })).toBe('var(--text)');
  });
  it('copies a shape style onto an arrow and a note', () => {
    const sh: ShapeItem = { id: 's', type: 'rect', x: 0, y: 0, w: 10, h: 10, color: 'blue', fillColor: 'green', strokeWidth: 5, dash: 'dotted', opacity: 0.5 };
    const s = copyStyle(sh)!;
    const a: ArrowItem = { id: 'a', type: 'arrow', from: { x: 0, y: 0 }, to: { x: 1, y: 1 }, color: 'ink', dashed: true };
    pasteStyle(a, s);
    expect(a.color).toBe('blue');
    expect(a.width).toBe(5);
    expect(a.dash).toBe('dotted');
    expect(a.dashed).toBeUndefined();
    expect(a.opacity).toBe(0.5);
    const n = { id: 'n', type: 'note' as const, x: 0, y: 0, w: 1, h: 1, text: '', color: 'yellow', size: 24 };
    pasteStyle(n, s);
    expect(n.color).toBe('green');
  });
});
