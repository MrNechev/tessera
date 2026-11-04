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
