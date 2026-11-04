/** Vector outlines for photo masks and shape elements, built in a local w × h box. */
import type { Mask, MaskShape, ShapeKind } from '../model/types';

export const MASK_SHAPES: { id: MaskShape; name: string }[] = [
  { id: 'rect', name: 'Rectangle' },
  { id: 'squircle', name: 'Squircle' },
  { id: 'circle', name: 'Circle' },
  { id: 'pill', name: 'Pill' },
  { id: 'arch', name: 'Arch' },
  { id: 'heart', name: 'Heart' },
  { id: 'star', name: 'Star' },
  { id: 'hexagon', name: 'Hexagon' },
  { id: 'octagon', name: 'Octagon' },
  { id: 'diamond', name: 'Diamond' },
  { id: 'triangle', name: 'Triangle' },
  { id: 'scallop', name: 'Scallop' },
  { id: 'cloud', name: 'Cloud' },
  { id: 'blob', name: 'Blob' },
];

// Complex outlines authored in a 100 × 100 box.
const SVG_PATHS: Partial<Record<MaskShape, string>> = {
  heart:
    'M50 96 C24 77 2 58 2 32 C2 13 15 2 30 2 C40 2 47 8 50 17 C53 8 60 2 70 2 C85 2 98 13 98 32 C98 58 76 77 50 96 Z',
  cloud:
    'M24 86 C10 86 2 76 2 64 C2 52 11 43 23 43 C24 26 37 14 53 14 C66 14 77 22 81 34 C92 35 98 45 98 57 C98 73 87 86 72 86 Z',
  blob:
    'M52 3 C70 2 88 10 95 27 C101 43 96 58 90 71 C83 86 68 97 50 97 C31 98 14 89 6 73 C-1 58 2 40 9 26 C17 11 34 4 52 3 Z',
};

function roundRectPath(p: Path2D, x: number, y: number, w: number, h: number, r: number): void {
  r = Math.max(0, Math.min(r, w / 2, h / 2));
  if (r <= 0.01) {
    p.rect(x, y, w, h);
    return;
  }
  p.moveTo(x + r, y);
  p.lineTo(x + w - r, y);
  p.arcTo(x + w, y, x + w, y + r, r);
  p.lineTo(x + w, y + h - r);
  p.arcTo(x + w, y + h, x + w - r, y + h, r);
  p.lineTo(x + r, y + h);
  p.arcTo(x, y + h, x, y + h - r, r);
  p.lineTo(x, y + r);
  p.arcTo(x, y, x + r, y, r);
  p.closePath();
}

function polygon(p: Path2D, pts: [number, number][], w: number, h: number, ox = 0, oy = 0): void {
  pts.forEach(([x, y], i) => (i === 0 ? p.moveTo(ox + x * w, oy + y * h) : p.lineTo(ox + x * w, oy + y * h)));
  p.closePath();
}

function starPoints(points = 5, inner = 0.5): [number, number][] {
  const raw: [number, number][] = [];
  for (let i = 0; i < points * 2; i++) {
    const r = i % 2 === 0 ? 1 : inner;
    const a = -Math.PI / 2 + (i * Math.PI) / points;
    raw.push([Math.cos(a) * r, Math.sin(a) * r]);
  }
  const xs = raw.map((p) => p[0]);
  const ys = raw.map((p) => p[1]);
  const x0 = Math.min(...xs);
  const x1 = Math.max(...xs);
  const y0 = Math.min(...ys);
  const y1 = Math.max(...ys);
  return raw.map(([x, y]) => [(x - x0) / (x1 - x0), (y - y0) / (y1 - y0)]);
}

const STAR = starPoints(5, 0.5);

/**
 * Outline of `shape` filling the box (x, y, w, h).
 * `radius` is a fraction of the shorter side (used by rect).
 */
export function shapePath(shape: ShapeKind, w: number, h: number, radius = 0, x = 0, y = 0): Path2D {
  const p = new Path2D();
  const m = Math.min(w, h);
  switch (shape) {
    case 'rect':
    case 'line':
      roundRectPath(p, x, y, w, h, radius * m);
      break;
    case 'pill':
      roundRectPath(p, x, y, w, h, m / 2);
      break;
    case 'circle':
    case 'ring':
      p.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
      break;
    case 'squircle': {
      const n = 4;
      const steps = 96;
      for (let i = 0; i <= steps; i++) {
        const t = (i / steps) * Math.PI * 2;
        const c = Math.cos(t);
        const s = Math.sin(t);
        const px = x + w / 2 + (w / 2) * Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
        const py = y + h / 2 + (h / 2) * Math.sign(s) * Math.pow(Math.abs(s), 2 / n);
        if (i === 0) p.moveTo(px, py);
        else p.lineTo(px, py);
      }
      p.closePath();
      break;
    }
    case 'arch': {
      const ry = Math.min(w / 2, h / 2);
      p.moveTo(x, y + h);
      p.lineTo(x, y + ry);
      p.ellipse(x + w / 2, y + ry, w / 2, ry, 0, Math.PI, 0);
      p.lineTo(x + w, y + h);
      p.closePath();
      break;
    }
    case 'star':
      polygon(p, STAR, w, h, x, y);
      break;
    case 'hexagon':
      polygon(p, [[0.25, 0], [0.75, 0], [1, 0.5], [0.75, 1], [0.25, 1], [0, 0.5]], w, h, x, y);
      break;
    case 'octagon': {
      const c = 0.2929;
      polygon(p, [[c, 0], [1 - c, 0], [1, c], [1, 1 - c], [1 - c, 1], [c, 1], [0, 1 - c], [0, c]], w, h, x, y);
      break;
    }
    case 'diamond':
      polygon(p, [[0.5, 0], [1, 0.5], [0.5, 1], [0, 0.5]], w, h, x, y);
      break;
    case 'triangle':
      polygon(p, [[0.5, 0], [1, 1], [0, 1]], w, h, x, y);
      break;
    case 'scallop': {
      const bumps = 16;
      const cx = x + w / 2;
      const cy = y + h / 2;
      const rx = (w / 2) * 0.9;
      const ry = (h / 2) * 0.9;
      for (let i = 0; i <= bumps; i++) {
        const a = (i / bumps) * Math.PI * 2 - Math.PI / 2;
        const px = cx + Math.cos(a) * rx;
        const py = cy + Math.sin(a) * ry;
        if (i === 0) {
          p.moveTo(px, py);
          continue;
        }
        const am = a - Math.PI / bumps;
        p.quadraticCurveTo(cx + Math.cos(am) * rx * 1.2, cy + Math.sin(am) * ry * 1.2, px, py);
      }
      p.closePath();
      break;
    }
    case 'heart':
    case 'cloud':
    case 'blob': {
      const src = new Path2D(SVG_PATHS[shape]);
      const m2 = new DOMMatrix().translateSelf(x, y).scaleSelf(w / 100, h / 100);
      p.addPath(src, m2);
      break;
    }
    default:
      p.rect(x, y, w, h);
  }
  return p;
}

export function maskPath(mask: Mask, w: number, h: number, x = 0, y = 0): Path2D {
  return shapePath(mask.shape, w, h, mask.radius, x, y);
}
