import { describe, expect, it } from 'vitest';
import { replaySvg, SvgPath } from '../../../render/pathsink';
import { autoSide, headsOf, nearestSide, nearestT, pointAt, polyLength, roundedPath, routeElbow, simplifyOrtho, type Pt } from './connectors';
import { splitStroke, strokeHit } from './eraser';
import { guideLines, snapEdge, snapGuides } from './guides';
import { outline, outlineExit, outlinePath, rayExit, textBox } from './shapes';
import type { Rect } from '../types';

const inside = (p: Pt, r: Rect) => p.x > r.x + 0.01 && p.x < r.x + r.w - 0.01 && p.y > r.y + 0.01 && p.y < r.y + r.h - 0.01;
/** does the segment a-b (axis-aligned) pass through the inside of r? */
function crosses(a: Pt, b: Pt, r: Rect) {
  for (let i = 1; i < 20; i++) {
    const t = i / 20;
    if (inside({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }, r)) return true;
  }
  return false;
}
const ortho = (pts: Pt[]) => pts.every((p, i) => !i || Math.abs(p.x - pts[i - 1].x) < 0.01 || Math.abs(p.y - pts[i - 1].y) < 0.01);

describe('SvgPath', () => {
  it('builds path data and a flattened outline', () => {
    const p = new SvgPath();
    p.moveTo(0, 0);
    p.lineTo(10, 0);
    p.lineTo(10, 10);
    p.closePath();
    expect(p.toString()).toBe('M0 0L10 0L10 10Z');
    expect(p.outline).toEqual([
      [0, 0],
      [10, 0],
      [10, 10],
    ]);
  });
  it('turns arcTo into an SVG arc with the right sweep', () => {
    const p = new SvgPath();
    p.moveTo(0, 0);
    p.lineTo(8, 0);
    p.arcTo(10, 0, 10, 2, 2);
    expect(p.toString()).toMatch(/A2 2 0 0 1 10 2$/);
  });
  it('draws a full ellipse as arcs that come back to the start', () => {
    const p = new SvgPath();
    p.ellipse(50, 50, 50, 20, 0, 0, Math.PI * 2);
    const pts = p.outline;
    expect(pts[0][0]).toBeCloseTo(100);
    expect(pts[pts.length - 1][0]).toBeCloseTo(100);
    expect(Math.min(...pts.map((q) => q[1]))).toBeCloseTo(30, 0);
  });
  it('replays authored outlines scaled', () => {
    const p = new SvgPath();
    replaySvg(p, 'M0 0 L100 0 C100 50 50 100 0 100 Z', 10, 20, 2, 0.5);
    expect(p.toString()).toBe('M10 20L210 20C210 45 110 70 10 70Z');
  });
});

describe('diagram shapes', () => {
  it('outlines stay inside their box', () => {
    for (const k of ['rect', 'ellipse', 'diamond', 'triangle', 'parallelogram', 'hexagon', 'cylinder', 'document', 'cloud', 'callout', 'star'] as const) {
      const pts = outline(k, 200, 120).outline;
      expect(pts.length).toBeGreaterThan(2);
      for (const [x, y] of pts) {
        expect(x).toBeGreaterThanOrEqual(-0.5);
        expect(x).toBeLessThanOrEqual(200.5);
        expect(y).toBeGreaterThanOrEqual(-0.5);
        expect(y).toBeLessThanOrEqual(120.5 + 30); // the document's wave may dip a little
      }
      expect(outlinePath(k, 200, 120, 3)).toMatch(/^M/);
      const tb = textBox(k, 200, 120);
      expect(tb.x).toBeGreaterThanOrEqual(0);
      expect(tb.x + tb.w).toBeLessThanOrEqual(200);
    }
  });
  it('rays leave a diamond on its outline', () => {
    const r = { x: 0, y: 0, w: 200, h: 100 };
    expect(outlineExit('diamond', r, 1, 0)).toEqual({ x: 200, y: 50 });
    const q = outlineExit('diamond', r, 0, -1);
    expect(q.x).toBeCloseTo(100);
    expect(q.y).toBeCloseTo(0);
    // diagonal: on the edge from (100,0) to (200,50)
    const d = outlineExit('diamond', r, Math.SQRT1_2, -Math.SQRT1_2);
    expect(d.x / 2 - d.y).toBeCloseTo(50, 5); // the edge (100, 0)–(200, 50): y = (x - 100) / 2
  });
  it('rays leave rectangles and ellipses exactly', () => {
    const r = { x: 10, y: 10, w: 100, h: 50 };
    expect(outlineExit('rect', r, 0, 1)).toEqual({ x: 60, y: 60 });
    const e = outlineExit('ellipse', r, 1, 0);
    expect(e.x).toBeCloseTo(110);
    expect(rayExit([[0, 0], [10, 0], [10, 10], [0, 10]], 5, 5, -1, 0)).toEqual({ x: 0, y: 5 });
  });
});

describe('connectors', () => {
  it('reads old and new arrowheads', () => {
    expect(headsOf({})).toEqual({ start: 'none', end: 'arrow' });
    expect(headsOf({ head: 'both' })).toEqual({ start: 'arrow', end: 'arrow' });
    expect(headsOf({ head: 'none' })).toEqual({ start: 'none', end: 'none' });
    expect(headsOf({ head: 'none', start: 'dot', end: 'diamond' })).toEqual({ start: 'dot', end: 'diamond' });
  });
  it('picks the facing side and the nearest port', () => {
    const r = { x: 0, y: 0, w: 100, h: 100 };
    expect(autoSide(r, { x: 300, y: 60 })).toBe('right');
    expect(autoSide(r, { x: 40, y: -200 })).toBe('top');
    const ports = { top: { x: 50, y: 0 }, right: { x: 100, y: 50 }, bottom: { x: 50, y: 100 }, left: { x: 0, y: 50 } };
    expect(nearestSide(ports, { x: 96, y: 54 }, 10)).toBe('right');
    expect(nearestSide(ports, { x: 50, y: 50 }, 10)).toBeUndefined();
  });
  it('measures and walks polylines', () => {
    const pts = [
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 10 },
    ];
    expect(polyLength(pts)).toBe(20);
    expect(pointAt(pts, 0.75)).toEqual({ x: 10, y: 5 });
    expect(nearestT(pts, { x: 12, y: 5 })).toBeCloseTo(0.75);
    expect(simplifyOrtho([{ x: 0, y: 0 }, { x: 5, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 0 }, { x: 10, y: 5 }])).toEqual([
      { x: 0, y: 0 },
      { x: 10, y: 0 },
      { x: 10, y: 5 },
    ]);
    expect(roundedPath(pts, 4)).toBe('M0,0L6,0Q10,0 10,4L10,10');
  });
  it('routes an elbow between two side ports without crossing the shapes', () => {
    const a = { x: 0, y: 0, w: 100, h: 60 };
    const b = { x: 300, y: 200, w: 100, h: 60 };
    const pts = routeElbow({ x: 108, y: 30 }, { x: 1, y: 0 }, { x: 350, y: 192 }, { x: 0, y: -1 }, [a, b]);
    expect(ortho(pts)).toBe(true);
    expect(pts[0]).toEqual({ x: 108, y: 30 });
    expect(pts[pts.length - 1]).toEqual({ x: 350, y: 192 });
    for (let i = 1; i < pts.length; i++) {
      expect(crosses(pts[i - 1], pts[i], a)).toBe(false);
      expect(crosses(pts[i - 1], pts[i], b)).toBe(false);
    }
    // one bend is enough here (right, then down)
    expect(pts.length).toBeLessThanOrEqual(4);
  });
  it('goes around a box in the way', () => {
    const a = { x: 0, y: 0, w: 80, h: 80 };
    const b = { x: 500, y: 0, w: 80, h: 80 };
    const wall = { x: 200, y: -100, w: 100, h: 280 };
    const pts = routeElbow({ x: 88, y: 40 }, { x: 1, y: 0 }, { x: 492, y: 40 }, { x: -1, y: 0 }, [a, b, wall]);
    expect(ortho(pts)).toBe(true);
    for (let i = 1; i < pts.length; i++) expect(crosses(pts[i - 1], pts[i], wall)).toBe(false);
  });
  it('leaves and re-enters the same shape around it (loop back)', () => {
    const a = { x: 0, y: 0, w: 100, h: 60 };
    const pts = routeElbow({ x: 108, y: 30 }, { x: 1, y: 0 }, { x: 50, y: -8 }, { x: 0, y: -1 }, [a]);
    expect(ortho(pts)).toBe(true);
    for (let i = 1; i < pts.length; i++) expect(crosses(pts[i - 1], pts[i], a)).toBe(false);
  });
  it('routes free ends straight when aligned', () => {
    const pts = routeElbow({ x: 0, y: 0 }, null, { x: 200, y: 0 }, null, []);
    expect(pts).toEqual([
      { x: 0, y: 0 },
      { x: 200, y: 0 },
    ]);
  });
});

describe('guides', () => {
  it('snaps edges and centres to the nearest candidate', () => {
    const g = snapGuides({ x: 0, y: 0, w: 100, h: 50 }, 97, 3, [{ x: 200, y: 200, w: 100, h: 100 }], 6);
    // right edge 197 → 200, top 3 stays (no candidate near)
    expect(g.dx).toBe(100);
    expect(g.x).toEqual([200]);
    expect(g.y).toEqual([]);
    expect(g.dy).toBe(3);
  });
  it('snaps centre to centre', () => {
    const g = snapGuides({ x: 0, y: 0, w: 100, h: 100 }, 0, 148, [{ x: 300, y: 150, w: 100, h: 100 }], 6);
    expect(g.dy).toBe(150);
    expect(g.y).toEqual([150]);
  });
  it('snaps a single edge', () => {
    const lines = guideLines([{ x: 0, y: 0, w: 100, h: 100 }], 'x');
    expect(lines).toEqual([0, 50, 100]);
    expect(snapEdge(97, lines, 6)).toEqual({ v: 100, guide: 100 });
    expect(snapEdge(70, lines, 6)).toEqual({ v: 70, guide: null });
  });
});

describe('eraser', () => {
  const line: [number, number][] = [
    [0, 0],
    [100, 0],
  ];
  it('hits strokes within reach', () => {
    expect(strokeHit(line, 4, 50, 8, 6)).toBe(true);
    expect(strokeHit(line, 4, 50, 12, 6)).toBe(false);
  });
  it('cuts a stroke in two where it is rubbed', () => {
    const parts = splitStroke(line, 2, 50, 0, 10)!;
    expect(parts).toHaveLength(2);
    expect(Math.max(...parts[0].map((p) => p[0]))).toBeLessThan(40);
    expect(Math.min(...parts[1].map((p) => p[0]))).toBeGreaterThan(60);
  });
  it('drops what is left when it is too small, and ignores misses', () => {
    expect(splitStroke(line, 2, 50, 40, 5)).toBeNull();
    expect(splitStroke([[0, 0], [4, 0]], 2, 2, 0, 10)).toEqual([]);
  });
});
