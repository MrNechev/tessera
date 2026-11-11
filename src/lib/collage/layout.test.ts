import { describe, expect, it } from 'vitest';
import { createDoc, createPhoto } from '../model/factory';
import type { CollageDoc, LayoutEngine, Rect } from '../model/types';
import { columnCount } from './engines/columns';
import { layoutArea, relayout } from './layout';

function docWith(n: number, engine: LayoutEngine, w = 3508, h = 2480): CollageDoc {
  const doc = createDoc({ width: w, height: h });
  doc.layout.engine = engine;
  doc.layout.seed = 1234;
  for (let i = 0; i < n; i++) {
    const id = `a${i}`;
    // Mix of landscape, portrait and square photos.
    const kind = i % 3;
    doc.assets[id] = { path: `/p/${id}.jpg`, name: id, width: kind === 0 ? 4000 : kind === 1 ? 3000 : 2000, height: kind === 0 ? 3000 : kind === 1 ? 4000 : 2000 };
    doc.elements.push(createPhoto({ x: 0, y: 0, w: 10, h: 10, asset: id, slot: true }));
  }
  return doc;
}

function overlap(a: Rect, b: Rect): number {
  const w = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

const TILING: LayoutEngine[] = ['mosaic', 'columns', 'rows', 'grid', 'featured'];
const ALL: LayoutEngine[] = [...TILING, 'shape', 'scatter', 'ring'];

describe('layout engines', () => {
  for (const engine of ALL) {
    for (const n of [1, 2, 3, 7, 20, 64]) {
      it(`${engine} lays out ${n} photos sanely`, () => {
        const doc = relayout(docWith(n, engine));
        const photos = doc.elements.filter((e) => e.kind === 'photo');
        if (engine === 'shape') expect(photos.length).toBeGreaterThanOrEqual(n);
        else expect(photos.length).toBe(n);
        const assets = new Set(photos.map((p) => (p.kind === 'photo' ? p.asset : null)));
        expect(assets.size).toBe(n);
        for (const p of photos) {
          for (const v of [p.x, p.y, p.w, p.h, p.rotation]) expect(Number.isFinite(v)).toBe(true);
          expect(p.w).toBeGreaterThan(0);
          expect(p.h).toBeGreaterThan(0);
          if (engine !== 'scatter') {
            expect(p.x).toBeGreaterThanOrEqual(-0.5);
            expect(p.y).toBeGreaterThanOrEqual(-0.5);
            expect(p.x + p.w).toBeLessThanOrEqual(doc.page.width + 0.5);
            expect(p.y + p.h).toBeLessThanOrEqual(doc.page.height + 0.5);
          }
        }
        if (engine === 'ring') {
          for (let i = 0; i < photos.length; i++)
            for (let j = i + 1; j < photos.length; j++) {
              const a = photos[i];
              const b = photos[j];
              const d = Math.hypot(a.x + a.w / 2 - (b.x + b.w / 2), a.y + a.h / 2 - (b.y + b.h / 2));
              expect(d).toBeGreaterThanOrEqual(a.w / 2 + b.w / 2 - 1);
            }
        }
        if (TILING.includes(engine) || engine === 'shape') {
          for (let i = 0; i < photos.length; i++)
            for (let j = i + 1; j < photos.length; j++) expect(overlap(photos[i], photos[j])).toBeLessThan(1);
        }
        if (engine === 'mosaic' || engine === 'columns' || engine === 'rows') {
          // Tiling engines fill the layout area (minus gaps).
          const area = layoutArea(doc);
          const covered = photos.reduce((s, p) => s + p.w * p.h, 0);
          expect(covered / (area.w * area.h)).toBeGreaterThan(0.85);
        }
      });
    }
  }

  it('is deterministic for a seed and changes on shuffle', () => {
    const a = relayout(docWith(12, 'mosaic'));
    const b = relayout(docWith(12, 'mosaic'));
    expect(a.elements.map((e) => [e.x, e.y])).toEqual(b.elements.map((e) => [e.x, e.y]));
    const c = relayout(a, { shuffle: true });
    expect(c.layout.seed).not.toBe(a.layout.seed);
  });

  it('keeps element ids and styling across regenerations', () => {
    const a = relayout(docWith(6, 'grid'));
    const first = a.elements[0];
    if (first.kind === 'photo') first.border = { width: 12, color: '#ff0000' };
    const b = relayout(a, { shuffle: true });
    const same = b.elements.find((e) => e.id === first.id);
    expect(same).toBeDefined();
    expect(same && same.kind === 'photo' ? same.border?.color : null).toBe('#ff0000');
  });

  it('matches PhotoCollage column heuristic', () => {
    // 10 landscape 4:3 photos on an A4 landscape page.
    expect(columnCount(Array(10).fill(4 / 3), 2480 / 3508)).toBe(Math.round(Math.sqrt((0.75 / (2480 / 3508)) * 20)));
  });
});

import { resizeBox } from '../editor/geometry';

describe('resizeBox', () => {
  it('scales proportionally along the diagonal for wide boxes', () => {
    const start = { x: 0, y: 0, w: 300, h: 50, rotation: 0 };
    // Drag the SE corner 30 px right and 50 px down.
    const r = resizeBox(start, 1, 1, { x: 330, y: 100 }, true);
    expect(r.w / r.h).toBeCloseTo(6, 5);
    expect(r.w).toBeGreaterThan(300);
    expect(r.w).toBeLessThan(400);
    expect(r.x).toBeCloseTo(0, 5);
    expect(r.y).toBeCloseTo(0, 5);
  });

  it('keeps the opposite corner fixed for rotated boxes', () => {
    const start = { x: 100, y: 100, w: 200, h: 100, rotation: 30 };
    const r = resizeBox(start, 1, 1, { x: 450, y: 400 }, false);
    const rot = (p: { x: number; y: number }, c: { x: number; y: number }, deg: number) => {
      const a = (deg * Math.PI) / 180;
      return { x: c.x + (p.x - c.x) * Math.cos(a) - (p.y - c.y) * Math.sin(a), y: c.y + (p.x - c.x) * Math.sin(a) + (p.y - c.y) * Math.cos(a) };
    };
    const before = rot({ x: 100, y: 100 }, { x: 200, y: 150 }, 30);
    const after = rot({ x: r.x, y: r.y }, { x: r.x + r.w / 2, y: r.y + r.h / 2 }, 30);
    expect(after.x).toBeCloseTo(before.x, 4);
    expect(after.y).toBeCloseTo(before.y, 4);
  });
});

import { shapeCells } from './engines/artistic';

describe('shape layouts', () => {
  it('draws a heart with a visible notch between the lobes', () => {
    const area = { x: 0, y: 0, w: 1000, h: 1000 };
    const cells = shapeCells('heart', area, 4, 40);
    expect(cells.length).toBeGreaterThanOrEqual(40);
    const top = Math.min(...cells.map((c) => c.y));
    const row = cells.filter((c) => Math.abs(c.y - top) < 1).map((c) => c.x + c.w / 2).sort((a, b) => a - b);
    const centre = (Math.min(...cells.map((c) => c.x)) + Math.max(...cells.map((c) => c.x + c.w))) / 2;
    // No cell of the top row sits on the vertical centre line.
    for (const cx of row) expect(Math.abs(cx - centre)).toBeGreaterThan(cells[0].w / 2);
    // …and the figure is left/right symmetric.
    const xs = cells.map((c) => c.x + c.w / 2 - centre).sort((a, b) => a - b);
    const mirrored = xs.map((x) => -x).sort((a, b) => a - b);
    xs.forEach((x, i) => expect(Math.abs(x - mirrored[i])).toBeLessThan(1));
  });
});

import { maskCells, type FigureMask } from './engines/artistic';
import { daysInMonth, monthGrid } from '../render/calendar';

describe('mask layouts', () => {
  // A chunky "T": 10×10 bitmap.
  const bitmap = [
    '1111111111',
    '1111111111',
    '1111111111',
    '0001111000',
    '0001111000',
    '0001111000',
    '0001111000',
    '0001111000',
    '0001111000',
    '0001111000',
  ];
  const mask: FigureMask = {
    aspect: 1,
    strict: true,
    inside: (u, v) => u >= 0 && v >= 0 && u < 1 && v < 1 && bitmap[Math.floor(v * 10)][Math.floor(u * 10)] === '1',
  };

  it('fills a T-shaped mask with cells only where the mask is', () => {
    const area = { x: 0, y: 0, w: 1000, h: 1000 };
    const cells = maskCells(mask, area, 6, 40);
    expect(cells.length).toBeGreaterThanOrEqual(40);
    for (const c of cells) {
      const u = (c.x + c.w / 2) / 1000;
      const v = (c.y + c.h / 2) / 1000;
      expect(mask.inside(u, v)).toBe(true);
      expect(c.x).toBeGreaterThanOrEqual(-0.5);
      expect(c.x + c.w).toBeLessThanOrEqual(1000.5);
    }
    // The bar is wider than the stem.
    const top = cells.filter((c) => c.y < 250).length;
    const stem = cells.filter((c) => c.y > 400).length;
    expect(top).toBeGreaterThan(0);
    expect(stem).toBeGreaterThan(0);
  });

  it('letters and spiral engines lay out photos without errors', () => {
    for (const engine of ['letters', 'spiral'] as const) {
      for (const n of [1, 3, 9]) {
        const doc = relayout(docWith(n, engine));
        const photos = doc.elements.filter((e) => e.kind === 'photo');
        expect(photos.length).toBeGreaterThanOrEqual(n);
        for (const p of photos) {
          expect(p.w).toBeGreaterThan(0);
          expect(p.h).toBeGreaterThan(0);
          expect(p.x + p.w).toBeLessThanOrEqual(doc.page.width + 0.5);
          expect(p.y + p.h).toBeLessThanOrEqual(doc.page.height + 0.5);
        }
        if (engine === 'spiral') {
          for (let i = 0; i < photos.length; i++)
            for (let j = i + 1; j < photos.length; j++) expect(overlap(photos[i], photos[j])).toBeLessThan(1);
        }
      }
    }
  });
});

describe('calendar dates', () => {
  it('knows month lengths and leap years', () => {
    expect(daysInMonth(2026, 2)).toBe(28);
    expect(daysInMonth(2028, 2)).toBe(29);
    expect(daysInMonth(2026, 11)).toBe(30);
  });
  it('lays out November 2026 starting on Monday', () => {
    // 1 Nov 2026 is a Sunday.
    const g = monthGrid(2026, 11, 1);
    expect(g[0]).toEqual([0, 0, 0, 0, 0, 0, 1]);
    expect(g[g.length - 1].filter(Boolean).pop()).toBe(30);
    expect(g.every((w) => w.length === 7)).toBe(true);
    const sun = monthGrid(2026, 11, 0);
    expect(sun[0]).toEqual([1, 2, 3, 4, 5, 6, 7]);
  });
});

describe('mosaic search cache', () => {
  const rects = (d: CollageDoc) =>
    d.elements.flatMap((e) => (e.kind === 'photo' ? [[e.asset, e.x, e.y, e.w, e.h].map((v) => (typeof v === 'number' ? Math.round(v) : v))] : []));

  it('gives the same layout on a repeat run as on the first', () => {
    const a = relayout(docWith(120, 'mosaic'));
    const b = relayout(docWith(120, 'mosaic'));
    expect(rects(b)).toEqual(rects(a));
  });

  it('keeps the arrangement when only the spacing changes', () => {
    const base = docWith(40, 'mosaic');
    const tight = relayout({ ...base, layout: { ...base.layout, spacing: 0.002 } });
    // Chained, like a slider: each relayout starts from the previous result.
    const loose = relayout({ ...tight, layout: { ...tight.layout, spacing: 0.03 } });
    // Same tree, so every photo stays roughly where it was (only the gaps move).
    const pos = (d: CollageDoc) => new Map(d.elements.flatMap((e) => (e.kind === 'photo' ? [[e.asset, e]] : [])));
    const t = pos(tight);
    for (const [a, e] of pos(loose)) {
      const o = t.get(a)!;
      expect(Math.abs(e.x + e.w / 2 - (o.x + o.w / 2))).toBeLessThan(base.page.width * 0.05);
      expect(Math.abs(e.y + e.h / 2 - (o.y + o.h / 2))).toBeLessThan(base.page.height * 0.05);
    }
  });

  it('is not slower the second time with many photos', () => {
    const doc = docWith(150, 'mosaic');
    let t = performance.now();
    const once = relayout({ ...doc, layout: { ...doc.layout, spacing: 0.01 } });
    const first = performance.now() - t;
    t = performance.now();
    relayout({ ...once, layout: { ...once.layout, spacing: 0.02 } });
    const second = performance.now() - t;
    expect(second).toBeLessThan(Math.max(first / 3, 5));
  });
});

describe('relayout stability', () => {
  for (const engine of ALL) {
    it(`${engine}: re-running the layout with the same settings changes nothing`, () => {
      const once = relayout(docWith(30, engine));
      const twice = relayout(once);
      const pos = (d: CollageDoc) =>
        d.elements.flatMap((e) => (e.kind === 'photo' ? [`${e.asset}:${Math.round(e.x)},${Math.round(e.y)},${Math.round(e.w)},${Math.round(e.h)}`] : [])).sort();
      expect(pos(twice)).toEqual(pos(once));
    });
  }
});

describe('margin changes', () => {
  it('a small margin change keeps the mosaic arrangement', () => {
    const a = relayout(docWith(40, 'mosaic'));
    const b = relayout({ ...a, layout: { ...a.layout, margin: a.layout.margin + 0.002 } });
    const order = (d: CollageDoc) => d.elements.flatMap((e) => (e.kind === 'photo' ? [e.asset] : []));
    expect(order(b)).toEqual(order(a));
    const pos = (d: CollageDoc) => new Map(d.elements.flatMap((e) => (e.kind === 'photo' ? [[e.asset, e]] : [])));
    const pa = pos(a);
    for (const [k, e] of pos(b)) expect(Math.abs(e.x - pa.get(k)!.x)).toBeLessThan(a.page.width * 0.03);
  });
});
