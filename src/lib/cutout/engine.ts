/**
 * Main-thread side of cut-outs (loaded only when the dialog opens): photo
 * pixels from the engine, the segmentation worker, and saving the result as
 * a transparent PNG into Outputs/Cut-outs through the export job API (the
 * same write path — and write guard — as collage exports).
 */
import { api } from '../api/backend';
import { call } from '../api/http';
import type { Area } from '../api/types';
import { api as docApi } from '../documents/lib/api';
import { ctx2d, makeCanvas, type AnyCanvas } from '../render/patterns';
import { MODEL_SIZE, MODEL_URL } from './model';
import { cutoutName } from './names';
import type { WorkerReply, WorkerRequest } from './cutout.worker';
import { CUTOUTS_DIR, type CutoutResult } from './open.svelte';

/** Longest side of a saved cut-out: plenty for a sticker, and the mask is 320 px anyway. */
export const MAX_SIDE = 4096;
const TILE = 2048;
/** The worker (≈ 15 MB of WebAssembly + the model) is dropped after this long unused. */
const IDLE_MS = 120_000;

let worker: Worker | null = null;
let idle: ReturnType<typeof setTimeout> | null = null;
let nextId = 0;

function getWorker(): Worker {
  if (idle) clearTimeout(idle);
  idle = null;
  worker ??= new Worker(new URL('./cutout.worker.ts', import.meta.url), { type: 'module', name: 'cutout' });
  return worker;
}

function releaseLater(): void {
  if (idle) clearTimeout(idle);
  idle = setTimeout(() => {
    worker?.terminate();
    worker = null;
  }, IDLE_MS);
}

/** Shrink by halving first, so large photos don't alias when drawn small (WebKit's "high" smoothing is a single bilinear step). */
export function scaleTo(src: AnyCanvas | ImageBitmap, w: number, h: number): AnyCanvas {
  let cur: AnyCanvas | ImageBitmap = src;
  let cw = src.width;
  let ch = src.height;
  while (cw / 2 >= w && ch / 2 >= h) {
    const next = makeCanvas(Math.round(cw / 2), Math.round(ch / 2));
    const c = ctx2d(next);
    c.imageSmoothingQuality = 'high';
    c.drawImage(cur, 0, 0, next.width, next.height);
    cur = next;
    cw = next.width;
    ch = next.height;
  }
  const out = makeCanvas(w, h);
  const c = ctx2d(out);
  c.imageSmoothingQuality = 'high';
  c.drawImage(cur, 0, 0, w, h);
  return out;
}

/** A canvas holding these pixels. */
export function canvasOf(img: ImageData): AnyCanvas {
  const c = makeCanvas(img.width, img.height);
  ctx2d(c).putImageData(img, 0, 0);
  return c;
}

/** Upright pixels of a photo on the engine's machine, at most MAX_SIDE on the long side. */
export function loadPhoto(path: string): Promise<ImageData> {
  return api.loadImagePixels(path, MAX_SIDE, MAX_SIDE);
}

/** The model's mask (MODEL_SIZE², 0..1) for this photo. */
export function segment(photo: AnyCanvas, onProgress: (stage: string, fraction: number) => void): Promise<Float32Array> {
  const small = scaleTo(photo, MODEL_SIZE, MODEL_SIZE);
  const rgba = ctx2d(small).getImageData(0, 0, MODEL_SIZE, MODEL_SIZE).data;
  const id = ++nextId;
  const w = getWorker();
  return new Promise<Float32Array>((resolve, reject) => {
    const done = () => {
      w.removeEventListener('message', onMessage);
      w.removeEventListener('error', onError);
      releaseLater();
    };
    const onMessage = (e: MessageEvent<WorkerReply>) => {
      const m = e.data;
      if (m.id !== id) return;
      if (m.type === 'progress') onProgress(m.stage, m.fraction);
      else {
        done();
        if (m.type === 'mask') resolve(m.mask);
        else reject(new Error(m.message));
      }
    };
    const onError = (e: ErrorEvent) => {
      done();
      // A worker that fails to even load is useless; start fresh next time.
      worker?.terminate();
      worker = null;
      reject(new Error(e.message || 'The cut-out engine could not start'));
    };
    w.addEventListener('message', onMessage);
    w.addEventListener('error', onError);
    const req: WorkerRequest = { type: 'segment', id, modelUrl: MODEL_URL, rgba };
    w.postMessage(req, [rgba.buffer]);
  });
}

/** The Outputs area (where results go), or a readable error. */
export async function outputsArea(): Promise<Area> {
  const areas = await call<Area[]>('areas');
  const out = areas.find((a) => a.kind === 'outputs');
  if (!out) throw new Error('There is no Outputs folder. Choose a save folder in Settings → Saving first.');
  return out;
}

/**
 * Save straight RGBA as a PNG in Outputs/Cut-outs (a new file, never
 * overwriting) and wait until the library has indexed it, so stories and
 * the Stickers panel can use it right away.
 */
export async function saveCutout(rgba: Uint8ClampedArray, w: number, h: number, sourceName: string): Promise<CutoutResult> {
  const out = await outputsArea();
  const root = out.root.replace(/\/+$/, '');
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
    // dpi only goes into the PNG's pHYs chunk; 72 = "screen".
    saved = await api.exportFinish(job, `${root}/${CUTOUTS_DIR}/${cutoutName(sourceName)}`, 'png', 100, 72, true);
  } catch (e) {
    await api.exportCancel(job).catch(() => {});
    throw e;
  }
  const ref = { libraryId: out.id, path: saved.path.startsWith(root + '/') ? saved.path.slice(root.length + 1) : `${CUTOUTS_DIR}/${saved.path.split('/').pop()}` };
  // The library watcher picks new files up within a few seconds.
  for (let i = 0; i < 40; i++) {
    const res = await docApi.resolve([ref]).catch(() => []);
    if (res[0]?.found && res[0].file) break;
    await new Promise((r) => setTimeout(r, 400));
  }
  return { path: saved.path, ref, width: w, height: h };
}
