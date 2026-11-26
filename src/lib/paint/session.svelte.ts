/**
 * Opening, saving and exporting Paint documents — and the small API other
 * parts of Tessera use to send an image to Paint:
 *
 * - `openPaint(source)` — a library file, an engine path or a blob becomes
 *   the base layer of a new drawing (libraries stay read-only). An image
 *   that was exported from a drawing reopens that layered drawing instead.
 * - `openPaintDoc(id)` — reopen a saved drawing.
 * - `newPaint(options)`, `newPaintFromClipboard()`.
 *
 * Drawings autosave (debounced) as OpenRaster into the app's project library;
 * exports go to Outputs/Drawings as new files.
 */
import { api, deliver, isTauri } from '../api/backend';
import { call } from '../api/http';
import type { Area } from '../api/types';
import { picker } from '../platform/picker.svelte';
import { app } from '../stores/app.svelte';
import { toasts } from '../stores/toasts.svelte';
import { ctxOf, newCanvas, PaintDoc } from './doc.svelte';
import { blobCanvas, canvasBlob, decodeOra, encodeOra, imageDataCanvas, scaledCanvas } from './io';
import { capArea, fitView } from './lib/geom';
import { drawings } from './lib/store';
import { paint } from './state.svelte';

/** Working size limit (pixels); bigger images are offered a downscale. */
export const MAX_PIXELS = 16_000_000;
/** Longest side fetched from the engine when opening a photo. */
const LOAD_SIDE = 8192;
/** Where exports go, inside Outputs. */
export const DRAWINGS_DIR = 'Drawings';
const SAVE_DELAY = 2500;
const TILE = 2048;

export type PaintSource =
  | { libraryId: string; path: string; name?: string }
  | { path: string; name?: string }
  | { blob: Blob; name?: string };

export interface NewPaintOptions {
  width: number;
  height: number;
  /** 'white', 'transparent' or a colour. */
  background: string;
  name?: string;
}

function stem(name: string): string {
  const base = name.split('/').pop() ?? name;
  const dot = base.lastIndexOf('.');
  return (dot > 0 ? base.slice(0, dot) : base).trim() || 'Untitled drawing';
}

/** Show a document in the Paint view (pending saves of the previous one first). */
async function show(doc: PaintDoc, saveNow = false): Promise<void> {
  await flushPaint();
  paint.doc?.cancelOverlay();
  paint.textEdit = null;
  paint.crop = null;
  doc.onEdit = scheduleSave;
  doc.savedRev = doc.editRev;
  paint.doc = doc;
  paint.saving = 'idle';
  paint.fitPending = true;
  app.view = 'paint';
  if (saveNow) await saveNow_(doc);
}

/** A blank drawing. */
export async function newPaint(o: NewPaintOptions): Promise<PaintDoc> {
  const w = Math.max(1, Math.min(16384, Math.round(o.width)));
  const h = Math.max(1, Math.min(16384, Math.round(o.height)));
  const doc = new PaintDoc(w, h);
  doc.name = o.name ?? 'Untitled drawing';
  const bg = doc.makeLayer('Background');
  if (o.background !== 'transparent') {
    const c = ctxOf(doc.canvas(bg.id));
    c.fillStyle = o.background === 'white' ? '#ffffff' : o.background;
    c.fillRect(0, 0, w, h);
  }
  doc.initLayers([bg]);
  await show(doc);
  return doc;
}

/** Offer a downscale for images over the working size. Returns null if cancelled. */
async function capped(c: HTMLCanvasElement): Promise<HTMLCanvasElement | null> {
  const cap = capArea(c.width, c.height, MAX_PIXELS);
  if (!cap.scaled) return c;
  const mp = ((c.width * c.height) / 1e6).toFixed(0);
  const choice = await paint.choose(
    'This image is very large',
    `It is ${c.width} × ${c.height} (${mp} MP). Painting is smoother at ${cap.w} × ${cap.h} (16 MP), and exports at that size still print well.`,
    [
      { id: 'keep', label: 'Keep full size' },
      { id: 'scale', label: `Work at ${cap.w} × ${cap.h}`, primary: true },
    ],
  );
  if (!choice) return null;
  return choice === 'scale' ? scaledCanvas(c, cap.w, cap.h) : c;
}

/** A drawing whose base layer is this image. */
export async function newPaintFromCanvas(src: HTMLCanvasElement, name: string, source: string | null = null): Promise<PaintDoc | null> {
  const c = await capped(src);
  if (!c) return null;
  const doc = new PaintDoc(c.width, c.height);
  doc.name = name;
  doc.source = source;
  const base = doc.makeLayer('Image', c);
  doc.initLayers([base]);
  await show(doc);
  return doc;
}

function isOra(name: string | undefined): boolean {
  return !!name && /\.ora$/i.test(name);
}

/**
 * Open an image in Paint as the base layer of a new drawing. Images that were
 * exported from a drawing reopen that drawing (with its layers).
 */
export async function openPaint(src: PaintSource): Promise<PaintDoc | null> {
  paint.busy = 'Opening…';
  try {
    if ('blob' in src) {
      if (isOra(src.name) || (await sniffZip(src.blob))) return await openOraBytes(new Uint8Array(await src.blob.arrayBuffer()), src.name);
      const c = await blobCanvas(src.blob);
      return await newPaintFromCanvas(c, stem(src.name ?? 'Pasted image'));
    }
    const name = src.name ?? src.path.split('/').pop() ?? 'Image';
    const abs = 'libraryId' in src ? await call<string>('resolve', { libraryId: src.libraryId, path: src.path }) : src.path;
    const linked = await drawings.forExport(abs).catch(() => null);
    if (linked) {
      const doc = await openPaintDoc(linked);
      if (doc) toasts.push({ kind: 'info', title: 'Opened the drawing with its layers', message: `“${name}” was exported from “${doc.name}”.` });
      return doc;
    }
    if (isOra(name)) {
      const res = await fetch(`/api/t/file?path=${encodeURIComponent(abs)}`, { credentials: 'same-origin' });
      if (!res.ok) throw new Error(`Could not read ${name} (${res.status})`);
      return await openOraBytes(new Uint8Array(await res.arrayBuffer()), name);
    }
    const px = await api.loadImagePixels(abs, LOAD_SIDE, LOAD_SIDE);
    return await newPaintFromCanvas(imageDataCanvas(px), stem(name), abs);
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not open this image in Paint', message: String(e instanceof Error ? e.message : e) });
    return null;
  } finally {
    paint.busy = null;
  }
}

async function sniffZip(b: Blob): Promise<boolean> {
  const head = new Uint8Array(await b.slice(0, 4).arrayBuffer());
  return head[0] === 0x50 && head[1] === 0x4b && head[2] === 3 && head[3] === 4;
}

/** An .ora file from anywhere (a new drawing in Tessera's store). */
export async function openOraBytes(bytes: Uint8Array, name?: string): Promise<PaintDoc> {
  const doc = await decodeOra(bytes);
  doc.name = stem(name ?? 'Drawing');
  await show(doc, true);
  return doc;
}

/** Reopen a saved drawing. */
export async function openPaintDoc(id: string): Promise<PaintDoc | null> {
  if (paint.doc?.id === id) {
    app.view = 'paint';
    return paint.doc;
  }
  paint.busy = 'Opening…';
  try {
    const [bytes, list] = await Promise.all([drawings.load(id), drawings.list().catch(() => [])]);
    const doc = await decodeOra(bytes, id);
    doc.name = list.find((d) => d.id === id)?.name ?? 'Drawing';
    await show(doc);
    return doc;
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not open the drawing', message: String(e instanceof Error ? e.message : e) });
    return null;
  } finally {
    paint.busy = null;
  }
}

/** The image on the system clipboard (async Clipboard API), if any. */
export async function clipboardImage(): Promise<Blob | null> {
  const clip = navigator.clipboard as Clipboard | undefined;
  if (!clip?.read) return null;
  try {
    for (const item of await clip.read()) {
      const type = item.types.find((t) => t.startsWith('image/'));
      if (type) return await item.getType(type);
    }
  } catch {
    /* denied or unsupported */
  }
  return null;
}

export async function newPaintFromClipboard(): Promise<PaintDoc | null> {
  const blob = await clipboardImage();
  if (!blob) {
    toasts.push({ kind: 'info', title: 'No image on the clipboard', message: 'Copy an image first — or open Paint and press Ctrl+V.' });
    return null;
  }
  return openPaint({ blob, name: 'Pasted image' });
}

/* ------------------------------------------------------------ saving */

let timer: ReturnType<typeof setTimeout> | null = null;
let chain: Promise<void> = Promise.resolve();
let warned = false;

export function scheduleSave(): void {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    const doc = paint.doc;
    if (doc) void saveNow_(doc, false);
  }, SAVE_DELAY);
}

function saveNow_(doc: PaintDoc, force = true): Promise<void> {
  // Don't drop pixels that are being moved, or interrupt a stroke, just to autosave.
  if (!force && (doc.floating || doc.overlay)) {
    scheduleSave();
    return chain;
  }
  const rev = doc.editRev;
  const job = chain.then(async () => {
    if (paint.doc === doc) paint.saving = 'saving';
    try {
      const bytes = await encodeOra(doc);
      await drawings.save(doc.id, doc.name, bytes);
      doc.savedRev = Math.max(doc.savedRev, rev);
      if (paint.doc === doc) paint.saving = doc.dirty ? 'idle' : 'saved';
      warned = false;
    } catch (e) {
      console.warn('paint save failed', e);
      if (paint.doc === doc) paint.saving = 'error';
      if (!warned) {
        warned = true;
        toasts.push({ kind: 'error', title: 'Could not save your drawing', message: String(e instanceof Error ? e.message : e) });
      }
    }
  });
  chain = job;
  return job;
}

/** Save the open drawing now ("Save progress", Ctrl+S). */
export function saveNow(): Promise<void> {
  if (timer) clearTimeout(timer);
  timer = null;
  const doc = paint.doc;
  if (!doc) return chain;
  return saveNow_(doc);
}

/** Write pending changes (before switching documents, closing the window…). */
export function flushPaint(): Promise<void> {
  const doc = paint.doc;
  if (timer || (doc && doc.dirty)) return saveNow();
  return chain;
}

/** Close the drawing (saved first). */
export async function closePaint(): Promise<void> {
  await flushPaint();
  paint.doc = null;
  paint.textEdit = null;
  paint.crop = null;
}

/** Forget the open drawing without saving (it was deleted). */
export function dropPaint(): void {
  if (timer) clearTimeout(timer);
  timer = null;
  paint.doc = null;
}

/* ------------------------------------------------------------ export */

export type ExportFormat = 'png' | 'jpeg' | 'webp';

export interface ExportResult {
  path: string;
  name: string;
  ref: { libraryId: string; path: string };
  width: number;
  height: number;
}

type ExportListener = (r: ExportResult & { drawingId: string }) => void;
const exportListeners = new Set<ExportListener>();

/**
 * Be told whenever a drawing is exported (e.g. to put the new image into a
 * story or a canvas that sent it to Paint). Returns the unsubscribe function.
 */
export function onPaintExport(fn: ExportListener): () => void {
  exportListeners.add(fn);
  return () => exportListeners.delete(fn);
}

export function exportFileName(doc: PaintDoc, format: ExportFormat): string {
  const safe = doc.name.replace(/[\\/:*?"<>|\u0000-\u001f]/g, '_').replace(/^[.\s]+/, '').trim() || 'Drawing';
  return `${safe}.${format === 'jpeg' ? 'jpg' : format}`;
}

async function outputs(): Promise<Area> {
  const areas = await call<Area[]>('areas');
  const out = areas.find((a) => a.kind === 'outputs');
  if (!out) throw new Error('There is no Outputs folder. Choose a save folder in Settings → Saving first.');
  return out;
}

/** The flattened drawing at `scale`, ready to encode (JPEG gets a white background). */
function exportCanvas(doc: PaintDoc, format: ExportFormat, scale = 1): HTMLCanvasElement {
  const flat = doc.flattened(format === 'jpeg' ? '#ffffff' : null);
  if (scale === 1) return flat;
  return scaledCanvas(flat, Math.max(1, Math.round(flat.width * scale)), Math.max(1, Math.round(flat.height * scale)));
}

/**
 * Save the drawing as a new image in Outputs/Drawings (never overwriting),
 * remember which drawing it came from, and save the drawing itself.
 */
export async function exportDrawing(format: ExportFormat, quality: number, scale = 1, download = false): Promise<ExportResult | null> {
  const doc = paint.doc;
  if (!doc) return null;
  paint.busy = 'Exporting…';
  try {
    const out = await outputs();
    const root = out.root.replace(/\/+$/, '');
    const c = exportCanvas(doc, format, scale);
    const w = c.width;
    const h = c.height;
    const rgba = ctxOf(c).getImageData(0, 0, w, h).data;
    const job = await api.exportBegin(w, h);
    let saved;
    try {
      for (let y = 0; y < h; y += TILE) {
        for (let x = 0; x < w; x += TILE) {
          const tw = Math.min(TILE, w - x);
          const th = Math.min(TILE, h - y);
          const tile = new Uint8ClampedArray(tw * th * 4);
          for (let r = 0; r < th; r++) tile.set(rgba.subarray(((y + r) * w + x) * 4, ((y + r) * w + x + tw) * 4), r * tw * 4);
          await api.exportTile(job, x, y, tw, th, tile);
        }
      }
      await api.ensureDir(`${root}/${DRAWINGS_DIR}`);
      saved = await api.exportFinish(job, `${root}/${DRAWINGS_DIR}/${exportFileName(doc, format)}`, format, Math.round(quality), 72, true);
    } catch (e) {
      await api.exportCancel(job).catch(() => {});
      throw e;
    }
    await saveNow();
    await drawings.linkExport(doc.id, saved.path).catch(() => undefined);
    const name = saved.path.split('/').pop() ?? '';
    const ref = { libraryId: out.id, path: saved.path.startsWith(root + '/') ? saved.path.slice(root.length + 1) : `${DRAWINGS_DIR}/${name}` };
    toasts.push({
      kind: 'success',
      title: 'Image saved',
      message: `Outputs › ${DRAWINGS_DIR} › ${name} · ${w}×${h}`,
      action: isTauri ? { label: 'Show in folder', run: () => void api.revealPath(saved.path) } : { label: 'Download', run: () => downloadNow(saved.path) },
    });
    if (download && !isTauri) downloadNow(saved.path);
    else if (!isTauri) deliver([saved.path]);
    const result = { path: saved.path, name, ref, width: w, height: h };
    for (const l of exportListeners) {
      try {
        l({ ...result, drawingId: doc.id });
      } catch (e) {
        console.warn('paint export listener failed', e);
      }
    }
    return result;
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Export failed', message: String(e instanceof Error ? e.message : e) });
    return null;
  } finally {
    paint.busy = null;
  }
}

function downloadNow(path: string): void {
  const auto = picker.downloadResults;
  picker.downloadResults = true;
  deliver([path]);
  picker.downloadResults = auto;
}

/** Browser: download the image straight from this page (nothing is saved on the server). */
export async function downloadDrawing(format: ExportFormat, quality: number, scale = 1): Promise<void> {
  const doc = paint.doc;
  if (!doc) return;
  const c = exportCanvas(doc, format, scale);
  const type = format === 'jpeg' ? 'image/jpeg' : format === 'webp' ? 'image/webp' : 'image/png';
  const blob = await canvasBlob(c, type, quality / 100);
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = exportFileName(doc, blob.type === type ? format : 'png');
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

/** Add an image as a new layer of the open drawing (fitted inside it). */
export function addImageLayer(src: HTMLCanvasElement, name: string): void {
  const d = paint.doc;
  if (!d) return;
  d.addLayer(stem(name), fitInto(src, d.width, d.height), 'Add image');
}

/** Files dropped on Paint (browser): open the first, or add them as layers. */
export async function paintDropFiles(files: File[]): Promise<void> {
  const imgs = files.filter((f) => f.type.startsWith('image/') || isOra(f.name));
  if (!imgs.length) return;
  if (!paint.doc || isOra(imgs[0].name)) {
    await openPaint({ blob: imgs[0], name: imgs[0].name });
    return;
  }
  for (const f of imgs) addImageLayer(await blobCanvas(f), f.name);
}

/** Paths dropped on Paint (desktop): open the first, or add them as layers. */
export async function paintDropPaths(paths: string[]): Promise<void> {
  if (!paths.length) return;
  if (!paint.doc || isOra(paths[0])) {
    await openPaint({ path: paths[0] });
    return;
  }
  for (const p of paths) {
    try {
      addImageLayer(imageDataCanvas(await api.loadImagePixels(p, LOAD_SIDE, LOAD_SIDE)), p.split('/').pop() ?? 'Image');
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not add this image', message: String(e instanceof Error ? e.message : e) });
    }
  }
}

/** Fit the drawing into a viewport of this size. */
export function fitTo(vw: number, vh: number, upscale = false): void {
  const d = paint.doc;
  if (!d || vw <= 0 || vh <= 0) return;
  paint.view = fitView(d.width, d.height, vw, vh, 28, upscale);
}

/** A canvas with these pixels at the drawing's size (new layer from an image). */
export function fitInto(src: HTMLCanvasElement, w: number, h: number): HTMLCanvasElement {
  const s = Math.min(1, w / src.width, h / src.height);
  const out = newCanvas(w, h);
  const dw = Math.round(src.width * s);
  const dh = Math.round(src.height * s);
  const c = ctxOf(out);
  c.imageSmoothingQuality = 'high';
  c.drawImage(s < 1 ? scaledCanvas(src, dw, dh) : src, Math.round((w - dw) / 2), Math.round((h - dh) / 2));
  return out;
}
