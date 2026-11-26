// Eraser for pen / highlighter strokes: hit-testing a stroke against the eraser circle and
// cutting the erased part out of it (the rest becomes one stroke per piece).

type P = [number, number];

function segDist(px: number, py: number, a: P, b: P) {
  const dx = b[0] - a[0],
    dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 ? Math.max(0, Math.min(1, ((px - a[0]) * dx + (py - a[1]) * dy) / l2)) : 0;
  return Math.hypot(px - (a[0] + dx * t), py - (a[1] + dy * t));
}

/** does a stroke (absolute points, line width w) come within r of (x, y)? */
export function strokeHit(pts: P[], w: number, x: number, y: number, r: number): boolean {
  const lim = r + w / 2;
  if (pts.length === 1) return Math.hypot(pts[0][0] - x, pts[0][1] - y) <= lim;
  for (let i = 1; i < pts.length; i++) if (segDist(x, y, pts[i - 1], pts[i]) <= lim) return true;
  return false;
}

/** extra points so no segment is longer than `step` */
export function resample(pts: P[], step: number): P[] {
  if (pts.length < 2) return pts;
  const out: P[] = [pts[0]];
  for (let i = 1; i < pts.length; i++) {
    const a = pts[i - 1],
      b = pts[i];
    const n = Math.ceil(Math.hypot(b[0] - a[0], b[1] - a[1]) / step);
    for (let k = 1; k <= n; k++) out.push([a[0] + ((b[0] - a[0]) * k) / n, a[1] + ((b[1] - a[1]) * k) / n]);
  }
  return out;
}

/**
 * Cut the part of a stroke within r of (x, y) out: returns the pieces that are left
 * (each at least two points), or null when nothing was touched.
 */
export function splitStroke(pts: P[], w: number, x: number, y: number, r: number): P[][] | null {
  if (!strokeHit(pts, w, x, y, r)) return null;
  const lim = r + w / 2;
  const dense = resample(pts, Math.max(0.5, r / 3));
  const pieces: P[][] = [];
  let cur: P[] = [];
  for (const p of dense) {
    if (Math.hypot(p[0] - x, p[1] - y) <= lim) {
      if (cur.length >= 2) pieces.push(cur);
      cur = [];
    } else cur.push(p);
  }
  if (cur.length >= 2) pieces.push(cur);
  return pieces;
}
