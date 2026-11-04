/** Geometry of decorative photo frames, shared by the renderer and layout engines. */
import type { PhotoFrame, Rect } from '../model/types';

const POLAROID_SIDE = 0.06;
const POLAROID_BOTTOM = 0.22;
const INSTANT_SIDE = 0.05;
const INSTANT_BOTTOM = 0.15;

/** Height of a polaroid frame of width `w` whose photo window has the given aspect (w/h). */
export function polaroidHeightFor(w: number, photoAspect: number): number {
  const p = w * POLAROID_SIDE;
  return p + (w - 2 * p) / photoAspect + w * POLAROID_BOTTOM;
}

/** Where the photo itself is drawn inside an element box of size w × h. */
export function photoWindow(frame: PhotoFrame | null | undefined, w: number, h: number): Rect {
  const style = frame?.style ?? 'none';
  switch (style) {
    case 'polaroid': {
      const p = w * POLAROID_SIDE;
      const pb = Math.min(w * POLAROID_BOTTOM, h * 0.3);
      return { x: p, y: p, w: w - 2 * p, h: Math.max(1, h - p - pb) };
    }
    case 'instant': {
      const p = w * INSTANT_SIDE;
      const pb = Math.min(w * INSTANT_BOTTOM, h * 0.25);
      return { x: p, y: p, w: w - 2 * p, h: Math.max(1, h - p - pb) };
    }
    case 'film': {
      const band = h * 0.12;
      const side = Math.min(w, h) * 0.035;
      return { x: side, y: band, w: w - 2 * side, h: h - 2 * band };
    }
    case 'mat': {
      const p = Math.min(w, h) * 0.075;
      return { x: p, y: p, w: w - 2 * p, h: h - 2 * p };
    }
    default:
      return { x: 0, y: 0, w, h };
  }
}

/** Caption area for polaroid / instant frames (below the photo window). */
export function captionArea(frame: PhotoFrame | null | undefined, w: number, h: number): Rect | null {
  if (!frame || (frame.style !== 'polaroid' && frame.style !== 'instant')) return null;
  const win = photoWindow(frame, w, h);
  const top = win.y + win.h;
  return { x: win.x, y: top, w: win.w, h: h - top };
}
