/** High-level document actions: new, open, autosave, save image, save as. */
import { api, isTauri, pickImages, pickProjectFile, pickSavePath } from '../api/backend';
import type { ExportFormat } from '../api/types';
import { relayout } from '../collage/layout';
import { createDoc, defaultLayout, createPhoto } from '../model/factory';
import { presetById, presetSize } from '../model/pages';
import { DOC_VERSION, type CollageDoc } from '../model/types';
import { AdjustCache } from '../render/adjust';
import type { RenderResources } from '../render/renderer';
import { app } from '../stores/app.svelte';
import { library } from '../stores/library.svelte';
import { stickers } from '../stores/stickers.svelte';
import { toasts } from '../stores/toasts.svelte';
import { instantiateTemplate } from '../templates/instantiate';
import type { TemplateDef } from '../templates/types';
import { editor } from './editor.svelte';
import { buildFilename, exportDocument, joinPath, renderThumbnail } from './exporter';

function defaultPage(): { width: number; height: number; dpi: number; preset: string } {
  const s = app.settings;
  const preset = presetById(s.defaultPreset) ?? presetById('a4')!;
  const [width, height] = presetSize(preset, s.defaultDpi, s.defaultOrientation);
  return { width, height, dpi: preset.px ? 72 : s.defaultDpi, preset: preset.id };
}

export function openEditor(doc: CollageDoc, path: string | null = null): void {
  editor.load(doc, path);
  editor.tool = doc.layout.engine !== 'none' ? 'layouts' : doc.template ? 'photos' : 'photos';
  app.view = 'editor';
}

/** A blank canvas for free-form ("handcrafted") collages. */
export function newBlank(): void {
  const page = defaultPage();
  const doc = createDoc({ ...page, name: 'Untitled collage' });
  openEditor(doc);
}

/** PhotoCollage-style quick collage: pick photos, get an automatic layout. */
export async function newQuickCollage(paths?: string[]): Promise<void> {
  const files = paths ?? (await pickImages(app.settings.recentImportDir));
  if (!files.length) return;
  rememberDir(files[0]);
  const images = await library.import(files);
  if (!images.length) return;
  const page = defaultPage();
  let doc = createDoc({ ...page, name: 'Quick collage', layout: defaultLayout('mosaic') });
  for (const img of images) {
    doc.assets[img.id] = { path: img.path, name: img.name, width: img.width, height: img.height };
    doc.elements.push(createPhoto({ x: 0, y: 0, w: 1, h: 1, asset: img.id, slot: true }));
  }
  doc = relayout(doc);
  openEditor(doc);
  editor.tool = 'layouts';
}

export function newFromTemplate(t: TemplateDef, useLibrary = true): void {
  const photos = useLibrary
    ? library.items.map((i) => ({ id: i.id, ref: { path: i.path, name: i.name, width: i.width, height: i.height } }))
    : [];
  const usable = t.slots?.length ? photos.slice(0, t.slots.length) : photos;
  const doc = instantiateTemplate(t, { photos: usable, stickerAspect: (id) => stickers.aspect(id) });
  openEditor(doc);
  editor.tool = 'photos';
}

function rememberDir(path: string): void {
  const dir = path.substring(0, path.lastIndexOf('/'));
  if (dir && dir !== app.settings.recentImportDir) void app.saveSettings({ recentImportDir: dir });
}

export async function addPhotos(): Promise<string[]> {
  const files = await pickImages(app.settings.recentImportDir);
  if (!files.length) return [];
  rememberDir(files[0]);
  const images = await library.import(files);
  return images.map((i) => i.id);
}

/* ------------------------------------------------------------ projects */

function editorResources(): RenderResources {
  return {
    image: (id, need) => library.entry(id, need),
    sticker: (id) => stickers.image(id),
    adjustCache: new AdjustCache(4),
  };
}

function serialize(doc: CollageDoc): string {
  return JSON.stringify({ ...doc, version: DOC_VERSION }, null, 0);
}

/** Save into the app's project library (home screen "Recent"). */
export async function autosave(): Promise<void> {
  if (!isTauri || !editor.open) return;
  const doc = editor.doc;
  try {
    const used = [...editor.usedAssets()];
    await library.preload(used);
    const thumb = await renderThumbnail(doc, editorResources());
    await api.saveProject(doc.id, serialize(doc), thumb);
    if (editor.projectPath) await api.saveProjectFile(editor.projectPath, serialize(doc));
  } catch (e) {
    console.warn('autosave failed', e);
  }
}

let autosaveTimer: ReturnType<typeof setTimeout> | null = null;
export function scheduleAutosave(): void {
  if (autosaveTimer) clearTimeout(autosaveTimer);
  autosaveTimer = setTimeout(() => {
    autosaveTimer = null;
    void autosave();
  }, 1500);
}

export function parseProject(json: string): CollageDoc {
  const doc = JSON.parse(json) as CollageDoc;
  if (!doc || typeof doc !== 'object' || !Array.isArray(doc.elements) || !doc.page) throw new Error('Not a Tessera project');
  if ((doc.version ?? 1) > DOC_VERSION) throw new Error('This project was made with a newer version of Tessera');
  return doc;
}

export async function openProjectPath(path: string, external: boolean): Promise<void> {
  try {
    const doc = parseProject(await api.loadProject(path));
    const missing = await library.ensureAssets(doc.assets);
    openEditor(doc, external ? path : null);
    if (missing.length) {
      toasts.push({
        kind: 'warning',
        title: `${missing.length} photo${missing.length > 1 ? 's are' : ' is'} missing`,
        message: 'They were moved or deleted. Drop new photos onto the empty frames.',
      });
    }
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not open project', message: String(e) });
  }
}

export async function openProjectDialog(): Promise<void> {
  const path = await pickProjectFile();
  if (path) await openProjectPath(path, true);
}

export async function saveProjectAs(): Promise<void> {
  const name = `${editor.doc.name}.tessera`;
  const dir = app.settings.saveDirectory;
  const path = await pickSavePath(dir ? joinPath(dir, name) : name, [{ name: 'Tessera project', extensions: ['tessera'] }], 'Save project as');
  if (!path) return;
  const final = path.endsWith('.tessera') ? path : `${path}.tessera`;
  try {
    await api.saveProjectFile(final, serialize(editor.doc));
    editor.projectPath = final;
    toasts.push({ kind: 'success', title: 'Project saved', message: final });
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not save project', message: String(e) });
  }
}

/* ---------------------------------------------------------------- saving */

export interface SaveState {
  busy: boolean;
  progress: number;
  stage: string;
}

export const saveState = $state<SaveState>({ busy: false, progress: 0, stage: '' });

function checkEmpty(): boolean {
  const hasContent = editor.doc.elements.some((e) => !(e.kind === 'photo' && !e.asset));
  if (!hasContent) {
    toasts.push({ kind: 'warning', title: 'Nothing to save yet', message: 'Add some photos, text or stickers first.' });
    return false;
  }
  return true;
}

async function runExport(path: string, format: ExportFormat, quality: number, scale: number, unique: boolean): Promise<void> {
  if (saveState.busy) return;
  saveState.busy = true;
  saveState.progress = 0;
  saveState.stage = 'Preparing';
  try {
    const res = await exportDocument(editor.doc, {
      path,
      format,
      quality,
      scale,
      unique,
      onProgress: (p, stage) => {
        saveState.progress = p;
        saveState.stage = stage;
      },
    });
    editor.markSaved();
    void autosave();
    const mb = (res.bytes / 1024 / 1024).toFixed(res.bytes > 10 * 1024 * 1024 ? 0 : 1);
    toasts.push({
      kind: 'success',
      title: 'Collage saved',
      message: `${res.path.split('/').pop()} · ${res.width}×${res.height} · ${mb} MB`,
      action: { label: 'Show in folder', run: () => void api.revealPath(res.path) },
    });
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Saving failed', message: String(e) });
  } finally {
    saveState.busy = false;
  }
}

/** One-click Save: writes a copy into the default save folder. */
export async function quickSave(): Promise<void> {
  if (!checkEmpty()) return;
  let dir = app.settings.saveDirectory;
  if (!dir) {
    toasts.push({ kind: 'warning', title: 'Choose a save folder first', message: 'Set it in Settings → Saving.' });
    app.view = 'settings';
    return;
  }
  try {
    dir = await api.ensureDir(dir);
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Save folder unavailable', message: String(e) });
    return;
  }
  const s = app.settings;
  const file = buildFilename(s.filenamePattern, editor.doc.name, s.exportFormat);
  await runExport(joinPath(dir, file), s.exportFormat, s.exportQuality, 1, true);
}

/** Save As: the caller (export dialog) already chose format and options. */
export async function saveAs(format: ExportFormat, quality: number, scale: number): Promise<void> {
  if (!checkEmpty()) return;
  const s = app.settings;
  const file = buildFilename('{name}', editor.doc.name, format);
  const ext = file.split('.').pop()!;
  const names: Record<ExportFormat, string> = {
    jpeg: 'JPEG image',
    png: 'PNG image',
    webp: 'WebP image',
    avif: 'AVIF image',
    tiff: 'TIFF image',
    pdf: 'PDF document',
  };
  const path = await pickSavePath(s.saveDirectory ? joinPath(s.saveDirectory, file) : file, [{ name: names[format], extensions: [ext] }]);
  if (!path) return;
  const final = /\.[a-z0-9]{2,5}$/i.test(path) ? path : `${path}.${ext}`;
  await runExport(final, format, quality, scale, false);
}

export function newDocFromPreset(presetId: string, orientation: 'portrait' | 'landscape', dpi: number): CollageDoc {
  const preset = presetById(presetId) ?? presetById('a4')!;
  const [width, height] = presetSize(preset, dpi, orientation);
  return createDoc({ width, height, dpi: preset.px ? 72 : dpi, preset: preset.id });
}
