/**
 * Golden spiral: each photo takes the golden-ratio share of the remaining
 * rectangle, turning clockwise, so sizes shrink in a Fibonacci-like spiral.
 */
import type { Rect } from '../../model/types';
import type { Engine, Placement } from './types';

const PHI = 0.618;

export const spiralEngine: Engine = ({ items, area, gap, rng }) => {
  const n = items.length;
  if (n === 0) return { placements: [] };
  const order = rng.shuffle(items.map((_, i) => i));
  const placements: Placement[] = [];
  let rest: Rect = { ...area };
  // Start on a random side so Regenerate changes the spiral too.
  let dir = rng.int(0, 3);
  for (let k = 0; k < n; k++) {
    if (k === n - 1) {
      placements.push({ item: order[k], rect: rest });
      break;
    }
    let cell: Rect;
    if (dir === 0) {
      const w = (rest.w - gap) * PHI;
      cell = { x: rest.x, y: rest.y, w, h: rest.h };
      rest = { x: rest.x + w + gap, y: rest.y, w: rest.w - w - gap, h: rest.h };
    } else if (dir === 1) {
      const h = (rest.h - gap) * PHI;
      cell = { x: rest.x, y: rest.y, w: rest.w, h };
      rest = { x: rest.x, y: rest.y + h + gap, w: rest.w, h: rest.h - h - gap };
    } else if (dir === 2) {
      const w = (rest.w - gap) * PHI;
      cell = { x: rest.x + rest.w - w, y: rest.y, w, h: rest.h };
      rest = { x: rest.x, y: rest.y, w: rest.w - w - gap, h: rest.h };
    } else {
      const h = (rest.h - gap) * PHI;
      cell = { x: rest.x, y: rest.y + rest.h - h, w: rest.w, h };
      rest = { x: rest.x, y: rest.y, w: rest.w, h: rest.h - h - gap };
    }
    placements.push({ item: order[k], rect: cell });
    dir = (dir + 1) % 4;
  }
  return { placements };
};
