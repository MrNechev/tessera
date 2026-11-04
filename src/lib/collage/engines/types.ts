import type { LayoutSettings, Mask, PhotoFrame, Rect, Shadow } from '../../model/types';
import type { Rng } from '../rng';

export interface LayoutItem {
  /** Asset id. */
  key: string;
  /** Width / height. */
  aspect: number;
}

export interface Placement {
  /** Index into the input items (shape layouts may repeat items). */
  item: number;
  rect: Rect;
  rotation?: number;
}

export interface EngineInput {
  items: LayoutItem[];
  /** Area available to the layout (page minus margins). */
  area: Rect;
  /** Gap between photos, in page units. */
  gap: number;
  rng: Rng;
  settings: LayoutSettings;
  pageShort: number;
}

export interface EngineStyle {
  mask?: Mask;
  frame?: PhotoFrame | null;
  shadow?: Shadow | null;
}

export interface EngineResult {
  placements: Placement[];
  /** Style forced onto every managed photo by this engine (e.g. circles for `ring`). */
  style?: EngineStyle;
}

export type Engine = (input: EngineInput) => EngineResult;

export function insetRect(r: Rect, d: number): Rect {
  const dx = Math.min(d, r.w / 2 - 0.5);
  const dy = Math.min(d, r.h / 2 - 0.5);
  return { x: r.x + dx, y: r.y + dy, w: r.w - 2 * dx, h: r.h - 2 * dy };
}

/**
 * Many engines compute edge-to-edge cells; this turns them into cells with a
 * uniform `gap` between neighbours while keeping the outer edge where it is.
 */
export function applyGap(cells: Rect[], area: Rect, gap: number): Rect[] {
  if (gap <= 0) return cells;
  const half = gap / 2;
  // Expand the area by half a gap so the outer gap stays equal to the margin.
  const sx = (area.w + gap) / area.w;
  const sy = (area.h + gap) / area.h;
  return cells.map((c) => {
    const x = area.x - half + (c.x - area.x) * sx;
    const y = area.y - half + (c.y - area.y) * sy;
    return insetRect({ x, y, w: c.w * sx, h: c.h * sy }, half);
  });
}
