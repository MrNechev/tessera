/**
 * Full-resolution export: renders the document with the same renderer as the
 * editor, tile by tile, using full-quality photos, and streams the pixels to
 * the backend which encodes the final file (JPEG/PNG/WebP/AVIF/TIFF/PDF).
 */
import { api } from '../api/backend';
import type { ExportFormat, ExportResult } from '../api/types';
import { ensureFont } from '../assets/fonts';
import type { CollageDoc } from '../model/types';
import { AdjustCache } from '../render/adjust';
import { photoWindow } from '../render/frameGeometry';
import { coverRect, renderDocument, type ImageEntry, type RenderResources } from '../render/renderer';
import { ctx2d, makeCanvas } from '../render/patterns';
import { stickers } from '../stores/stickers.svelte';
import { pageDoc } from '../model/multipage';

export interface ExportOptions {
  path: string;
  format: ExportFormat;
  quality: number;
  /** Output pixels per page unit (1 = the page's pixel size). */
  scale?: number;
  /** Append " (1)" etc. instead of overwriting. */
  unique?: boolean;
  onProgress?: (fraction: number, stage: string) => void;
  signal?: { cancelled: boolean };
}

const TILE = 2048;

/**
 * Render one page at full resolution into a backend export job and return
 * its id. The caller finishes the job (single image) or adds it to a PDF.
 */
export async function renderToJob(
  doc: CollageDoc,
  opts: { scale?: number; onProgress?: (fraction: number, stage: string) => void; signal?: { cancelled: boolean } },
): Promise<number> {
  const scale = opts.scale ?? 1;
  const W = Math.max(1, Math.round(doc.page.width * scale));
  const H = Math.max(1, Math.round(doc.page.height * scale));
  const progress = opts.onProgress ?? (() => {});

  // 1. Work out how many pixels each photo really needs.
  const need = new Map<string, { w: number; h: number }>();
  const bump = (id: string, w: number, h: number) => {
    const cur = need.get(id) ?? { w: 0, h: 0 };
    need.set(id, { w: Math.max(cur.w, w), h: Math.max(cur.h, h) });
  };
  for (const el of doc.elements) {
    if (el.kind !== 'photo' || !el.asset || el.hidden) continue;
    const a = doc.assets[el.asset];
    if (!a) continue;
    const win = photoWindow(el.frame, el.w, el.h);
    const r = coverRect(a.width, a.height, win, el.crop);
    bump(el.asset, Math.ceil(r.w * scale), Math.ceil(r.h * scale));
  }
  if (doc.background.image) {
    const a = doc.assets[doc.background.image.asset];
    if (a) {
      const r = coverRect(a.width, a.height, { x: 0, y: 0, w: doc.page.width, h: doc.page.height }, { x: 0.5, y: 0.5, zoom: 1 });
      bump(doc.background.image.asset, Math.ceil(r.w * scale), Math.ceil(r.h * scale));
    }
  }

  // 2. Load full-quality pixels.
  const bitmaps = new Map<string, ImageEntry>();
  let i = 0;
  for (const [id, size] of need) {
    if (opts.signal?.cancelled) throw new Error('Export cancelled');
    const a = doc.assets[id];
    progress((0.45 * i) / Math.max(1, need.size), `Loading photo ${i + 1} of ${need.size}`);
    try {
      const data = await api.loadImagePixels(a.path, Math.max(1, size.w), Math.max(1, size.h));
      const bmp = await createImageBitmap(data);
      bitmaps.set(id, { source: bmp, width: bmp.width, height: bmp.height, key: `${id}@export` });
    } catch (e) {
      throw new Error(`Could not load “${a.name}”: ${e}`);
    }
    i++;
  }

  // 3. Fonts and stickers must be ready before drawing.
  const fonts: Promise<void>[] = [];
  const stickerIds: string[] = [];
  for (const el of doc.elements) {
    if (el.kind === 'text') fonts.push(ensureFont(el.font, el.weight));
    if (el.kind === 'photo' && el.frame?.caption) fonts.push(ensureFont(el.frame.captionFont ?? 'caveat', 400));
    if (el.kind === 'sticker') stickerIds.push(el.sticker);
    if (el.kind === 'calendar') fonts.push(ensureFont(el.titleFont, 700), ensureFont(el.font, 400), ensureFont(el.font, 600));
  }
  await Promise.all([...fonts, stickers.preload(stickerIds)]);

  const resources: RenderResources = {
    image: (id) => bitmaps.get(id) ?? null,
    sticker: (id) => stickers.image(id),
    adjustCache: new AdjustCache(6),
  };

  // 4. Render tiles and stream them to the backend.
  const job = await api.exportBegin(W, H);
  try {
    const cols = Math.ceil(W / TILE);
    const rows = Math.ceil(H / TILE);
    let n = 0;
    for (let ty = 0; ty < rows; ty++) {
      for (let tx = 0; tx < cols; tx++) {
        if (opts.signal?.cancelled) throw new Error('Export cancelled');
        const x = tx * TILE;
        const y = ty * TILE;
        const w = Math.min(TILE, W - x);
        const h = Math.min(TILE, H - y);
        const canvas = makeCanvas(w, h);
        const c = ctx2d(canvas);
        c.translate(-x, -y);
        renderDocument(c, doc, {
          scale,
          resources,
          placeholders: false,
          clip: { x: x / scale, y: y / scale, w: w / scale, h: h / scale },
        });
        const pixels = c.getImageData(0, 0, w, h).data;
        await api.exportTile(job, x, y, w, h, pixels);
        n++;
        progress(0.45 + (0.45 * n) / (cols * rows), `Rendering ${n} / ${cols * rows}`);
      }
    }
    progress(0.95, 'Encoding');
    return job;
  } catch (e) {
    await api.exportCancel(job).catch(() => {});
    throw e;
  } finally {
    for (const b of bitmaps.values()) (b.source as ImageBitmap).close?.();
    resources.adjustCache.clear();
  }
}

/** Export the document's active page to one image or PDF file. */
export async function exportDocument(doc: CollageDoc, opts: ExportOptions): Promise<ExportResult> {
  const scale = opts.scale ?? 1;
  const job = await renderToJob(doc, opts);
  return api.exportFinish(job, opts.path, opts.format, opts.quality, Math.round(doc.page.dpi * scale), opts.unique ?? false);
}

/**
 * Export several pages. PDF → one multi-page file at `opts.path`; image
 * formats → one file per page, numbered "<name> - page 01.jpg".
 */
export async function exportPages(doc: CollageDoc, pages: number[], opts: ExportOptions): Promise<ExportResult[]> {
  const scale = opts.scale ?? 1;
  const dpi = Math.round(doc.page.dpi * scale);
  const progress = opts.onProgress ?? (() => {});
  const per = (k: number) => (f: number, stage: string) => progress((k + f) / pages.length, `Page ${k + 1} of ${pages.length}: ${stage}`);
  if (opts.format === 'pdf') {
    const pdf = await api.pdfBegin();
    try {
      for (const [k, i] of pages.entries()) {
        const job = await renderToJob(pageDoc(doc, i), { ...opts, onProgress: per(k) });
        await api.pdfAddPage(pdf, job, opts.quality, dpi);
      }
      progress(0.98, 'Writing PDF');
      return [await api.pdfFinish(pdf, opts.path, opts.unique ?? false)];
    } catch (e) {
      await api.pdfCancel(pdf).catch(() => {});
      throw e;
    }
  }
  const dot = opts.path.lastIndexOf('.');
  const stem = dot > opts.path.lastIndexOf('/') ? opts.path.slice(0, dot) : opts.path;
  const ext = dot > opts.path.lastIndexOf('/') ? opts.path.slice(dot) : `.${extensionFor(opts.format)}`;
  const digits = Math.max(2, String(pages.length).length);
  const out: ExportResult[] = [];
  for (const [k, i] of pages.entries()) {
    const path = pages.length === 1 ? opts.path : `${stem} - page ${String(i + 1).padStart(digits, '0')}${ext}`;
    out.push(await exportDocument(pageDoc(doc, i), { ...opts, path, onProgress: per(k) }));
  }
  return out;
}

/** Small JPEG data URL used for project thumbnails on the home screen. */
export async function renderThumbnail(doc: CollageDoc, resources: RenderResources, maxSide = 480): Promise<string> {
  const s = maxSide / Math.max(doc.page.width, doc.page.height);
  const canvas = document.createElement('canvas');
  canvas.width = Math.max(1, Math.round(doc.page.width * s));
  canvas.height = Math.max(1, Math.round(doc.page.height * s));
  const c = canvas.getContext('2d')!;
  renderDocument(c, doc, { scale: s, resources, placeholders: true });
  return canvas.toDataURL('image/jpeg', 0.82);
}

export function extensionFor(format: ExportFormat): string {
  return format === 'jpeg' ? 'jpg' : format === 'tiff' ? 'tif' : format;
}

/** Expand the filename pattern from settings. */
export function buildFilename(pattern: string, name: string, format: ExportFormat): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  const date = `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const time = `${pad(d.getHours())}-${pad(d.getMinutes())}-${pad(d.getSeconds())}`;
  const base = (pattern || '{name}')
    .replaceAll('{name}', name)
    .replaceAll('{date}', date)
    .replaceAll('{time}', time)
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, '-')
    .replace(/\s+/g, ' ')
    .trim();
  return `${base || 'collage'}.${extensionFor(format)}`;
}

export function joinPath(dir: string, file: string): string {
  return dir.endsWith('/') ? dir + file : `${dir}/${file}`;
}
