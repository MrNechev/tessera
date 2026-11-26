/**
 * Paint documents ↔ bytes: OpenRaster files (layers as PNG), images from
 * blobs, thumbnails. Layer PNGs are cached per canvas and revision, so an
 * autosave only re-encodes the layers that changed.
 */
import type { TextElement } from '../model/types';
import { ctxOf, newCanvas, PaintDoc, type Layer } from './doc.svelte';
import { buildOra, parseOra, type OraLayer } from './lib/ora';

export async function canvasBlob(c: HTMLCanvasElement, type = 'image/png', quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => c.toBlob((b) => (b ? resolve(b) : reject(new Error('Could not encode the image'))), type, quality));
}

export async function canvasPng(c: HTMLCanvasElement): Promise<Uint8Array> {
  return new Uint8Array(await (await canvasBlob(c)).arrayBuffer());
}

/** Decode an image blob (PNG, JPEG, WebP, GIF…) into a canvas. */
export async function blobCanvas(blob: Blob): Promise<HTMLCanvasElement> {
  if (typeof createImageBitmap === 'function') {
    try {
      const bmp = await createImageBitmap(blob, { premultiplyAlpha: 'none', colorSpaceConversion: 'default' } as ImageBitmapOptions);
      const c = newCanvas(bmp.width, bmp.height);
      ctxOf(c).drawImage(bmp, 0, 0);
      bmp.close();
      return c;
    } catch {
      /* fall back to <img> */
    }
  }
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    const c = newCanvas(img.naturalWidth, img.naturalHeight);
    ctxOf(c).drawImage(img, 0, 0);
    return c;
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function imageDataCanvas(d: ImageData): HTMLCanvasElement {
  const c = newCanvas(d.width, d.height);
  ctxOf(c).putImageData(d, 0, 0);
  return c;
}

/** Scale a canvas down (halving steps first, so it doesn't alias). */
export function scaledCanvas(src: HTMLCanvasElement, w: number, h: number): HTMLCanvasElement {
  let cur = src;
  while (cur.width / 2 >= w && cur.height / 2 >= h) {
    const half = newCanvas(cur.width / 2, cur.height / 2);
    const c = ctxOf(half);
    c.imageSmoothingQuality = 'high';
    c.drawImage(cur, 0, 0, half.width, half.height);
    cur = half;
  }
  const out = newCanvas(w, h);
  const c = ctxOf(out);
  c.imageSmoothingQuality = 'high';
  c.drawImage(cur, 0, 0, w, h);
  return out;
}

/** A small rendition of the image (at most `max` px on the long side). */
export function thumbCanvas(src: HTMLCanvasElement, max = 256): HTMLCanvasElement {
  const s = Math.min(1, max / Math.max(src.width, src.height));
  return scaledCanvas(src, Math.max(1, Math.round(src.width * s)), Math.max(1, Math.round(src.height * s)));
}

const pngCache = new WeakMap<HTMLCanvasElement, { rev: number; png: Uint8Array }>();

async function layerPng(c: HTMLCanvasElement, rev: number): Promise<Uint8Array> {
  const hit = pngCache.get(c);
  if (hit && hit.rev === rev) return hit.png;
  const png = await canvasPng(c);
  pngCache.set(c, { rev, png });
  return png;
}

/** The document as an OpenRaster file. */
export async function encodeOra(doc: PaintDoc): Promise<Uint8Array<ArrayBuffer>> {
  doc.commitFloating();
  const layers = doc.layers;
  const top = [...layers].reverse();
  const ora: OraLayer[] = [];
  for (const l of top) {
    const c = doc.canvas(l.id);
    ora.push({
      name: l.name,
      png: await layerPng(c, l.rev),
      x: 0,
      y: 0,
      opacity: l.opacity,
      visible: l.visible,
      locked: l.locked,
      blend: l.blend,
      selected: l.id === doc.activeId,
      ...(l.text ? { text: JSON.stringify(l.text) } : {}),
    });
  }
  const merged = await canvasPng(doc.composite);
  const thumb = await canvasPng(thumbCanvas(doc.composite, 256));
  return buildOra({ width: doc.width, height: doc.height, layers: ora }, merged, thumb);
}

/** Open an OpenRaster file as a document (`id`: keep the stored drawing's id). */
export async function decodeOra(bytes: Uint8Array, id?: string): Promise<PaintDoc> {
  const ora = parseOra(bytes);
  const doc = new PaintDoc(ora.width, ora.height, id);
  const layers: Layer[] = [];
  let active: string | undefined;
  // stack.xml lists the top layer first; Paint keeps the bottom first.
  for (const l of [...ora.layers].reverse()) {
    const img = await blobCanvas(new Blob([l.png as Uint8Array<ArrayBuffer>], { type: 'image/png' }));
    const c = newCanvas(ora.width, ora.height);
    ctxOf(c).drawImage(img, l.x, l.y);
    const layer = doc.makeLayer(l.name, c);
    layer.visible = l.visible;
    layer.locked = l.locked;
    layer.opacity = l.opacity;
    layer.blend = l.blend;
    if (l.text) {
      try {
        layer.text = JSON.parse(l.text) as TextElement;
      } catch {
        /* plain pixels then */
      }
    }
    if (l.selected) active = layer.id;
    layers.push(layer);
  }
  if (!layers.length) layers.push(doc.makeLayer('Background'));
  doc.initLayers(layers, active);
  return doc;
}
