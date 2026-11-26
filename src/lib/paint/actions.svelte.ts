/**
 * Paint commands used by menus, shortcuts and the command palette:
 * clipboard, select subject, looks, memes, and the keyboard map.
 */
import { ensureFont } from '../assets/fonts';
import type { PhotoAdjust, TextElement } from '../model/types';
import { createText } from '../model/factory';
import { applyLookPixels } from '../render/look';
import { textHeight } from '../render/text';
import { presetElement, textPreset } from '../text/styles';
import { app } from '../stores/app.svelte';
import { toasts } from '../stores/toasts.svelte';
import type { PaintDoc } from './doc.svelte';
import { blobCanvas, canvasBlob } from './io';
import { combine } from './lib/mask';
import { zoomStep, zoomAt } from './lib/geom';
import { clipboardImage, newPaintFromCanvas, saveNow } from './session.svelte';
import { paint, type ToolId } from './state.svelte';
import { finishTextEdit, SIZED } from './tools.svelte';

/* ------------------------------------------------------------ clipboard */

let internal: { canvas: HTMLCanvasElement; x: number; y: number } | null = null;

export async function copySelection(cut = false): Promise<void> {
  const d = paint.doc;
  if (!d) return;
  d.commitFloating();
  const px = d.copyPixels();
  if (!px) return;
  internal = px;
  // Also on the system clipboard, where the browser allows it (secure pages).
  try {
    if (typeof ClipboardItem !== 'undefined' && navigator.clipboard?.write) {
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': canvasBlob(px.canvas) })]);
    }
  } catch {
    /* the in-app clipboard still works */
  }
  if (cut) d.clearSelection();
  toasts.push({ kind: 'success', title: cut ? 'Cut' : 'Copied', message: `${px.canvas.width} × ${px.canvas.height}`, timeout: 1500 });
}

/** Paste an image as a floating selection (centred in view when it came from elsewhere). */
export async function pasteCanvas(c: HTMLCanvasElement, at?: { x: number; y: number }): Promise<void> {
  const d = paint.doc;
  if (!d) {
    await newPaintFromCanvas(c, 'Pasted image');
    return;
  }
  if (c.width > d.width || c.height > d.height) {
    const choice = await paint.choose('The pasted image is bigger than the drawing', `It is ${c.width} × ${c.height}; the drawing is ${d.width} × ${d.height}.`, [
      { id: 'keep', label: 'Keep the drawing size' },
      { id: 'grow', label: 'Make the drawing bigger', primary: true },
    ]);
    if (!choice) return;
    if (choice === 'grow') d.resizeCanvas(Math.max(d.width, c.width), Math.max(d.height, c.height), 0, 0, null);
  }
  const v = paint.view;
  let x: number;
  let y: number;
  if (at) {
    x = at.x;
    y = at.y;
  } else {
    // the middle of what's on screen
    const vw = (document.querySelector('.paint-viewport') as HTMLElement | null)?.clientWidth ?? 800;
    const vh = (document.querySelector('.paint-viewport') as HTMLElement | null)?.clientHeight ?? 600;
    x = Math.round((vw / 2 - v.x) / v.zoom - c.width / 2);
    y = Math.round((vh / 2 - v.y) / v.zoom - c.height / 2);
  }
  x = Math.max(Math.min(x, d.width - c.width), 0);
  y = Math.max(Math.min(y, d.height - c.height), 0);
  d.paste(c, x, y);
  setTool('move');
}

let pastePending = false;

/** Ctrl+V: the paste event brings system clipboard images; otherwise the in-app clipboard. */
export function pasteShortcut(): void {
  pastePending = true;
  // WebKit only sends paste events to editable elements; ask the Clipboard API then.
  setTimeout(async () => {
    if (!pastePending) return;
    pastePending = false;
    const blob = await clipboardImage();
    if (blob) await pasteCanvas(await blobCanvas(blob));
    else if (internal) await pasteCanvas(internal.canvas, { x: internal.x, y: internal.y });
  }, 120);
}

export async function onPaste(e: ClipboardEvent): Promise<void> {
  const t = e.target as HTMLElement | null;
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
  const items = Array.from(e.clipboardData?.items ?? []);
  const img = items.find((i) => i.kind === 'file' && i.type.startsWith('image/'));
  const file = img?.getAsFile();
  if (file) {
    pastePending = false;
    e.preventDefault();
    await pasteCanvas(await blobCanvas(file));
  } else if (internal && pastePending) {
    pastePending = false;
    e.preventDefault();
    await pasteCanvas(internal.canvas, { x: internal.x, y: internal.y });
  }
}

/* ------------------------------------------------------------ select subject */

/** Select the main subject with the cut-out model (on this device). */
export async function selectSubject(mode: 'replace' | 'add' = 'replace'): Promise<void> {
  const d = paint.doc;
  if (!d || paint.busy) return;
  paint.busy = 'Finding the subject…';
  try {
    if (typeof Worker === 'undefined') throw new Error('This browser cannot run the cut-out engine.');
    const [{ segment }, { MODEL_SIZE }, { buildAlpha, DEFAULT_MASK }] = await Promise.all([import('../cutout/engine'), import('../cutout/model'), import('../cutout/mask')]);
    const raw = await segment(d.composite, (stage, f) => (paint.busy = `${stage}… ${Math.round(f * 100)}%`));
    const a = buildAlpha(raw, MODEL_SIZE, MODEL_SIZE, d.width, d.height, DEFAULT_MASK);
    const m = new Uint8Array(a.length);
    for (let i = 0; i < a.length; i++) m[i] = Math.round(a[i] * 255);
    d.commitFloating();
    d.setSelection(combine(mode === 'add' ? d.selection : null, m, mode));
    if (!d.selection) toasts.push({ kind: 'info', title: 'No clear subject found', message: 'Try the lasso or the magic wand instead.' });
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not select the subject', message: String(e instanceof Error ? e.message : e) });
  } finally {
    paint.busy = null;
  }
}

/* ------------------------------------------------------------ looks */

/** Apply a photo look (filters and adjustments) to the active layer (inside the selection). */
export function applyLook(adjust: PhotoAdjust): void {
  const d = paint.doc;
  const a = d?.active;
  if (!d || !a) return;
  if (a.locked) {
    toasts.push({ kind: 'info', title: `“${a.name}” is locked` });
    return;
  }
  const r = { x: 0, y: 0, w: d.width, h: d.height };
  d.editImageData(a.id, r, 'Look', (img) => lookPixels(img.data, img.width, img.height, adjust));
}

/** The look pipeline plus its vignette (which the collage renderer draws on top). */
export function lookPixels(px: Uint8ClampedArray, w: number, h: number, adjust: PhotoAdjust): void {
  applyLookPixels(px, w, h, { ...adjust, vignette: 0 });
  const v = adjust.vignette ?? 0;
  if (v <= 0) return;
  const cx = w / 2;
  const cy = h / 2;
  const r0 = Math.min(w, h) * 0.25;
  const r1 = Math.hypot(w, h) / 2;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const t = Math.max(0, Math.min(1, (Math.hypot(x - cx, y - cy) - r0) / (r1 - r0)));
      if (!t) continue;
      const k = 1 - 0.7 * v * t;
      const i = (y * w + x) * 4;
      px[i] *= k;
      px[i + 1] *= k;
      px[i + 2] *= k;
    }
  }
}

/* ------------------------------------------------------------ memes */

/** A classic meme text element (white Anton, black outline) across the top or bottom. */
export function memeElement(d: PaintDoc, where: 'top' | 'bottom', text: string): TextElement {
  const p = textPreset('meme')!;
  const size = Math.round(Math.min(d.width, d.height) * 0.11);
  const margin = Math.round(d.width * 0.04);
  const el = createText({ x: margin, y: 0, w: d.width - 2 * margin, ...presetElement(p, size, Math.min(d.width, d.height)), text, align: 'center' });
  el.h = textHeight(el);
  el.y = where === 'top' ? Math.round(d.height * 0.03) : Math.round(d.height - el.h - d.height * 0.03);
  return el;
}

/** Add top and bottom meme text layers (or focus the existing ones). */
export async function addMemeText(): Promise<void> {
  const d = paint.doc;
  if (!d) return;
  finishTextEdit();
  await ensureFont('anton', 400);
  if (paint.doc !== d) return;
  const has = (n: string) => d.layers.find((l) => l.name === n && l.text);
  const top = has('Top text') ?? d.addTextLayer(memeElement(d, 'top', 'TOP TEXT'), 'Top text');
  if (!has('Bottom text')) d.addTextLayer(memeElement(d, 'bottom', 'BOTTOM TEXT'), 'Bottom text');
  paint.opts.textPreset = 'meme';
  setTool('text');
  d.activeId = top.id;
  paint.textEdit = { layerId: top.id, before: null };
}

/** Keep a meme text layer's bottom edge in place while its text grows. */
export function refitMeme(d: PaintDoc, id: string): void {
  const l = d.layer(id);
  if (!l?.text || l.name !== 'Bottom text') return;
  const el = { ...($state.snapshot(l.text) as TextElement) };
  const h = textHeight(el);
  el.y = Math.round(d.height - h - d.height * 0.03);
  if (el.y !== l.text.y) d.setText(id, el);
}

/* ------------------------------------------------------------ tools & keys */

export function setTool(t: ToolId): void {
  const d = paint.doc;
  if (paint.tool === t) return;
  if (paint.tool === 'text' && t !== 'text') finishTextEdit();
  if (paint.tool === 'move' && t !== 'move') d?.commitFloating();
  if (paint.tool === 'crop') paint.crop = null;
  paint.tool = t;
  paint.persist();
  d?.repaint();
}

export function applyCrop(): void {
  const d = paint.doc;
  const r = paint.crop;
  if (!d || !r) return;
  d.crop(r);
  paint.crop = null;
  paint.fitPending = true;
}

export function zoomBy(dir: 1 | -1, sx?: number, sy?: number): void {
  const el = document.querySelector('.paint-viewport') as HTMLElement | null;
  const x = sx ?? (el?.clientWidth ?? 800) / 2;
  const y = sy ?? (el?.clientHeight ?? 600) / 2;
  paint.view = zoomAt(paint.view, zoomStep(paint.view.zoom, dir), x, y);
}

export function zoomTo(z: number): void {
  const el = document.querySelector('.paint-viewport') as HTMLElement | null;
  paint.view = zoomAt(paint.view, z, (el?.clientWidth ?? 800) / 2, (el?.clientHeight ?? 600) / 2);
}

const TOOL_KEYS: Record<string, ToolId> = {
  b: 'brush',
  p: 'pencil',
  e: 'eraser',
  g: 'fill',
  i: 'picker',
  u: 'shape',
  t: 'text',
  m: 'select',
  l: 'lasso',
  w: 'wand',
  v: 'move',
  h: 'hand',
  c: 'crop',
  r: 'blur',
  s: 'clone',
};

/** The shortcut list shown in the help dialog. */
export const SHORTCUTS: [string, string][] = [
  ['B', 'Brush'],
  ['P', 'Pencil (hard pixels)'],
  ['E', 'Eraser'],
  ['G', 'Fill bucket'],
  ['I', 'Eyedropper (or Alt+click while painting)'],
  ['U', 'Shapes: line, arrow, rectangle, ellipse'],
  ['T', 'Text'],
  ['M', 'Rectangle / ellipse select'],
  ['L', 'Lasso select'],
  ['W', 'Magic wand'],
  ['V', 'Move the selection (or the whole layer)'],
  ['H or Space', 'Hand: drag to pan'],
  ['C', 'Crop'],
  ['R / S', 'Blur, smudge / clone stamp'],
  ['[  ]', 'Smaller / bigger brush'],
  ['X', 'Swap primary and secondary colours'],
  ['D', 'Black and white colours'],
  ['Ctrl+Z, Ctrl+Shift+Z / Ctrl+Y', 'Undo, redo'],
  ['Ctrl+A, Ctrl+D', 'Select all, deselect'],
  ['Ctrl+Shift+I', 'Invert the selection'],
  ['Delete', 'Clear the selection'],
  ['Ctrl+C, Ctrl+X, Ctrl+V', 'Copy, cut, paste (images from other apps too)'],
  ['Enter / Esc', 'Apply / cancel (crop, moving, text)'],
  ['Arrows', 'Nudge what you are moving (Shift: 10 px)'],
  ['Ctrl+wheel, Ctrl + / −', 'Zoom'],
  ['Ctrl+0, Ctrl+1', 'Fit to window, 100 %'],
  ['Ctrl+S', 'Save progress'],
  ['Ctrl+E', 'Export as an image'],
  ['Shift while drawing', 'Straight 45° lines, squares and circles'],
  ['Shift / Alt while selecting', 'Add to / remove from the selection'],
];

function typing(e: KeyboardEvent): boolean {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
}

/** Paint's keyboard shortcuts; returns true when the key was handled. */
export function handleKey(e: KeyboardEvent, fit: () => void): boolean {
  const d = paint.doc;
  const k = e.key.toLowerCase();
  const mod = e.ctrlKey || e.metaKey;
  if (app.modals > 0 || paint.dialog || paint.ask) return false;
  if (!d) {
    // Ctrl+V on the start screen: a new drawing from the clipboard
    if (mod && k === 'v' && !typing(e)) pasteShortcut();
    return false;
  }
  if (typing(e)) {
    if (e.key === 'Escape' && paint.textEdit) {
      finishTextEdit();
      (e.target as HTMLElement).blur();
      return true;
    }
    return false;
  }
  if (mod) {
    if (k === 'z' && !e.shiftKey) d.undo();
    else if ((k === 'z' && e.shiftKey) || k === 'y') d.redo();
    else if (k === 'a') d.selectAll();
    else if (k === 'd') d.deselect();
    else if (k === 'i' && e.shiftKey) d.invertSelection();
    else if (k === 'c') void copySelection(false);
    else if (k === 'x') void copySelection(true);
    else if (k === 'v') {
      pasteShortcut();
      return false; // let the paste event through
    } else if (k === 's') void saveNow();
    else if (k === 'e') paint.dialog = 'export';
    else if (k === '0') fit();
    else if (k === '1') zoomTo(1);
    else if (k === '=' || k === '+') zoomBy(1);
    else if (k === '-') zoomBy(-1);
    else return false;
    return true;
  }
  if (e.altKey) return false;
  if (e.key === 'Escape') {
    if (paint.textEdit) finishTextEdit();
    else if (d.floating) d.cancelFloating();
    else if (paint.crop) paint.crop = null;
    else if (d.selection) d.deselect();
    else return false;
    d.repaint();
    return true;
  }
  if (e.key === 'Enter') {
    if (paint.crop) applyCrop();
    else if (d.floating) d.commitFloating();
    else if (paint.textEdit) finishTextEdit();
    else return false;
    return true;
  }
  if (e.key === 'Delete' || e.key === 'Backspace') {
    if (!d.selection && !d.floating) return false;
    d.clearSelection();
    return true;
  }
  if (e.key.startsWith('Arrow') && d.floating) {
    const s = e.shiftKey ? 10 : 1;
    const f = d.floating;
    const dx = e.key === 'ArrowLeft' ? -s : e.key === 'ArrowRight' ? s : 0;
    const dy = e.key === 'ArrowUp' ? -s : e.key === 'ArrowDown' ? s : 0;
    d.moveFloating(f.x + dx, f.y + dy);
    return true;
  }
  if (k === '[' || k === ']') {
    const s = paint.size;
    if (SIZED.includes(paint.tool)) paint.size = k === '[' ? Math.max(1, s < 10 ? s - 1 : s / 1.2) : s < 10 ? s + 1 : s * 1.2;
    else if (paint.tool === 'shape') paint.opts.shapeWidth = Math.max(1, Math.round(k === '[' ? paint.opts.shapeWidth / 1.2 : paint.opts.shapeWidth * 1.2 + 0.5));
    d.repaint();
    return true;
  }
  if (k === 'x') paint.swap();
  else if (k === 'd') paint.resetColors();
  else if (k === 'r' && paint.tool === 'blur') setTool('smudge');
  else if (TOOL_KEYS[k]) {
    // U and M cycle through their variants, like most paint apps
    if (k === 'u' && paint.tool === 'shape') {
      const order = ['line', 'arrow', 'rect', 'ellipse'] as const;
      paint.opts.shape = order[(order.indexOf(paint.opts.shape) + 1) % order.length];
    } else if (k === 'm' && paint.tool === 'select') paint.opts.selectShape = paint.opts.selectShape === 'rect' ? 'ellipse' : 'rect';
    else setTool(TOOL_KEYS[k]);
  } else if (k === '?') paint.dialog = 'help';
  else return false;
  return true;
}
