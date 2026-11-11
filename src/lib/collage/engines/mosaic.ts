/**
 * Mosaic layout: a random "slicing floorplan" (guillotine partition).
 *
 * Photos are the leaves of a binary tree whose internal nodes place their
 * children side by side (H) or stacked (V). The aspect ratio of a subtree can
 * be computed exactly from its photos' aspects, so by sampling many random
 * trees and keeping the one whose aspect best matches the page, every photo is
 * shown almost uncropped and the page is filled edge to edge.
 */
import type { Rect } from '../../model/types';
import type { Rng } from '../rng';
import { type Engine, type LayoutItem, type Placement } from './types';

type Node =
  | { leaf: number; aspect: number }
  | { dir: 'h' | 'v'; a: Node; b: Node; aspect: number };

function combine(dir: 'h' | 'v', a: Node, b: Node): Node {
  const aspect = dir === 'h' ? a.aspect + b.aspect : 1 / (1 / a.aspect + 1 / b.aspect);
  return { dir, a, b, aspect };
}

function randomTree(indices: number[], items: LayoutItem[], rng: Rng, depth = 0): Node {
  if (indices.length === 1) return { leaf: indices[0], aspect: items[indices[0]].aspect };
  const n = indices.length;
  // Prefer balanced splits, with some jitter for variety.
  let k = Math.round(n / 2 + rng.gaussian() * Math.max(0.6, n / 6));
  k = Math.max(1, Math.min(n - 1, k));
  const left = randomTree(indices.slice(0, k), items, rng, depth + 1);
  const right = randomTree(indices.slice(k), items, rng, depth + 1);
  // Choose the direction that keeps the subtree's aspect moderate, with randomness.
  const h = combine('h', left, right);
  const v = combine('v', left, right);
  const scoreH = Math.abs(Math.log(h.aspect));
  const scoreV = Math.abs(Math.log(v.aspect));
  const preferH = scoreH < scoreV;
  return rng.next() < 0.78 ? (preferH ? h : v) : preferH ? v : h;
}

function leafAreas(node: Node, rect: Rect, out: Map<number, Rect>): void {
  if ('leaf' in node) {
    out.set(node.leaf, rect);
    return;
  }
  if (node.dir === 'h') {
    const w1 = (rect.w * node.a.aspect) / (node.a.aspect + node.b.aspect);
    leafAreas(node.a, { x: rect.x, y: rect.y, w: w1, h: rect.h }, out);
    leafAreas(node.b, { x: rect.x + w1, y: rect.y, w: rect.w - w1, h: rect.h }, out);
  } else {
    const ia = 1 / node.a.aspect;
    const ib = 1 / node.b.aspect;
    const h1 = (rect.h * ia) / (ia + ib);
    leafAreas(node.a, { x: rect.x, y: rect.y, w: rect.w, h: h1 }, out);
    leafAreas(node.b, { x: rect.x, y: rect.y + h1, w: rect.w, h: rect.h - h1 }, out);
  }
}

/** Place a tree inside `rect`, leaving `gap` between siblings. */
function placeWithGap(node: Node, rect: Rect, gap: number, out: Map<number, Rect>): void {
  if ('leaf' in node) {
    out.set(node.leaf, rect);
    return;
  }
  if (node.dir === 'h') {
    const avail = Math.max(1, rect.w - gap);
    const w1 = (avail * node.a.aspect) / (node.a.aspect + node.b.aspect);
    placeWithGap(node.a, { x: rect.x, y: rect.y, w: w1, h: rect.h }, gap, out);
    placeWithGap(node.b, { x: rect.x + w1 + gap, y: rect.y, w: avail - w1, h: rect.h }, gap, out);
  } else {
    const avail = Math.max(1, rect.h - gap);
    const ia = 1 / node.a.aspect;
    const ib = 1 / node.b.aspect;
    const h1 = (avail * ia) / (ia + ib);
    placeWithGap(node.a, { x: rect.x, y: rect.y, w: rect.w, h: h1 }, gap, out);
    placeWithGap(node.b, { x: rect.x, y: rect.y + h1 + gap, w: rect.w, h: avail - h1 }, gap, out);
  }
}

function score(node: Node, target: number, n: number): number {
  const aspectErr = Math.abs(Math.log(node.aspect / target));
  // Penalise trees where some photo becomes tiny compared to the others.
  const areas = new Map<number, Rect>();
  leafAreas(node, { x: 0, y: 0, w: target, h: 1 }, areas);
  let min = Infinity;
  let max = 0;
  for (const r of areas.values()) {
    const a = r.w * r.h;
    min = Math.min(min, a);
    max = Math.max(max, a);
  }
  const balance = n > 1 ? Math.log(max / Math.max(min, 1e-9)) : 0;
  return aspectErr * 4 + balance * 0.18;
}

/**
 * The tree search depends only on the random state, the photos' aspect ratios
 * and the area's aspect ratio, not on the gap. Remember the last few results
 * so dragging the spacing slider over a big collage doesn't redo thousands of
 * random trees per tick.
 */
const searchCache = new Map<string, { tree: Node; after: number }>();
const SEARCH_CACHE_SIZE = 8;

function searchTree(items: LayoutItem[], target: number, rng: Rng): Node {
  const n = items.length;
  const key = `${rng.state()}|${target.toFixed(6)}|${items.map((i) => i.aspect.toFixed(5)).join(',')}`;
  const hit = searchCache.get(key);
  if (hit) {
    rng.setState(hit.after);
    return hit.tree;
  }
  const order = items.map((_, i) => i);
  let best: Node | null = null;
  let bestScore = Infinity;
  const tries = Math.min(2500, 300 + n * 40);
  for (let t = 0; t < tries; t++) {
    if (t % 25 === 0) rng.shuffle(order);
    const tree = randomTree(order, items, rng);
    const s = score(tree, target, n);
    if (s < bestScore) {
      bestScore = s;
      best = tree;
    }
  }
  searchCache.set(key, { tree: best!, after: rng.state() });
  if (searchCache.size > SEARCH_CACHE_SIZE) searchCache.delete(searchCache.keys().next().value!);
  return best!;
}

export function mosaicPlacements(items: LayoutItem[], area: Rect, gap: number, rng: Rng): Placement[] {
  if (items.length === 0) return [];
  // Search for a slightly rounded aspect ratio (4 % steps): dragging the
  // margin slider then keeps the arrangement instead of re-searching (and
  // jumping to a different one) on every tick. The tree is still fitted to
  // the exact area below.
  const step = 0.04;
  const target = Math.exp(Math.round(Math.log(area.w / area.h) / step) * step);
  const best = searchTree(items, target, rng);
  const out = new Map<number, Rect>();
  placeWithGap(best, area, gap, out);
  return [...out.entries()].map(([item, rect]) => ({ item, rect }));
}

export const mosaicEngine: Engine = ({ items, area, gap, rng }) => ({
  placements: mosaicPlacements(items, area, gap, rng),
});
