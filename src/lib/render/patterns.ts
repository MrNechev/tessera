/** Procedural background patterns and paper textures. */
import type { PatternId, TextureId } from '../model/types';
import { mulberry32 } from '../collage/rng';

export type AnyCanvas = HTMLCanvasElement | OffscreenCanvas;
export type AnyCtx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

export function makeCanvas(w: number, h: number): AnyCanvas {
  w = Math.max(1, Math.round(w));
  h = Math.max(1, Math.round(h));
  if (typeof OffscreenCanvas !== 'undefined') return new OffscreenCanvas(w, h);
  const c = document.createElement('canvas');
  c.width = w;
  c.height = h;
  return c;
}

export function ctx2d(c: AnyCanvas): AnyCtx {
  return c.getContext('2d') as AnyCtx;
}

export const PATTERNS: { id: PatternId; name: string }[] = [
  { id: 'dots', name: 'Dots' },
  { id: 'polka', name: 'Polka' },
  { id: 'stripes', name: 'Stripes' },
  { id: 'pinstripe', name: 'Pinstripe' },
  { id: 'grid', name: 'Grid' },
  { id: 'checker', name: 'Checker' },
  { id: 'gingham', name: 'Gingham' },
  { id: 'hearts', name: 'Hearts' },
  { id: 'stars', name: 'Stars' },
  { id: 'snowflakes', name: 'Snowflakes' },
  { id: 'confetti', name: 'Confetti' },
  { id: 'sparkles', name: 'Sparkles' },
  { id: 'chevron', name: 'Chevron' },
  { id: 'waves', name: 'Waves' },
  { id: 'plus', name: 'Plus' },
  { id: 'triangles', name: 'Triangles' },
];

export const TEXTURES: { id: TextureId; name: string }[] = [
  { id: 'paper', name: 'Paper' },
  { id: 'linen', name: 'Linen' },
  { id: 'kraft', name: 'Kraft' },
  { id: 'grain', name: 'Film grain' },
  { id: 'watercolor', name: 'Watercolor' },
];

const CONFETTI = ['#ff5d8f', '#ffc145', '#3ec6b8', '#8b5cf6', '#ff8a4c', '#4f8cff'];

function heart(c: AnyCtx, cx: number, cy: number, s: number) {
  c.beginPath();
  c.moveTo(cx, cy + s * 0.42);
  c.bezierCurveTo(cx - s * 0.52, cy + s * 0.06, cx - s * 0.5, cy - s * 0.46, cx, cy - s * 0.22);
  c.bezierCurveTo(cx + s * 0.5, cy - s * 0.46, cx + s * 0.52, cy + s * 0.06, cx, cy + s * 0.42);
  c.fill();
}

function star(c: AnyCtx, cx: number, cy: number, r: number, points = 5, inner = 0.48) {
  c.beginPath();
  for (let i = 0; i < points * 2; i++) {
    const rr = i % 2 === 0 ? r : r * inner;
    const a = -Math.PI / 2 + (i * Math.PI) / points;
    const x = cx + Math.cos(a) * rr;
    const y = cy + Math.sin(a) * rr;
    if (i === 0) c.moveTo(x, y);
    else c.lineTo(x, y);
  }
  c.closePath();
  c.fill();
}

function sparkle(c: AnyCtx, cx: number, cy: number, r: number) {
  c.beginPath();
  c.moveTo(cx, cy - r);
  c.quadraticCurveTo(cx, cy, cx + r, cy);
  c.quadraticCurveTo(cx, cy, cx, cy + r);
  c.quadraticCurveTo(cx, cy, cx - r, cy);
  c.quadraticCurveTo(cx, cy, cx, cy - r);
  c.fill();
}

function snowflake(c: AnyCtx, cx: number, cy: number, r: number) {
  c.save();
  c.translate(cx, cy);
  c.lineCap = 'round';
  c.lineWidth = r * 0.12;
  for (let i = 0; i < 6; i++) {
    c.rotate(Math.PI / 3);
    c.beginPath();
    c.moveTo(0, 0);
    c.lineTo(0, -r);
    c.moveTo(0, -r * 0.55);
    c.lineTo(-r * 0.25, -r * 0.8);
    c.moveTo(0, -r * 0.55);
    c.lineTo(r * 0.25, -r * 0.8);
    c.stroke();
  }
  c.restore();
}

/** Draw one seamless tile of `id` into a size×size context. */
function drawTile(c: AnyCtx, id: PatternId, S: number, color: string) {
  c.fillStyle = color;
  c.strokeStyle = color;
  switch (id) {
    case 'dots':
      c.beginPath();
      c.arc(S / 2, S / 2, S * 0.12, 0, Math.PI * 2);
      c.fill();
      break;
    case 'polka':
      c.beginPath();
      c.arc(S * 0.25, S * 0.25, S * 0.1, 0, Math.PI * 2);
      c.arc(S * 0.75, S * 0.75, S * 0.1, 0, Math.PI * 2);
      c.fill();
      break;
    case 'stripes':
      c.lineWidth = S * 0.22;
      c.beginPath();
      for (const k of [-1, 0, 1]) {
        c.moveTo(k * S - S * 0.1, S * 1.1);
        c.lineTo(k * S + S * 1.1, -S * 0.1);
      }
      c.stroke();
      break;
    case 'pinstripe':
      c.fillRect(S * 0.47, 0, S * 0.06, S);
      break;
    case 'grid':
      c.fillRect(0, 0, S, S * 0.035);
      c.fillRect(0, 0, S * 0.035, S);
      break;
    case 'checker':
      c.fillRect(0, 0, S / 2, S / 2);
      c.fillRect(S / 2, S / 2, S / 2, S / 2);
      break;
    case 'gingham':
      c.globalAlpha = 0.5;
      c.fillRect(0, 0, S / 2, S);
      c.fillRect(0, 0, S, S / 2);
      c.globalAlpha = 1;
      break;
    case 'hearts':
      heart(c, S * 0.27, S * 0.3, S * 0.34);
      heart(c, S * 0.77, S * 0.78, S * 0.22);
      break;
    case 'stars':
      star(c, S * 0.28, S * 0.3, S * 0.15);
      star(c, S * 0.76, S * 0.76, S * 0.09);
      break;
    case 'snowflakes':
      snowflake(c, S * 0.3, S * 0.3, S * 0.18);
      snowflake(c, S * 0.76, S * 0.74, S * 0.1);
      c.beginPath();
      c.arc(S * 0.78, S * 0.24, S * 0.025, 0, Math.PI * 2);
      c.arc(S * 0.22, S * 0.8, S * 0.02, 0, Math.PI * 2);
      c.fill();
      break;
    case 'confetti': {
      const rng = mulberry32(7);
      const multi = color === 'multi';
      for (let i = 0; i < 9; i++) {
        c.fillStyle = multi ? CONFETTI[i % CONFETTI.length] : color;
        c.globalAlpha = multi ? 1 : 0.55 + rng.next() * 0.45;
        const x = S * (0.08 + rng.next() * 0.84);
        const y = S * (0.08 + rng.next() * 0.84);
        c.save();
        c.translate(x, y);
        c.rotate(rng.next() * Math.PI);
        if (i % 3 === 0) {
          c.beginPath();
          c.arc(0, 0, S * 0.025, 0, Math.PI * 2);
          c.fill();
        } else c.fillRect(-S * 0.035, -S * 0.012, S * 0.07, S * 0.024);
        c.restore();
      }
      c.globalAlpha = 1;
      break;
    }
    case 'sparkles':
      sparkle(c, S * 0.3, S * 0.32, S * 0.16);
      sparkle(c, S * 0.76, S * 0.74, S * 0.09);
      c.beginPath();
      c.arc(S * 0.78, S * 0.22, S * 0.02, 0, Math.PI * 2);
      c.fill();
      break;
    case 'chevron':
      c.lineWidth = S * 0.12;
      c.lineJoin = 'miter';
      c.beginPath();
      c.moveTo(-S * 0.1, S * 0.7);
      c.lineTo(S * 0.5, S * 0.3);
      c.lineTo(S * 1.1, S * 0.7);
      c.stroke();
      break;
    case 'waves':
      c.lineWidth = S * 0.07;
      c.beginPath();
      for (let i = 0; i <= 40; i++) {
        const x = (i / 40) * S;
        const y = S * 0.5 + Math.sin((i / 40) * Math.PI * 2) * S * 0.16;
        if (i === 0) c.moveTo(x, y);
        else c.lineTo(x, y);
      }
      c.stroke();
      break;
    case 'plus':
      c.fillRect(S * 0.44, S * 0.32, S * 0.12, S * 0.36);
      c.fillRect(S * 0.32, S * 0.44, S * 0.36, S * 0.12);
      break;
    case 'triangles':
      c.beginPath();
      c.moveTo(S * 0.5, S * 0.3);
      c.lineTo(S * 0.7, S * 0.66);
      c.lineTo(S * 0.3, S * 0.66);
      c.closePath();
      c.fill();
      break;
  }
}

const tileCache = new Map<string, AnyCanvas>();

export function patternTile(id: PatternId, color: string, res: number): AnyCanvas {
  res = Math.max(8, Math.min(1024, Math.round(res / 8) * 8));
  const key = `${id}|${color}|${res}`;
  let tile = tileCache.get(key);
  if (!tile) {
    tile = makeCanvas(res, res);
    drawTile(ctx2d(tile), id, res, color);
    if (tileCache.size > 64) tileCache.clear();
    tileCache.set(key, tile);
  }
  return tile;
}

/* ---------------------------------------------------------------- textures */

function valueNoise(size: number, cells: number, seed: number): Float32Array {
  const rng = mulberry32(seed);
  const grid = new Float32Array((cells + 1) * (cells + 1));
  for (let i = 0; i < grid.length; i++) grid[i] = rng.next();
  // Wrap edges so the tile is seamless.
  for (let i = 0; i <= cells; i++) {
    grid[i * (cells + 1) + cells] = grid[i * (cells + 1)];
    grid[cells * (cells + 1) + i] = grid[i];
  }
  const out = new Float32Array(size * size);
  for (let y = 0; y < size; y++) {
    const gy = (y / size) * cells;
    const y0 = Math.floor(gy);
    const fy = gy - y0;
    const sy = fy * fy * (3 - 2 * fy);
    for (let x = 0; x < size; x++) {
      const gx = (x / size) * cells;
      const x0 = Math.floor(gx);
      const fx = gx - x0;
      const sx = fx * fx * (3 - 2 * fx);
      const a = grid[y0 * (cells + 1) + x0];
      const b = grid[y0 * (cells + 1) + x0 + 1];
      const c = grid[(y0 + 1) * (cells + 1) + x0];
      const d = grid[(y0 + 1) * (cells + 1) + x0 + 1];
      out[y * size + x] = a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy;
    }
  }
  return out;
}

const textureCache = new Map<TextureId, AnyCanvas>();

/** A 512 px seamless greyscale tile, meant to be drawn with `multiply`. */
export function textureTile(id: TextureId): AnyCanvas {
  const cached = textureCache.get(id);
  if (cached) return cached;
  const S = 512;
  const canvas = makeCanvas(S, S);
  const c = ctx2d(canvas);
  const img = c.createImageData(S, S);
  const d = img.data;
  const rng = mulberry32(42);
  const coarse = valueNoise(S, 8, 1);
  const mid = valueNoise(S, 32, 2);
  const fine = valueNoise(S, 128, 3);
  for (let y = 0; y < S; y++) {
    for (let x = 0; x < S; x++) {
      const i = y * S + x;
      let v = 1;
      let tint: [number, number, number] = [1, 1, 1];
      switch (id) {
        case 'paper':
          v = 1 - 0.05 * coarse[i] - 0.06 * mid[i] - 0.06 * fine[i] - 0.05 * rng.next();
          break;
        case 'linen': {
          const wx = Math.sin((x / S) * Math.PI * 2 * 96) * 0.5 + 0.5;
          const wy = Math.sin((y / S) * Math.PI * 2 * 96) * 0.5 + 0.5;
          v = 1 - 0.06 * wx * (0.6 + 0.4 * fine[i]) - 0.06 * wy * (0.6 + 0.4 * mid[i]) - 0.04 * coarse[i];
          break;
        }
        case 'kraft':
          v = 1 - 0.1 * coarse[i] - 0.08 * mid[i] - 0.1 * fine[i] - 0.06 * rng.next();
          tint = [0.86, 0.72, 0.55];
          break;
        case 'grain':
          v = 1 - 0.16 * rng.next();
          break;
        case 'watercolor':
          v = 1 - 0.14 * Math.pow(coarse[i], 1.6) - 0.08 * mid[i] - 0.03 * fine[i];
          break;
      }
      const o = i * 4;
      d[o] = Math.round(255 * v * tint[0]);
      d[o + 1] = Math.round(255 * v * tint[1]);
      d[o + 2] = Math.round(255 * v * tint[2]);
      d[o + 3] = 255;
    }
  }
  c.putImageData(img, 0, 0);
  textureCache.set(id, canvas);
  return canvas;
}
