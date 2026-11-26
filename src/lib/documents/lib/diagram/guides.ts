// Smart alignment guides: while boxes move, their edges and centres snap to the edges
// and centres of nearby boxes that stay put.
import type { Rect } from '../types';

export interface GuideSnap {
  /** the move after snapping */
  dx: number;
  dy: number;
  /** world x of the vertical guide line / y of the horizontal one (empty = no snap on that axis) */
  x: number[];
  y: number[];
}

/**
 * `box` is the moving selection's bounds before the move, (dx, dy) the raw move and
 * `cands` the boxes it can line up with. Within `thr` world px, the nearest edge/centre
 * pair wins on each axis.
 */
export function snapGuides(box: Rect, dx: number, dy: number, cands: Rect[], thr: number): GuideSnap {
  const out: GuideSnap = { dx, dy, x: [], y: [] };
  const xs = [box.x + dx, box.x + dx + box.w / 2, box.x + dx + box.w];
  const ys = [box.y + dy, box.y + dy + box.h / 2, box.y + dy + box.h];
  let bestX = thr,
    bestY = thr,
    adjX = 0,
    adjY = 0;
  for (const c of cands) {
    for (const cx of [c.x, c.x + c.w / 2, c.x + c.w])
      for (const x of xs)
        if (Math.abs(cx - x) < bestX) {
          bestX = Math.abs(cx - x);
          adjX = cx - x;
          out.x = [cx];
        }
    for (const cy of [c.y, c.y + c.h / 2, c.y + c.h])
      for (const y of ys)
        if (Math.abs(cy - y) < bestY) {
          bestY = Math.abs(cy - y);
          adjY = cy - y;
          out.y = [cy];
        }
  }
  if (out.x.length) out.dx += adjX;
  if (out.y.length) out.dy += adjY;
  return out;
}

/**
 * Snap one moving edge (resize): `v` is the edge's world coordinate, `lines` the edges and
 * centres of the other boxes on that axis. Returns the snapped value and the guide (or null).
 */
export function snapEdge(v: number, lines: number[], thr: number): { v: number; guide: number | null } {
  let best = thr,
    guide: number | null = null;
  for (const l of lines)
    if (Math.abs(l - v) < best) {
      best = Math.abs(l - v);
      guide = l;
    }
  return { v: guide ?? v, guide };
}

/** edges and centres of boxes on one axis (for snapEdge) */
export function guideLines(rs: Rect[], axis: 'x' | 'y'): number[] {
  const out: number[] = [];
  for (const r of rs) {
    if (axis === 'x') out.push(r.x, r.x + r.w / 2, r.x + r.w);
    else out.push(r.y, r.y + r.h / 2, r.y + r.h);
  }
  return out;
}
