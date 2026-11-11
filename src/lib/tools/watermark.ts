/**
 * Watermark helpers. Text watermarks are rendered here (with the bundled
 * fonts) into a PNG that the backend composites; placement math mirrors
 * `src-tauri/src/batch_tools.rs` so the preview matches the output.
 */
import type { ResizeMode } from '../api/types';
import { ensureFont } from '../assets/fonts';
import { createText } from '../model/factory';
import { drawText, layoutText, naturalTextWidth } from '../render/text';

export type WatermarkPosition = 'tl' | 't' | 'tr' | 'l' | 'c' | 'r' | 'bl' | 'b' | 'br' | 'tile';

export interface TextWatermark {
  text: string;
  font: string;
  weight: number;
  color: string;
  outline: boolean;
  shadow: boolean;
}

/** Size of the output image for a resize mode (never enlarges), like the Rust side. */
export function resizedSize(w: number, h: number, mode: ResizeMode): [number, number] {
  let s = 1;
  if (mode.kind === 'percent') s = Math.min(1, mode.percent / 100);
  if (mode.kind === 'longEdge') s = Math.min(1, mode.pixels / Math.max(w, h));
  if (mode.kind === 'fit') {
    const sw = mode.width > 0 ? mode.width / w : Infinity;
    const sh = mode.height > 0 ? mode.height / h : Infinity;
    s = Math.min(1, sw, sh);
  }
  return [Math.max(1, Math.round(w * s)), Math.max(1, Math.round(h * s))];
}

/** Watermark box size on an image of size (w, h). */
export function watermarkSize(w: number, h: number, markW: number, markH: number, size: number): [number, number] {
  const aspect = markW / markH;
  let ww = Math.max(1, Math.round(Math.min(1, Math.max(0.01, size)) * w));
  let wh = Math.max(1, Math.round(ww / aspect));
  if (wh > h * 0.9) {
    wh = Math.max(1, Math.floor(h * 0.9));
    ww = Math.max(1, Math.round(wh * aspect));
  }
  return [ww, wh];
}

export function placements(w: number, h: number, ww: number, wh: number, pos: WatermarkPosition, margin: number): [number, number][] {
  const m = Math.round(margin * Math.min(w, h));
  const cx = Math.trunc((w - ww) / 2);
  const cy = Math.trunc((h - wh) / 2);
  const right = w - ww - m;
  const bottom = h - wh - m;
  switch (pos) {
    case 'tl':
      return [[m, m]];
    case 't':
      return [[cx, m]];
    case 'tr':
      return [[right, m]];
    case 'l':
      return [[m, cy]];
    case 'c':
      return [[cx, cy]];
    case 'r':
      return [[right, cy]];
    case 'bl':
      return [[m, bottom]];
    case 'b':
      return [[cx, bottom]];
    case 'br':
      return [[right, bottom]];
    case 'tile': {
      const sx = Math.max(1, Math.trunc(ww * 1.6));
      const sy = Math.max(1, Math.trunc(wh * 3));
      const out: [number, number][] = [];
      let row = 0;
      for (let y = -Math.trunc(wh / 2); y < h; y += sy, row++) {
        const offset = row % 2 === 1 ? Math.trunc(sx / 2) : 0;
        for (let x = -Math.trunc(ww / 2) - offset; x < w; x += sx) out.push([x, y]);
      }
      return out;
    }
  }
}

/** Render a text watermark to a transparent PNG canvas (large, so it stays sharp when scaled). */
export async function renderTextWatermark(t: TextWatermark): Promise<HTMLCanvasElement> {
  await ensureFont(t.font, t.weight);
  const size = 220;
  const el = createText({
    x: 0,
    y: 0,
    w: 10,
    text: t.text.trim() || '©',
    font: t.font,
    size,
    weight: t.weight,
    fill: t.color,
    align: 'center',
    stroke: t.outline ? { width: size * 0.06, color: luminanceOf(t.color) > 0.5 ? '#000000' : '#ffffff' } : null,
    shadow: t.shadow ? { color: 'rgba(0,0,0,0.55)', blur: 0.03, x: 0, y: 0.012 } : null,
  });
  el.w = naturalTextWidth(el) + size * 0.2;
  const L = layoutText(el);
  el.h = L.height;
  const pad = Math.round(size * 0.25);
  const canvas = document.createElement('canvas');
  canvas.width = Math.ceil(el.w + pad * 2);
  canvas.height = Math.ceil(el.h + pad * 2);
  const c = canvas.getContext('2d')!;
  c.translate(pad, pad);
  // Shadow sizes are relative to a "page short side"; use the canvas height.
  drawText(c, el, { scale: 1, pageShort: canvas.height });
  return trim(canvas);
}

function luminanceOf(hex: string): number {
  const m = /^#?([0-9a-f]{6})/i.exec(hex);
  if (!m) return 0.5;
  const n = parseInt(m[1], 16);
  return (0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)) / 255;
}

/** Crop fully transparent borders so the watermark size matches what you see. */
function trim(canvas: HTMLCanvasElement): HTMLCanvasElement {
  const c = canvas.getContext('2d')!;
  const { width: w, height: h } = canvas;
  const d = c.getImageData(0, 0, w, h).data;
  let x0 = w;
  let y0 = h;
  let x1 = -1;
  let y1 = -1;
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++)
      if (d[(y * w + x) * 4 + 3] > 4) {
        if (x < x0) x0 = x;
        if (x > x1) x1 = x;
        if (y < y0) y0 = y;
        if (y > y1) y1 = y;
      }
  if (x1 < 0) return canvas;
  const out = document.createElement('canvas');
  out.width = x1 - x0 + 1;
  out.height = y1 - y0 + 1;
  out.getContext('2d')!.drawImage(canvas, -x0, -y0);
  return out;
}

export async function canvasToPng(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob((b) => (b ? resolve(b) : reject(new Error('PNG encoding failed'))), 'image/png'));
  return new Uint8Array(await blob.arrayBuffer());
}
