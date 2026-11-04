/** Row-, grid- and "featured photo" based engines. */
import type { Rect } from '../../model/types';
import { mosaicPlacements } from './mosaic';
import type { Engine, Placement } from './types';

/** Justified rows (like photo-gallery layouts): rows fill the width, heights fit the page. */
export const rowsEngine: Engine = ({ items, area, gap, rng }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const order = rng.shuffle(items.map((_, i) => i));
  const aspects = order.map((i) => items[i].aspect);
  const total = aspects.reduce((s, a) => s + a, 0);

  let best: number[][] = [order];
  let bestErr = Infinity;
  for (let rows = 1; rows <= n; rows++) {
    // Greedy partition into `rows` rows of roughly equal summed aspect.
    const target = total / rows;
    const groups: number[][] = [];
    let cur: number[] = [];
    let acc = 0;
    for (let k = 0; k < order.length; k++) {
      cur.push(k);
      acc += aspects[k];
      const remainingRows = rows - groups.length - 1;
      const remainingItems = order.length - k - 1;
      if ((acc >= target * 0.92 && remainingRows > 0 && remainingItems >= remainingRows) || remainingItems === remainingRows) {
        if (remainingRows > 0) {
          groups.push(cur);
          cur = [];
          acc = 0;
        }
      }
    }
    if (cur.length) groups.push(cur);
    if (groups.length !== rows) continue;
    let height = (rows - 1) * gap;
    for (const g of groups) {
      const sum = g.reduce((s, k) => s + aspects[k], 0);
      height += (area.w - (g.length - 1) * gap) / sum;
    }
    const err = Math.abs(Math.log(height / area.h));
    if (err < bestErr) {
      bestErr = err;
      best = groups;
    }
  }

  // Natural row heights, then scale to fill the area exactly.
  const natural = best.map((g) => {
    const sum = g.reduce((s, k) => s + aspects[k], 0);
    return (area.w - (g.length - 1) * gap) / sum;
  });
  const avail = area.h - (best.length - 1) * gap;
  const scale = avail / natural.reduce((s, h) => s + h, 0);
  const placements: Placement[] = [];
  let y = area.y;
  best.forEach((g, r) => {
    const h = natural[r] * scale;
    const sum = g.reduce((s, k) => s + aspects[k], 0);
    const availW = area.w - (g.length - 1) * gap;
    let x = area.x;
    g.forEach((k, idx) => {
      const w = idx === g.length - 1 ? area.x + area.w - x : (availW * aspects[k]) / sum;
      placements.push({ item: order[k], rect: { x, y, w, h } });
      x += w + gap;
    });
    y += h + gap;
  });
  return { placements };
};

/** Choose a cols × rows grid for n photos that best matches their typical aspect. */
export function chooseGrid(n: number, area: Rect, medianAspect: number, fixedCols = 0): [number, number] {
  if (fixedCols > 0) return [Math.min(fixedCols, n), Math.ceil(n / Math.min(fixedCols, n))];
  let best: [number, number] = [1, n];
  let bestScore = Infinity;
  for (let cols = 1; cols <= n; cols++) {
    const rows = Math.ceil(n / cols);
    const cellAspect = area.w / cols / (area.h / rows);
    const empty = rows * cols - n;
    const s = Math.abs(Math.log(cellAspect / medianAspect)) + (empty / n) * 1.4 + (empty >= cols ? 5 : 0);
    if (s < bestScore) {
      bestScore = s;
      best = [cols, rows];
    }
  }
  return best;
}

export const gridEngine: Engine = ({ items, area, gap, settings }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const sorted = items.map((i) => i.aspect).sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)];
  const [cols, rows] = chooseGrid(n, area, median, settings.columns ?? 0);
  const cw = (area.w - (cols - 1) * gap) / cols;
  const ch = (area.h - (rows - 1) * gap) / rows;
  const placements: Placement[] = [];
  for (let i = 0; i < n; i++) {
    const r = Math.floor(i / cols);
    const c = i % cols;
    const inRow = r === rows - 1 ? n - r * cols : cols;
    // Centre an incomplete last row.
    const offset = ((cols - inRow) * (cw + gap)) / 2;
    placements.push({
      item: i,
      rect: { x: area.x + offset + c * (cw + gap), y: area.y + r * (ch + gap), w: cw, h: ch },
    });
  }
  return { placements };
};

/** One large "hero" photo with the rest arranged as a mosaic beside it. */
export const featuredEngine: Engine = ({ items, area, gap, rng }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  if (n === 1) return { placements: [{ item: 0, rect: area }] };
  const landscape = area.w >= area.h;
  const heroShare = n <= 3 ? 0.58 : n <= 8 ? 0.55 : 0.48;
  const flip = rng.next() < 0.5;
  const rest = items.slice(1);
  let hero: Rect;
  let other: Rect;
  if (landscape) {
    const hw = (area.w - gap) * heroShare;
    hero = { x: flip ? area.x + area.w - hw : area.x, y: area.y, w: hw, h: area.h };
    other = { x: flip ? area.x : area.x + hw + gap, y: area.y, w: area.w - hw - gap, h: area.h };
  } else {
    const hh = (area.h - gap) * heroShare;
    hero = { x: area.x, y: flip ? area.y + area.h - hh : area.y, w: area.w, h: hh };
    other = { x: area.x, y: flip ? area.y : area.y + hh + gap, w: area.w, h: area.h - hh - gap };
  }
  const placements: Placement[] = [{ item: 0, rect: hero }];
  for (const p of mosaicPlacements(rest, other, gap, rng)) placements.push({ item: p.item + 1, rect: p.rect });
  return { placements };
};
