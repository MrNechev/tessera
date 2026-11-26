/**
 * A Paint document: layers (one canvas each, bottom first), the selection,
 * the floating selection being moved or pasted, the live stroke overlay, and
 * undo/redo. The composite (all visible layers blended) is kept up to date in
 * dirty rectangles; the view just scales it onto the screen.
 *
 * Pixel edits go through `editPixels`, which records only the tiles that
 * changed (lib/history.ts); structural edits record their own undo steps.
 */
import { ensureFont } from '../assets/fonts';
import type { TextElement } from '../model/types';
import { drawText, textHeight } from '../render/text';
import { History, TileRecorder, patchBytes, type Patch, type Step } from './lib/history';
import { clipRect, invertMask, maskBounds, rectMask, translateMask, unionRect, type Rect } from './lib/mask';
import type { BlendMode } from './lib/ora';
import { anchorOffset } from './lib/geom';

export interface Layer {
  id: string;
  name: string;
  visible: boolean;
  locked: boolean;
  opacity: number;
  blend: BlendMode;
  /** A live text layer: re-rendered from this until painted on (then it is plain pixels). */
  text: TextElement | null;
  /** Bumped whenever the pixels change (thumbnails). */
  rev: number;
}

export interface Overlay {
  layerId: string;
  /** Document-sized scratch canvas holding the stroke/shape being drawn. */
  canvas: HTMLCanvasElement;
  alpha: number;
  op: GlobalCompositeOperation;
  rect: Rect | null;
}

export interface Floating {
  layerId: string;
  canvas: HTMLCanvasElement;
  x: number;
  y: number;
  /** Where the pixels were lifted from (null: pasted). */
  from: { x: number; y: number } | null;
  /** Selection mask at lift/paste time and its offset (moved along). */
  mask: Uint8Array;
  maskAt: { x: number; y: number };
}

export function newCanvas(w: number, h: number): HTMLCanvasElement {
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(w));
  c.height = Math.max(1, Math.round(h));
  return c;
}

export function ctxOf(c: HTMLCanvasElement): CanvasRenderingContext2D {
  return c.getContext('2d', { willReadFrequently: true })!;
}

export function cloneCanvas(src: HTMLCanvasElement): HTMLCanvasElement {
  const c = newCanvas(src.width, src.height);
  ctxOf(c).drawImage(src, 0, 0);
  return c;
}

let seq = 0;
export function layerId(): string {
  return `l${Date.now().toString(36)}${(seq++).toString(36)}`;
}
export function drawingId(): string {
  const r = Math.random().toString(36).slice(2, 8);
  return `d-${Date.now().toString(36)}-${r}`;
}

interface Snapshot {
  width: number;
  height: number;
  layers: Layer[];
  canvases: Map<string, HTMLCanvasElement>;
  activeId: string;
}

/** Layer property edits (opacity drags…) within this time merge into one undo step. */
const MERGE_MS = 1200;

export class PaintDoc {
  readonly id: string;
  name = $state('Untitled drawing');
  width = $state(1);
  height = $state(1);
  /** Bottom layer first. */
  layers = $state<Layer[]>([]);
  activeId = $state('');
  /** Bumped when the selection changes. */
  selRev = $state(0);
  selBounds = $state<Rect | null>(null);
  /** Bumped on undo-history changes (menus enable/disable). */
  histRev = $state(0);
  /** Bumped on every change to the document (autosave). */
  editRev = $state(0);
  savedRev = $state(0);
  floating = $state.raw<Floating | null>(null);
  /** Image this drawing was started from (shown in the title, used for export names). */
  source = $state<string | null>(null);

  selection: Uint8Array | null = null;
  overlay: Overlay | null = null;
  readonly history = new History();
  readonly canvases = new Map<string, HTMLCanvasElement>();
  private comp: HTMLCanvasElement;
  private pending: Rect | null = null;
  private work: HTMLCanvasElement;
  private selCanvasCache: HTMLCanvasElement | null = null;
  private scratch: HTMLCanvasElement | null = null;
  private lastProp: { key: string; at: number; step: Step & { after: Partial<Layer> } } | null = null;
  private listeners = new Set<(r: Rect | null) => void>();

  constructor(width: number, height: number, id = drawingId()) {
    this.id = id;
    this.width = width;
    this.height = height;
    this.comp = newCanvas(width, height);
    this.work = newCanvas(width, height);
  }

  /** All visible layers blended (brought up to date first). */
  get composite(): HTMLCanvasElement {
    this.flushComposite();
    return this.comp;
  }

  /** Recomposite what changed since the last call (the view calls this once per frame). */
  flushComposite(): void {
    const r = this.pending;
    if (!r) return;
    this.pending = null;
    this.recomposite(r);
  }

  get dirty(): boolean {
    return this.editRev !== this.savedRev;
  }

  /* ------------------------------------------------------------ basics */

  /** Redraw requests from the view: `null` = everything. */
  onChange(fn: (r: Rect | null) => void): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Mark `r` (null: all) for recompositing and tell the view. */
  invalidate(r: Rect | null = null): void {
    const c = r ? clipRect(r, this.width, this.height) : { x: 0, y: 0, w: this.width, h: this.height };
    if (!c) return;
    this.pending = unionRect(this.pending, c);
    for (const l of this.listeners) l(c);
  }

  /** Something visible-only changed (selection, tool overlay): tell the view, no recomposite. */
  repaint(): void {
    for (const l of this.listeners) l(null);
  }

  /** Called after every change (autosave). */
  onEdit: (() => void) | null = null;

  touched(): void {
    this.editRev++;
    this.onEdit?.();
  }

  layer(id: string): Layer | undefined {
    return this.layers.find((l) => l.id === id);
  }
  get active(): Layer | undefined {
    return this.layer(this.activeId) ?? this.layers.at(-1);
  }
  canvas(id: string): HTMLCanvasElement {
    return this.canvases.get(id)!;
  }
  get activeCanvas(): HTMLCanvasElement | undefined {
    const a = this.active;
    return a ? this.canvases.get(a.id) : undefined;
  }
  index(id: string): number {
    return this.layers.findIndex((l) => l.id === id);
  }

  private recomposite(r: Rect): void {
    const c = this.comp.getContext('2d')!;
    c.save();
    c.clearRect(r.x, r.y, r.w, r.h);
    c.beginPath();
    c.rect(r.x, r.y, r.w, r.h);
    c.clip();
    for (const l of this.layers) {
      if (!l.visible || l.opacity <= 0) continue;
      let src = this.canvases.get(l.id);
      if (!src) continue;
      const ov = this.overlay?.layerId === l.id ? this.overlay : null;
      const fl = this.floating?.layerId === l.id ? this.floating : null;
      if (ov || fl) {
        const w = this.work.getContext('2d')!;
        w.save();
        w.globalCompositeOperation = 'copy';
        w.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
        w.globalCompositeOperation = 'source-over';
        if (ov) {
          w.globalAlpha = ov.alpha;
          w.globalCompositeOperation = ov.op;
          w.drawImage(ov.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
          w.globalAlpha = 1;
          w.globalCompositeOperation = 'source-over';
        }
        if (fl) {
          w.beginPath();
          w.rect(r.x, r.y, r.w, r.h);
          w.clip();
          w.drawImage(fl.canvas, fl.x, fl.y);
        }
        w.restore();
        src = this.work;
      }
      c.globalAlpha = l.opacity;
      c.globalCompositeOperation = l.blend;
      c.drawImage(src, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    }
    c.restore();
  }

  /* ------------------------------------------------------------ history */

  private push(step: Step): void {
    this.history.push(step);
    this.lastProp = null;
    this.histRev++;
    this.touched();
  }

  undo(): void {
    this.commitFloating();
    if (this.history.undo()) {
      this.lastProp = null;
      this.histRev++;
      this.touched();
    }
  }

  redo(): void {
    this.commitFloating();
    if (this.history.redo()) {
      this.histRev++;
      this.touched();
    }
  }

  private readRect(c: HTMLCanvasElement) {
    return (r: Rect) => ctxOf(c).getImageData(r.x, r.y, r.w, r.h).data;
  }

  private applyPatches(id: string, patches: Patch[], which: 'before' | 'after'): void {
    const c = this.canvases.get(id);
    const l = this.layer(id);
    if (!c || !l) return;
    const ctx = ctxOf(c);
    let dirty: Rect | null = null;
    for (const p of patches) {
      ctx.putImageData(new ImageData(new Uint8ClampedArray(p[which]), p.rect.w, p.rect.h), p.rect.x, p.rect.y);
      dirty = unionRect(dirty, p.rect);
    }
    l.rev++;
    this.invalidate(dirty);
  }

  /**
   * Change pixels of a layer inside `rect` as one undo step. `fn` draws on
   * the layer's context; a live text layer becomes plain pixels.
   */
  editPixels(id: string, rect: Rect, label: string, fn: (c: CanvasRenderingContext2D) => void): boolean {
    const canvas = this.canvases.get(id);
    const l = this.layer(id);
    const r = clipRect(rect, this.width, this.height);
    if (!canvas || !l || !r) return false;
    const rec = new TileRecorder(this.width, this.height, this.readRect(canvas));
    rec.touch(r);
    const ctx = ctxOf(canvas);
    ctx.save();
    fn(ctx);
    ctx.restore();
    const patches = rec.finish();
    l.rev++;
    this.invalidate(r);
    if (!patches.length) return false;
    const text = l.text;
    if (text) l.text = null;
    this.push({
      label,
      bytes: patchBytes(patches),
      undo: () => {
        this.applyPatches(id, patches, 'before');
        const ll = this.layer(id);
        if (ll && text) ll.text = text;
      },
      redo: () => {
        this.applyPatches(id, patches, 'after');
        const ll = this.layer(id);
        if (ll) ll.text = null;
      },
    });
    return true;
  }

  /** Replace pixels via ImageData (filters, looks): `fn` edits the RGBA of `rect` in place. */
  editImageData(id: string, rect: Rect, label: string, fn: (d: ImageData) => void, limitToSelection = true): boolean {
    const r = clipRect(rect, this.width, this.height);
    if (!r) return false;
    const c = this.canvases.get(id);
    if (!c) return false;
    const before = ctxOf(c).getImageData(r.x, r.y, r.w, r.h);
    const after = new ImageData(new Uint8ClampedArray(before.data), r.w, r.h);
    fn(after);
    if (limitToSelection && this.selection) {
      const m = this.selection;
      const a = after.data;
      const b = before.data;
      for (let y = 0; y < r.h; y++) {
        for (let x = 0; x < r.w; x++) {
          const s = m[(r.y + y) * this.width + r.x + x];
          if (s === 255) continue;
          const i = (y * r.w + x) * 4;
          const t = s / 255;
          for (let k = 0; k < 4; k++) a[i + k] = b[i + k] + (a[i + k] - b[i + k]) * t;
        }
      }
    }
    return this.editPixels(id, r, label, (ctx) => ctx.putImageData(after, r.x, r.y));
  }

  /** Every layer and the size, for big structural changes (resize, crop, rotate). */
  private snapshot(): Snapshot {
    return {
      width: this.width,
      height: this.height,
      layers: this.layers.map((l) => ({ ...$state.snapshot(l) }) as Layer),
      canvases: new Map([...this.canvases].map(([k, v]) => [k, cloneCanvas(v)])),
      activeId: this.activeId,
    };
  }

  private restore(s: Snapshot): void {
    this.width = s.width;
    this.height = s.height;
    this.canvases.clear();
    for (const [k, v] of s.canvases) this.canvases.set(k, cloneCanvas(v));
    this.layers = s.layers.map((l) => ({ ...l, rev: l.rev + 1 }));
    this.activeId = s.activeId;
    this.resized();
  }

  private snapshotBytes(s: Snapshot): number {
    return s.layers.length * s.width * s.height * 4;
  }

  /** Run a structural change of the whole document as one undo step. */
  private structural(label: string, fn: () => void): void {
    this.commitFloating();
    const before = this.snapshot();
    fn();
    const after = this.snapshot();
    this.push({
      label,
      bytes: this.snapshotBytes(before) + this.snapshotBytes(after),
      undo: () => this.restore(before),
      redo: () => this.restore(after),
    });
  }

  /** After the size changed: helper canvases, selection. */
  private resized(): void {
    if (this.comp.width !== this.width || this.comp.height !== this.height) {
      this.comp = newCanvas(this.width, this.height);
      this.pending = null;
      this.work = newCanvas(this.width, this.height);
      this.scratch = null;
    }
    this.setSelection(null);
    this.invalidate(null);
  }

  /* ------------------------------------------------------------ layers */

  /** A new layer canvas and its record (not yet in the stack). */
  makeLayer(name: string, canvas?: HTMLCanvasElement): Layer {
    const l: Layer = { id: layerId(), name, visible: true, locked: false, opacity: 1, blend: 'source-over', text: null, rev: 0 };
    this.canvases.set(l.id, canvas ?? newCanvas(this.width, this.height));
    return l;
  }

  /** Put the first layers in (no undo step): a new or loaded document. */
  initLayers(layers: Layer[], active?: string): void {
    this.layers = layers;
    this.activeId = active && layers.some((l) => l.id === active) ? active : (layers.at(-1)?.id ?? '');
    this.invalidate(null);
  }

  private insertLayer(l: Layer, at: number, label: string): void {
    const canvas = this.canvases.get(l.id)!;
    const prevActive = this.activeId;
    let saved: Layer = l;
    const apply = () => {
      this.canvases.set(l.id, canvas);
      const list = [...this.layers];
      list.splice(at, 0, { ...saved });
      this.layers = list;
      this.activeId = l.id;
      this.invalidate(null);
    };
    apply();
    this.push({
      label,
      bytes: canvas.width * canvas.height * 4,
      undo: () => {
        const cur = this.layer(l.id);
        if (cur) saved = { ...($state.snapshot(cur) as Layer) };
        this.layers = this.layers.filter((x) => x.id !== l.id);
        this.activeId = prevActive;
        this.invalidate(null);
      },
      redo: apply,
    });
  }

  addLayer(name?: string, canvas?: HTMLCanvasElement, label = 'New layer'): Layer {
    this.commitFloating();
    const l = this.makeLayer(name ?? `Layer ${this.layers.length + 1}`, canvas);
    const at = this.active ? this.index(this.active.id) + 1 : this.layers.length;
    this.insertLayer(l, at, label);
    return this.layer(l.id)!;
  }

  duplicateLayer(id = this.activeId): void {
    const src = this.layer(id);
    if (!src) return;
    this.commitFloating();
    const l = this.makeLayer(`${src.name} copy`, cloneCanvas(this.canvas(id)));
    Object.assign(l, { visible: src.visible, opacity: src.opacity, blend: src.blend, text: src.text ? { ...$state.snapshot(src.text) } : null });
    this.insertLayer(l, this.index(id) + 1, 'Duplicate layer');
  }

  deleteLayer(id = this.activeId): void {
    if (this.layers.length <= 1) return;
    const at = this.index(id);
    const l = this.layer(id);
    if (at < 0 || !l) return;
    this.commitFloating();
    const canvas = this.canvas(id);
    const rec = { ...l };
    const apply = () => {
      this.layers = this.layers.filter((x) => x.id !== id);
      if (this.activeId === id) this.activeId = this.layers[Math.max(0, at - 1)]?.id ?? '';
      this.invalidate(null);
    };
    apply();
    this.push({
      label: 'Delete layer',
      bytes: canvas.width * canvas.height * 4,
      undo: () => {
        this.canvases.set(id, canvas);
        const list = [...this.layers];
        list.splice(at, 0, rec);
        this.layers = list;
        this.activeId = id;
        this.invalidate(null);
      },
      redo: apply,
    });
  }

  /** Move a layer to stack position `to` (0 = bottom). */
  moveLayer(id: string, to: number): void {
    const from = this.index(id);
    to = Math.max(0, Math.min(this.layers.length - 1, to));
    if (from < 0 || from === to) return;
    this.commitFloating();
    const move = (a: number, b: number) => {
      const list = [...this.layers];
      const [x] = list.splice(a, 1);
      list.splice(b, 0, x);
      this.layers = list;
      this.invalidate(null);
    };
    move(from, to);
    this.push({ label: 'Move layer', bytes: 64, undo: () => move(to, from), redo: () => move(from, to) });
  }

  /** Visibility, lock, opacity, blend, name: one undo step (quick repeats of the same property merge). */
  setLayerProps(id: string, patch: Partial<Pick<Layer, 'visible' | 'locked' | 'opacity' | 'blend' | 'name'>>, label = 'Layer settings'): void {
    const l = this.layer(id);
    if (!l) return;
    const key = id + ':' + Object.keys(patch).sort().join(',');
    const before: Partial<Layer> = {};
    for (const k of Object.keys(patch) as (keyof typeof patch)[]) (before as Record<string, unknown>)[k] = l[k];
    const apply = (p: Partial<Layer>) => {
      const ll = this.layer(id);
      if (!ll) return;
      Object.assign(ll, p);
      this.invalidate(null);
    };
    apply(patch);
    const now = performance.now();
    if (this.lastProp && this.lastProp.key === key && now - this.lastProp.at < MERGE_MS && this.history.undoLabel === this.lastProp.step.label) {
      Object.assign(this.lastProp.step.after, patch);
      this.lastProp.at = now;
      this.touched();
      return;
    }
    const step = { label, bytes: 64, after: { ...patch } as Partial<Layer>, undo: () => apply(before), redo: () => apply(step.after) };
    this.push(step);
    this.lastProp = { key, at: now, step };
  }

  /** Merge the layer into the one below it. */
  mergeDown(id = this.activeId): void {
    const at = this.index(id);
    if (at <= 0) return;
    const top = this.layers[at];
    const below = this.layers[at - 1];
    this.structural('Merge down', () => {
      const c = ctxOf(this.canvas(below.id));
      c.globalAlpha = top.opacity;
      c.globalCompositeOperation = top.blend;
      if (top.visible) c.drawImage(this.canvas(top.id), 0, 0);
      c.globalAlpha = 1;
      c.globalCompositeOperation = 'source-over';
      const b = this.layer(below.id)!;
      b.text = null;
      b.rev++;
      this.layers = this.layers.filter((l) => l.id !== top.id);
      this.canvases.delete(top.id);
      this.activeId = below.id;
      this.invalidate(null);
    });
  }

  /** One layer with everything visible (hidden layers are dropped). */
  flatten(): void {
    if (this.layers.length <= 1 && !this.layers[0]?.text) return;
    this.structural('Flatten', () => {
      const flat = cloneCanvas(this.composite);
      this.canvases.clear();
      const l = this.makeLayer('Background', flat);
      this.layers = [l];
      this.activeId = l.id;
      this.invalidate(null);
    });
  }

  /* ------------------------------------------------------------ text layers */

  /** Draw a text layer's text into its canvas. */
  renderText(id: string): void {
    const l = this.layer(id);
    const c = this.canvases.get(id);
    if (!l?.text || !c) return;
    const el = $state.snapshot(l.text) as TextElement;
    const ctx = ctxOf(c);
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.save();
    ctx.translate(el.x, el.y);
    drawText(ctx, el, { scale: 1, pageShort: Math.min(this.width, this.height) });
    ctx.restore();
    l.rev++;
    this.invalidate(null);
    // A font that isn't loaded yet draws with a fallback; redraw once it is.
    void ensureFont(el.font, el.weight).then(() => {
      const now = this.layer(id);
      if (now?.text && JSON.stringify($state.snapshot(now.text)) === JSON.stringify(el)) {
        ctx.clearRect(0, 0, c.width, c.height);
        ctx.save();
        ctx.translate(el.x, el.y);
        drawText(ctx, el, { scale: 1, pageShort: Math.min(this.width, this.height) });
        ctx.restore();
        now.rev++;
        this.invalidate(null);
      }
    });
  }

  addTextLayer(el: TextElement, name = 'Text'): Layer {
    el.h = textHeight(el);
    const l = this.addLayer(name, undefined, 'Add text');
    l.text = el;
    this.renderText(l.id);
    return l;
  }

  /** Live edit of a text layer (no undo step; call `commitText` when editing ends). */
  setText(id: string, el: TextElement): void {
    const l = this.layer(id);
    if (!l) return;
    el.h = textHeight(el);
    l.text = el;
    this.renderText(id);
    this.touched();
  }

  /** One undo step for a text editing session that started with `before`. */
  commitText(id: string, before: TextElement | null, label = 'Edit text'): void {
    const l = this.layer(id);
    if (!l?.text) return;
    const after = $state.snapshot(l.text) as TextElement;
    if (before && JSON.stringify(before) === JSON.stringify(after)) return;
    const set = (el: TextElement | null) => {
      const ll = this.layer(id);
      if (!ll) return;
      ll.text = el ? { ...el } : null;
      if (el) this.renderText(id);
      else {
        ctxOf(this.canvas(id)).clearRect(0, 0, this.width, this.height);
        ll.rev++;
        this.invalidate(null);
      }
    };
    this.push({ label, bytes: 512, undo: () => set(before), redo: () => set(after) });
  }

  /* ------------------------------------------------------------ selection */

  setSelection(mask: Uint8Array | null): void {
    this.selection = mask;
    this.selCanvasCache = null;
    this.selBounds = mask ? maskBounds(mask, this.width, this.height) : null;
    if (!this.selBounds) this.selection = null;
    this.selRev++;
    this.repaint();
  }

  selectAll(): void {
    this.commitFloating();
    this.setSelection(rectMask(this.width, this.height, { x: 0, y: 0, w: this.width, h: this.height }));
  }
  deselect(): void {
    this.commitFloating();
    this.setSelection(null);
  }
  invertSelection(): void {
    this.commitFloating();
    this.setSelection(this.selection ? invertMask(this.selection) : rectMask(this.width, this.height, { x: 0, y: 0, w: this.width, h: this.height }));
  }

  /** The selection as an alpha mask canvas (for clipping with destination-in). */
  get selCanvas(): HTMLCanvasElement | null {
    if (!this.selection) return null;
    if (!this.selCanvasCache) {
      const c = newCanvas(this.width, this.height);
      const img = new ImageData(this.width, this.height);
      const m = this.selection;
      for (let i = 0; i < m.length; i++) img.data[i * 4 + 3] = m[i];
      ctxOf(c).putImageData(img, 0, 0);
      this.selCanvasCache = c;
    }
    return this.selCanvasCache;
  }

  /** Clip what was drawn into `c` (document-sized) inside `r` to the selection. */
  clipToSelection(c: CanvasRenderingContext2D, r: Rect): void {
    const sel = this.selCanvas;
    if (!sel) return;
    c.save();
    c.globalCompositeOperation = 'destination-in';
    c.drawImage(sel, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
    c.restore();
  }

  /** The selected pixels of the active layer (whole layer without a selection), cropped. */
  copyPixels(id = this.activeId): { canvas: HTMLCanvasElement; x: number; y: number } | null {
    const src = this.canvases.get(id);
    if (!src) return null;
    const b = this.selBounds ?? { x: 0, y: 0, w: this.width, h: this.height };
    const out = newCanvas(b.w, b.h);
    const c = ctxOf(out);
    c.drawImage(src, b.x, b.y, b.w, b.h, 0, 0, b.w, b.h);
    const sel = this.selCanvas;
    if (sel) {
      c.globalCompositeOperation = 'destination-in';
      c.drawImage(sel, b.x, b.y, b.w, b.h, 0, 0, b.w, b.h);
    }
    return { canvas: out, x: b.x, y: b.y };
  }

  /** Delete (make transparent) the selected pixels of the active layer. */
  clearSelection(): void {
    const a = this.active;
    if (!a || a.locked) return;
    if (this.floating) {
      // Deleting while moving throws the floating pixels away.
      const f = this.floating;
      this.floating = null;
      this.invalidate({ x: f.x, y: f.y, w: f.canvas.width, h: f.canvas.height });
      this.finishFloat(f, false);
      return;
    }
    const b = this.selBounds ?? { x: 0, y: 0, w: this.width, h: this.height };
    const sel = this.selCanvas;
    this.editPixels(a.id, b, 'Delete', (c) => {
      if (sel) {
        c.globalCompositeOperation = 'destination-out';
        c.drawImage(sel, b.x, b.y, b.w, b.h, b.x, b.y, b.w, b.h);
      } else c.clearRect(b.x, b.y, b.w, b.h);
    });
  }

  /** Fill the selection (or everything) with a colour. */
  fillSelection(color: string): void {
    const a = this.active;
    if (!a || a.locked) return;
    this.commitFloating();
    const b = this.selBounds ?? { x: 0, y: 0, w: this.width, h: this.height };
    const sel = this.selCanvas;
    const tmp = newCanvas(b.w, b.h);
    const t = ctxOf(tmp);
    t.fillStyle = color;
    t.fillRect(0, 0, b.w, b.h);
    if (sel) {
      t.globalCompositeOperation = 'destination-in';
      t.drawImage(sel, b.x, b.y, b.w, b.h, 0, 0, b.w, b.h);
    }
    this.editPixels(a.id, b, 'Fill selection', (c) => c.drawImage(tmp, b.x, b.y));
  }

  /* ------------------------------------------------------------ floating selection */

  private floatRecorders = new WeakMap<Floating, TileRecorder>();

  /** Lift the selected pixels of the active layer so they can be moved. */
  lift(): Floating | null {
    if (this.floating) return this.floating;
    const a = this.active;
    if (!a || a.locked || !this.selection || !this.selBounds) return null;
    const px = this.copyPixels(a.id);
    if (!px) return null;
    const canvas = this.canvas(a.id);
    const rec = new TileRecorder(this.width, this.height, this.readRect(canvas));
    const b = this.selBounds;
    rec.touch(b);
    const c = ctxOf(canvas);
    c.save();
    c.globalCompositeOperation = 'destination-out';
    c.drawImage(this.selCanvas!, b.x, b.y, b.w, b.h, b.x, b.y, b.w, b.h);
    c.restore();
    const f: Floating = { layerId: a.id, canvas: px.canvas, x: px.x, y: px.y, from: { x: px.x, y: px.y }, mask: this.selection, maskAt: { x: px.x, y: px.y } };
    this.floatRecorders.set(f, rec);
    this.floating = f;
    this.invalidate(b);
    return f;
  }

  /** Paste pixels as a floating selection on the active layer (or a new one when it is locked). */
  paste(src: HTMLCanvasElement, x: number, y: number): void {
    this.commitFloating();
    let a = this.active;
    if (!a || a.locked || a.text) a = this.addLayer('Pasted');
    const canvas = this.canvas(a.id);
    const rec = new TileRecorder(this.width, this.height, this.readRect(canvas));
    const mask = rectMask(this.width, this.height, { x, y, w: src.width, h: src.height });
    const f: Floating = { layerId: a.id, canvas: src, x, y, from: null, mask, maskAt: { x, y } };
    this.floatRecorders.set(f, rec);
    this.floating = f;
    this.selection = mask;
    this.selCanvasCache = null;
    this.selBounds = clipRect({ x, y, w: src.width, h: src.height }, this.width, this.height);
    this.selRev++;
    this.invalidate({ x, y, w: src.width, h: src.height });
  }

  moveFloating(x: number, y: number): void {
    const f = this.floating;
    if (!f) return;
    const old = { x: f.x, y: f.y, w: f.canvas.width, h: f.canvas.height };
    const next = { ...f, x: Math.round(x), y: Math.round(y) };
    this.floatRecorders.set(next, this.floatRecorders.get(f)!);
    this.floating = next;
    this.invalidate(unionRect(old, { x: next.x, y: next.y, w: f.canvas.width, h: f.canvas.height }));
    this.repaint();
  }

  /** Drop the floating pixels where they are (one undo step with the lift). */
  commitFloating(): void {
    const f = this.floating;
    if (!f) return;
    this.floating = null;
    this.finishFloat(f, true);
  }

  /** Esc: lifted pixels go back where they were; pasted ones are dropped. */
  cancelFloating(): void {
    const f = this.floating;
    if (!f) return;
    if (f.from) {
      this.floating = { ...f, x: f.from.x, y: f.from.y };
      this.floatRecorders.set(this.floating, this.floatRecorders.get(f)!);
      this.commitFloating();
      this.setSelection(f.mask);
      return;
    }
    this.floating = null;
    this.invalidate({ x: f.x, y: f.y, w: f.canvas.width, h: f.canvas.height });
    this.finishFloat(f, false);
    this.setSelection(null);
  }

  private finishFloat(f: Floating, drop: boolean): void {
    const rec = this.floatRecorders.get(f);
    const canvas = this.canvases.get(f.layerId);
    const l = this.layer(f.layerId);
    if (!rec || !canvas || !l) return;
    const dest = { x: f.x, y: f.y, w: f.canvas.width, h: f.canvas.height };
    if (drop) {
      rec.touch(clipRect(dest, this.width, this.height) ?? { x: 0, y: 0, w: 0, h: 0 });
      ctxOf(canvas).drawImage(f.canvas, f.x, f.y);
    }
    const patches = rec.finish();
    l.rev++;
    this.invalidate(dest);
    if (drop) {
      const mask = translateMask(f.mask, this.width, this.height, f.x - f.maskAt.x, f.y - f.maskAt.y);
      this.setSelection(mask);
    }
    if (!patches.length) return;
    const id = f.layerId;
    this.push({
      label: f.from ? 'Move selection' : 'Paste',
      bytes: patchBytes(patches),
      undo: () => this.applyPatches(id, patches, 'before'),
      redo: () => this.applyPatches(id, patches, 'after'),
    });
  }

  /* ------------------------------------------------------------ stroke overlay */

  /** The shared scratch canvas for strokes and shapes being drawn. */
  scratchCanvas(): HTMLCanvasElement {
    if (!this.scratch || this.scratch.width !== this.width || this.scratch.height !== this.height) this.scratch = newCanvas(this.width, this.height);
    return this.scratch;
  }

  beginOverlay(alpha: number, op: GlobalCompositeOperation): Overlay | null {
    const a = this.active;
    if (!a) return null;
    this.commitFloating();
    const canvas = this.scratchCanvas();
    ctxOf(canvas).clearRect(0, 0, canvas.width, canvas.height);
    this.overlay = { layerId: a.id, canvas, alpha, op, rect: null };
    return this.overlay;
  }

  /** Commit the overlay onto its layer as one undo step. */
  endOverlay(label: string): void {
    const ov = this.overlay;
    if (!ov) return;
    this.overlay = null;
    if (ov.rect) {
      const r = ov.rect;
      this.editPixels(ov.layerId, r, label, (c) => {
        c.globalAlpha = ov.alpha;
        c.globalCompositeOperation = ov.op;
        c.drawImage(ov.canvas, r.x, r.y, r.w, r.h, r.x, r.y, r.w, r.h);
      });
      ctxOf(ov.canvas).clearRect(r.x, r.y, r.w, r.h);
    }
  }

  cancelOverlay(): void {
    const ov = this.overlay;
    if (!ov) return;
    this.overlay = null;
    if (ov.rect) {
      ctxOf(ov.canvas).clearRect(ov.rect.x, ov.rect.y, ov.rect.w, ov.rect.h);
      this.invalidate(ov.rect);
    }
  }

  /* ------------------------------------------------------------ direct edits (retouch brushes) */

  private direct: { id: string; rec: TileRecorder } | null = null;

  /** Start editing the active layer's pixels in place, dab by dab (blur, smudge, clone). */
  beginDirect(): { id: string; ctx: CanvasRenderingContext2D } | null {
    const a = this.active;
    if (!a) return null;
    this.commitFloating();
    const c = this.canvas(a.id);
    this.direct = { id: a.id, rec: new TileRecorder(this.width, this.height, this.readRect(c)) };
    return { id: a.id, ctx: ctxOf(c) };
  }

  /** Call before changing pixels inside `r`. */
  directTouch(r: Rect): void {
    const c = clipRect(r, this.width, this.height);
    if (c) this.direct?.rec.touch(c);
  }

  endDirect(label: string): void {
    const d = this.direct;
    this.direct = null;
    const l = d && this.layer(d.id);
    if (!d || !l) return;
    const patches = d.rec.finish();
    l.rev++;
    if (!patches.length) return;
    const text = l.text;
    if (text) l.text = null;
    const id = d.id;
    this.push({
      label,
      bytes: patchBytes(patches),
      undo: () => {
        this.applyPatches(id, patches, 'before');
        const ll = this.layer(id);
        if (ll && text) ll.text = text;
      },
      redo: () => this.applyPatches(id, patches, 'after'),
    });
  }

  /* ------------------------------------------------------------ image operations */

  private transformAll(nw: number, nh: number, draw: (c: CanvasRenderingContext2D, src: HTMLCanvasElement) => void, keepText = false): void {
    for (const l of this.layers) {
      const src = this.canvas(l.id);
      const out = newCanvas(nw, nh);
      const c = ctxOf(out);
      c.imageSmoothingQuality = 'high';
      draw(c, src);
      this.canvases.set(l.id, out);
      if (!keepText) l.text = null;
      l.rev++;
    }
    this.width = nw;
    this.height = nh;
    this.resized();
  }

  resizeImage(nw: number, nh: number): void {
    nw = Math.max(1, Math.round(nw));
    nh = Math.max(1, Math.round(nh));
    if (nw === this.width && nh === this.height) return;
    this.structural('Resize image', () =>
      this.transformAll(nw, nh, (c, src) => {
        // Halve first when shrinking a lot, so the result doesn't alias.
        let cur: HTMLCanvasElement = src;
        while (cur.width / 2 >= nw && cur.height / 2 >= nh) {
          const half = newCanvas(cur.width / 2, cur.height / 2);
          const hc = ctxOf(half);
          hc.imageSmoothingQuality = 'high';
          hc.drawImage(cur, 0, 0, half.width, half.height);
          cur = half;
        }
        c.drawImage(cur, 0, 0, nw, nh);
      }),
    );
  }

  /** New canvas size; the image is placed by `anchor` (0..2 each), new area filled with `fill` on the bottom layer (null: transparent). */
  resizeCanvas(nw: number, nh: number, ax: number, ay: number, fill: string | null = null): void {
    nw = Math.max(1, Math.round(nw));
    nh = Math.max(1, Math.round(nh));
    if (nw === this.width && nh === this.height) return;
    const off = anchorOffset(this.width, this.height, nw, nh, ax, ay);
    const bottom = this.layers[0]?.id;
    this.structural('Canvas size', () =>
      this.transformAll(
        nw,
        nh,
        (c, src) => {
          if (fill && src === this.canvases.get(bottom)) {
            c.fillStyle = fill;
            c.fillRect(0, 0, nw, nh);
          }
          c.drawImage(src, off.x, off.y);
        },
        false,
      ),
    );
  }

  crop(r: Rect): void {
    const c = clipRect(r, this.width, this.height);
    if (!c || (c.w === this.width && c.h === this.height)) return;
    this.structural('Crop', () => this.transformAll(c.w, c.h, (ctx, src) => ctx.drawImage(src, -c.x, -c.y)));
  }

  cropToSelection(): void {
    this.commitFloating();
    if (this.selBounds) this.crop(this.selBounds);
  }

  /** Turn the whole image clockwise by 90, 180 or 270 degrees. */
  rotate(deg: 90 | 180 | 270): void {
    const quarter = deg !== 180;
    const nw = quarter ? this.height : this.width;
    const nh = quarter ? this.width : this.height;
    const ow = this.width;
    const oh = this.height;
    this.structural(deg === 180 ? 'Rotate 180°' : deg === 90 ? 'Rotate right' : 'Rotate left', () =>
      this.transformAll(nw, nh, (c, src) => {
        c.translate(nw / 2, nh / 2);
        c.rotate((deg * Math.PI) / 180);
        c.drawImage(src, -ow / 2, -oh / 2);
      }),
    );
  }

  flip(dir: 'h' | 'v', layerOnly = false): void {
    const draw = (c: CanvasRenderingContext2D, src: HTMLCanvasElement) => {
      if (dir === 'h') {
        c.translate(this.width, 0);
        c.scale(-1, 1);
      } else {
        c.translate(0, this.height);
        c.scale(1, -1);
      }
      c.drawImage(src, 0, 0);
    };
    if (layerOnly) {
      const a = this.active;
      if (!a || a.locked) return;
      this.commitFloating();
      const copy = cloneCanvas(this.canvas(a.id));
      this.editPixels(a.id, { x: 0, y: 0, w: this.width, h: this.height }, dir === 'h' ? 'Flip layer' : 'Flip layer', (c) => {
        c.clearRect(0, 0, this.width, this.height);
        draw(c, copy);
      });
      return;
    }
    this.structural(dir === 'h' ? 'Flip horizontally' : 'Flip vertically', () => this.transformAll(this.width, this.height, draw));
  }

  /* ------------------------------------------------------------ output */

  /** The flattened image (what Export saves). */
  flattened(background: string | null = null): HTMLCanvasElement {
    this.commitFloating();
    const out = newCanvas(this.width, this.height);
    const c = ctxOf(out);
    if (background) {
      c.fillStyle = background;
      c.fillRect(0, 0, this.width, this.height);
    }
    c.drawImage(this.composite, 0, 0);
    return out;
  }

  /** Whether any pixel of the composite is not fully opaque. */
  hasTransparency(): boolean {
    const d = this.composite.getContext('2d')!.getImageData(0, 0, this.width, this.height).data;
    for (let i = 3; i < d.length; i += 4) if (d[i] < 255) return true;
    return false;
  }
}
