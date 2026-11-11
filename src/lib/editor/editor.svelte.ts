/** Editor state: the open document, selection, view and undo history. */
import { produce, type Draft } from 'immer';
import { addToLayout, layoutAssetOrder, relayout, removeFromLayout } from '../collage/layout';
import { cloneElement, createDoc, createPhoto, createText, nowIso, shortSide } from '../model/factory';
import type {
  CollageDoc,
  CollageElement,
  LayoutEngine,
  LayoutSettings,
  PhotoElement,
  TextElement,
} from '../model/types';
import { syncTextHeight } from '../render/text';
import {
  activePage,
  allPages,
  blankPage,
  currentContent,
  duplicatePage,
  insertPages,
  movePage,
  pageCount,
  removePage,
  replaceCurrent,
  switchPage,
} from '../model/multipage';
import type { PageContent } from '../model/types';
import { fitToPage, resizeDoc } from './pageSize';
import { AUTO_PLACEHOLDER } from '../templates/instantiate';
import { library } from '../stores/library.svelte';
import { boundsOf } from './geometry';

export type Tool = 'photos' | 'layouts' | 'templates' | 'text' | 'stickers' | 'shapes' | 'calendar' | 'background' | 'page';

export type Align = 'left' | 'hcenter' | 'right' | 'top' | 'vcenter' | 'bottom';

interface UpdateOptions {
  /** Coalesce consecutive updates with the same key into one undo step (sliders). */
  key?: string;
  /** Record in the undo history (default true; false inside drag transactions). */
  history?: boolean;
}

const HISTORY_LIMIT = 150;
const COALESCE_MS = 1200;

class Editor {
  doc = $state.raw<CollageDoc>(createDoc({ width: 3508, height: 2480 }));
  selection = $state<string[]>([]);
  tool = $state<Tool>('photos');
  cropTarget = $state<string | null>(null);
  editingText = $state<string | null>(null);
  zoom = $state(0.2);
  panX = $state(0);
  panY = $state(0);
  fit = $state(true);
  canUndo = $state(false);
  canRedo = $state(false);
  dirty = $state(false);
  open = $state(false);
  /** External .tessera file this document was opened from / saved to. */
  projectPath = $state<string | null>(null);
  layoutHistoryPos = $state(0);
  layoutHistoryLen = $state(0);

  private past: CollageDoc[] = [];
  private future: CollageDoc[] = [];
  private txBase: CollageDoc | null = null;
  private lastKey: string | null = null;
  private lastTime = 0;
  private clipboard: CollageElement[] = [];
  private layoutHistory: { seed: number; order: string[] }[] = [];

  /* ------------------------------------------------------------ queries */

  get selected(): CollageElement[] {
    const set = new Set(this.selection);
    return this.doc.elements.filter((e) => set.has(e.id));
  }

  get single(): CollageElement | null {
    return this.selection.length === 1 ? (this.doc.elements.find((e) => e.id === this.selection[0]) ?? null) : null;
  }

  element(id: string): CollageElement | undefined {
    return this.doc.elements.find((e) => e.id === id);
  }

  get pageShort(): number {
    return shortSide(this.doc);
  }

  get hasLayout(): boolean {
    return this.doc.layout.engine !== 'none';
  }

  /* ----------------------------------------------------------- lifecycle */

  load(doc: CollageDoc, projectPath: string | null = null): void {
    this.doc = doc;
    this.past = [];
    this.future = [];
    this.txBase = null;
    this.selection = [];
    this.cropTarget = null;
    this.editingText = null;
    this.projectPath = projectPath;
    this.dirty = false;
    this.fit = true;
    this.open = true;
    this.layoutHistory = [];
    this.recordLayout();
    this.syncFlags();
  }

  /* ------------------------------------------------------------- history */

  private syncFlags(): void {
    this.canUndo = this.past.length > 0;
    this.canRedo = this.future.length > 0;
  }

  private commit(next: CollageDoc, opts: UpdateOptions = {}): void {
    if (next === this.doc) return;
    const prev = this.doc;
    const now = performance.now();
    const record = opts.history !== false && this.txBase === null;
    if (record) {
      const coalesce = opts.key && opts.key === this.lastKey && now - this.lastTime < COALESCE_MS;
      if (!coalesce) {
        this.past.push(prev);
        if (this.past.length > HISTORY_LIMIT) this.past.shift();
      }
      this.future = [];
      this.lastKey = opts.key ?? null;
      this.lastTime = now;
    }
    this.doc = { ...next, modified: nowIso() };
    this.dirty = true;
    this.syncFlags();
  }

  update(recipe: (d: Draft<CollageDoc>) => void, opts: UpdateOptions = {}): void {
    this.commit(produce(this.doc, recipe), opts);
  }

  replace(doc: CollageDoc, opts: UpdateOptions = {}): void {
    this.commit(doc, opts);
  }

  /** Start a gesture: intermediate updates are not recorded until `end()`. */
  begin(): void {
    if (this.txBase === null) this.txBase = this.doc;
  }

  end(): void {
    const base = this.txBase;
    this.txBase = null;
    if (base && base !== this.doc) {
      this.past.push(base);
      if (this.past.length > HISTORY_LIMIT) this.past.shift();
      this.future = [];
      this.lastKey = null;
      this.syncFlags();
    }
  }

  undo(): void {
    if (this.txBase) this.end();
    const prev = this.past.pop();
    if (!prev) return;
    this.future.push(this.doc);
    this.doc = prev;
    this.lastKey = null;
    this.pruneSelection();
    this.dirty = true;
    this.syncFlags();
  }

  redo(): void {
    const next = this.future.pop();
    if (!next) return;
    this.past.push(this.doc);
    this.doc = next;
    this.lastKey = null;
    this.pruneSelection();
    this.dirty = true;
    this.syncFlags();
  }

  private pruneSelection(): void {
    const ids = new Set(this.doc.elements.map((e) => e.id));
    this.selection = this.selection.filter((id) => ids.has(id));
    if (this.cropTarget && !ids.has(this.cropTarget)) this.cropTarget = null;
  }

  /* ----------------------------------------------------------- selection */

  select(ids: string[], additive = false): void {
    if (additive) {
      const set = new Set(this.selection);
      for (const id of ids) {
        if (set.has(id)) set.delete(id);
        else set.add(id);
      }
      this.selection = [...set];
    } else this.selection = ids;
    if (this.cropTarget && !this.selection.includes(this.cropTarget)) this.cropTarget = null;
  }

  selectAll(): void {
    this.selection = this.doc.elements.filter((e) => !e.hidden && !e.locked).map((e) => e.id);
  }

  clearSelection(): void {
    this.selection = [];
    this.cropTarget = null;
  }

  /* ------------------------------------------------------------ elements */

  /** Patch one element (merged shallowly). */
  patch(id: string, patch: Partial<CollageElement>, opts: UpdateOptions = {}): void {
    this.update((d) => {
      const el = d.elements.find((e) => e.id === id);
      if (!el) return;
      Object.assign(el, patch);
      if (el.kind === 'text') syncTextHeight(el as TextElement);
    }, opts);
  }

  /** Patch every selected element of a kind. */
  patchSelected(patch: Partial<CollageElement> | ((el: Draft<CollageElement>) => void), opts: UpdateOptions = {}): void {
    const set = new Set(this.selection);
    this.update((d) => {
      for (const el of d.elements) {
        if (!set.has(el.id)) continue;
        if (typeof patch === 'function') patch(el);
        else Object.assign(el, patch);
        if (el.kind === 'text') syncTextHeight(el as TextElement);
      }
    }, opts);
  }

  addElements(els: CollageElement[], select = true): void {
    this.update((d) => {
      for (const el of els) d.elements.push(el as Draft<CollageElement>);
    });
    if (select) this.selection = els.map((e) => e.id);
  }

  /** Centre an element on the page (or at a point) and add it. */
  place(el: CollageElement, at?: { x: number; y: number }): void {
    const c = at ?? { x: this.doc.page.width / 2, y: this.doc.page.height / 2 };
    el.x = c.x - el.w / 2;
    el.y = c.y - el.h / 2;
    this.addElements([el]);
  }

  deleteSelected(): void {
    if (!this.selection.length) return;
    const set = new Set(this.selection);
    const removedManaged = this.selected.filter((e): e is PhotoElement => e.kind === 'photo' && !!e.slot);
    let next = produce(this.doc, (d) => {
      d.elements = d.elements.filter((e) => !set.has(e.id) || e.locked);
    });
    if (removedManaged.length && next.layout.engine !== 'none') {
      next = relayout(next, { order: layoutAssetOrder(next) });
    }
    this.commit(next);
    this.selection = [];
    this.cropTarget = null;
  }

  duplicateSelected(): void {
    const offset = this.pageShort * 0.03;
    const copies = this.selected.map((e) => cloneElement(e, offset));
    if (copies.length) this.addElements(copies);
  }

  copy(): void {
    this.clipboard = this.selected.map((e) => structuredClone(e));
  }

  paste(): void {
    if (!this.clipboard.length) return;
    const offset = this.pageShort * 0.03;
    const copies = this.clipboard.map((e) => cloneElement(e, offset));
    this.clipboard = copies.map((e) => structuredClone(e));
    this.addElements(copies);
  }

  nudge(dx: number, dy: number): void {
    const set = new Set(this.selection);
    this.update(
      (d) => {
        for (const el of d.elements) {
          if (set.has(el.id) && !el.locked && !(el.kind === 'photo' && el.slot)) {
            el.x += dx;
            el.y += dy;
          }
        }
      },
      { key: 'nudge' },
    );
  }

  arrange(how: 'front' | 'back' | 'forward' | 'backward'): void {
    const set = new Set(this.selection);
    this.update((d) => {
      const els = d.elements;
      if (how === 'front' || how === 'back') {
        const picked = els.filter((e) => set.has(e.id));
        const rest = els.filter((e) => !set.has(e.id));
        d.elements = how === 'front' ? [...rest, ...picked] : [...picked, ...rest];
      } else if (how === 'forward') {
        for (let i = els.length - 2; i >= 0; i--) {
          if (set.has(els[i].id) && !set.has(els[i + 1].id)) [els[i], els[i + 1]] = [els[i + 1], els[i]];
        }
      } else {
        for (let i = 1; i < els.length; i++) {
          if (set.has(els[i].id) && !set.has(els[i - 1].id)) [els[i], els[i - 1]] = [els[i - 1], els[i]];
        }
      }
    });
  }

  /** Move an element to a z-index (layers panel). */
  moveTo(id: string, index: number): void {
    this.update((d) => {
      const i = d.elements.findIndex((e) => e.id === id);
      if (i < 0) return;
      const [el] = d.elements.splice(i, 1);
      d.elements.splice(Math.max(0, Math.min(d.elements.length, index)), 0, el);
    });
  }

  align(how: Align): void {
    const sel = this.selected.filter((e) => !e.locked);
    if (!sel.length) return;
    const ref =
      sel.length === 1 ? { x: 0, y: 0, w: this.doc.page.width, h: this.doc.page.height } : boundsOf(sel)!;
    const set = new Set(sel.map((e) => e.id));
    this.update((d) => {
      for (const el of d.elements) {
        if (!set.has(el.id)) continue;
        const b = boundsOf([el as CollageElement])!;
        let dx = 0;
        let dy = 0;
        if (how === 'left') dx = ref.x - b.x;
        if (how === 'right') dx = ref.x + ref.w - (b.x + b.w);
        if (how === 'hcenter') dx = ref.x + ref.w / 2 - (b.x + b.w / 2);
        if (how === 'top') dy = ref.y - b.y;
        if (how === 'bottom') dy = ref.y + ref.h - (b.y + b.h);
        if (how === 'vcenter') dy = ref.y + ref.h / 2 - (b.y + b.h / 2);
        el.x += dx;
        el.y += dy;
        if (el.kind === 'photo') el.slot = false;
      }
    });
  }

  /* -------------------------------------------------------------- photos */

  ensureAsset(d: Draft<CollageDoc>, assetId: string): void {
    if (!d.assets[assetId]) {
      const ref = library.assetRef(assetId);
      if (ref) d.assets[assetId] = ref;
    }
  }

  /** Put an asset into a photo element (drop on a frame). */
  setPhotoAsset(id: string, assetId: string): void {
    this.update((d) => {
      this.ensureAsset(d, assetId);
      const el = d.elements.find((e) => e.id === id);
      if (el && el.kind === 'photo') {
        el.asset = assetId;
        el.crop = { x: 0.5, y: 0.5, zoom: 1 };
      }
    });
  }

  /** Swap the photos shown by two photo elements (PhotoCollage's drag-to-swap). */
  swapPhotos(a: string, b: string): void {
    this.update((d) => {
      const ea = d.elements.find((e) => e.id === a);
      const eb = d.elements.find((e) => e.id === b);
      if (!ea || !eb || ea.kind !== 'photo' || eb.kind !== 'photo') return;
      [ea.asset, eb.asset] = [eb.asset, ea.asset];
      [ea.crop, eb.crop] = [eb.crop, ea.crop];
      [ea.adjust, eb.adjust] = [eb.adjust, ea.adjust];
      [ea.filter, eb.filter] = [eb.filter, ea.filter];
    });
  }

  /** Add photos as free elements, cascading from a point. */
  addFreePhotos(assetIds: string[], at?: { x: number; y: number }): void {
    const S = this.pageShort;
    const base = at ?? { x: this.doc.page.width / 2, y: this.doc.page.height / 2 };
    const els: PhotoElement[] = assetIds.map((id, i) => {
      const ref = library.assetRef(id);
      const aspect = ref ? ref.width / ref.height : 1.5;
      const size = S * 0.42;
      const w = aspect >= 1 ? size : size * aspect;
      const h = aspect >= 1 ? size / aspect : size;
      const off = i * S * 0.035;
      return createPhoto({ x: base.x - w / 2 + off, y: base.y - h / 2 + off, w, h, asset: id });
    });
    this.update((d) => {
      for (const id of assetIds) this.ensureAsset(d, id);
      for (const el of els) d.elements.push(el as Draft<CollageElement>);
    });
    this.selection = els.map((e) => e.id);
  }

  /** Add photos to the auto-layout (switching to Mosaic when in freestyle). */
  addToLayout(assetIds: string[]): void {
    let doc = produce(this.doc, (d) => {
      for (const id of assetIds) this.ensureAsset(d, id);
      if (d.layout.engine === 'none') d.layout.engine = 'mosaic';
      // Template preview frames make way for real photos.
      d.elements = d.elements.filter((e) => e.name !== AUTO_PLACEHOLDER);
    });
    doc = addToLayout(doc, assetIds);
    this.commit(doc);
    this.recordLayout();
  }

  removeFromLayout(elementId: string): void {
    const el = this.element(elementId);
    if (!el || el.kind !== 'photo' || !el.asset) return;
    this.commit(removeFromLayout(this.doc, el.asset));
    this.selection = this.selection.filter((s) => s !== elementId);
    this.recordLayout();
  }

  /** Free a managed photo so it can be moved/resized by hand. */
  detach(id: string): void {
    this.patch(id, { slot: false } as Partial<PhotoElement>);
  }

  /** Turn every managed photo into a free element (switch to freestyle). */
  unlockLayout(): void {
    this.update((d) => {
      for (const e of d.elements) if (e.kind === 'photo') e.slot = false;
      d.layout.engine = 'none';
    });
  }

  /* -------------------------------------------------------------- layout */

  private recordLayout(): void {
    if (this.doc.layout.engine === 'none') {
      this.layoutHistory = [];
    } else {
      const entry = { seed: this.doc.layout.seed, order: layoutAssetOrder(this.doc) };
      this.layoutHistory = this.layoutHistory.slice(0, this.layoutHistoryPos + 1);
      this.layoutHistory.push(entry);
      if (this.layoutHistory.length > 40) this.layoutHistory.shift();
    }
    this.layoutHistoryPos = this.layoutHistory.length - 1;
    this.layoutHistoryLen = this.layoutHistory.length;
  }

  setEngine(engine: LayoutEngine): void {
    if (engine === 'none') {
      this.unlockLayout();
      return;
    }
    const fromFreestyle = this.doc.layout.engine === 'none';
    let doc = produce(this.doc, (d) => {
      d.layout.engine = engine;
      // Freestyle → layout: adopt every photo on the page.
      if (fromFreestyle) for (const e of d.elements) if (e.kind === 'photo' && e.asset) e.slot = true;
      d.elements = d.elements.filter((e) => e.name !== AUTO_PLACEHOLDER);
    });
    doc = relayout(doc, { restyle: true });
    this.commit(doc);
    this.recordLayout();
  }

  /** PhotoCollage's "Regenerate": a brand new random arrangement. */
  regenerate(): void {
    if (!this.hasLayout) return;
    this.commit(relayout(this.doc, { shuffle: true }));
    this.recordLayout();
  }

  layoutBack(): void {
    if (this.layoutHistoryPos <= 0) return;
    this.layoutHistoryPos--;
    const h = this.layoutHistory[this.layoutHistoryPos];
    this.commit(relayout(this.doc, { seed: h.seed, order: h.order }));
  }

  layoutForward(): void {
    if (this.layoutHistoryPos >= this.layoutHistory.length - 1) return;
    this.layoutHistoryPos++;
    const h = this.layoutHistory[this.layoutHistoryPos];
    this.commit(relayout(this.doc, { seed: h.seed, order: h.order }));
  }

  setLayout(patch: Partial<LayoutSettings>, key?: string): void {
    const doc = produce(this.doc, (d) => {
      Object.assign(d.layout, patch);
    });
    this.commit(relayout(doc), { key });
  }

  /** Re-run the layout without touching history (e.g. a font finished loading). */
  refreshLayoutQuietly(): void {
    if (this.doc.layout.engine !== 'letters') return;
    const next = relayout(this.doc);
    if (next !== this.doc) {
      this.doc = next;
      // Keep undo entries that point at the old layout meaningful: replace the tip.
    }
  }

  /** Apply a style patch to every photo managed by the layout. */
  styleLayoutPhotos(fn: (p: Draft<PhotoElement>) => void, key?: string): void {
    this.update((d) => {
      for (const e of d.elements) if (e.kind === 'photo' && e.slot) fn(e);
    }, { key });
  }

  /* ---------------------------------------------------------------- page */

  /** Resize the page (every page of a multi-page project), scaling content proportionally. */
  setPage(width: number, height: number, dpi: number, preset?: string): void {
    this.commit(resizeDoc(this.doc, width, height, dpi, preset));
  }

  /* --------------------------------------------------------------- pages */

  get pageCount(): number {
    return pageCount(this.doc);
  }

  get activePage(): number {
    return activePage(this.doc);
  }

  private afterPageChange(): void {
    this.selection = [];
    this.cropTarget = null;
    this.editingText = null;
    this.layoutHistory = [];
    this.recordLayout();
  }

  /** Show another page. Not an undo step by itself. */
  goToPage(i: number): void {
    if (i === this.activePage) return;
    this.commit(switchPage(this.doc, i), { history: false });
    this.afterPageChange();
  }

  addPage(content?: PageContent): void {
    this.commit(insertPages(this.doc, [content ?? blankPage(currentContent(this.doc))]));
    this.afterPageChange();
  }

  addPages(contents: PageContent[], after?: number): void {
    if (!contents.length) return;
    this.commit(insertPages(this.doc, contents, after));
    this.afterPageChange();
  }

  duplicatePage(i = this.activePage): void {
    this.commit(duplicatePage(this.doc, i));
    this.afterPageChange();
  }

  removePage(i = this.activePage): void {
    if (this.pageCount <= 1) return;
    this.commit(removePage(this.doc, i));
    this.afterPageChange();
  }

  movePage(from: number, to: number): void {
    this.commit(movePage(this.doc, from, to));
  }

  /** Replace the content of the active page (e.g. with a template), fitted to the page size. */
  replaceCurrentPage(source: CollageDoc): void {
    const fitted = fitToPage(source, this.doc.page);
    const doc = replaceCurrent(
      { ...this.doc, assets: { ...this.doc.assets, ...fitted.assets } },
      { background: fitted.background, frame: fitted.frame, layout: fitted.layout, elements: fitted.elements },
    );
    this.commit(doc);
    this.afterPageChange();
  }


  /* ---------------------------------------------------------------- text */

  addText(preset: Partial<TextElement>, at?: { x: number; y: number }): TextElement {
    const W = this.doc.page.width;
    const el = createText({ x: 0, y: 0, w: W * 0.6, size: this.pageShort * 0.08, ...preset });
    if (preset.size === undefined && preset.font === undefined) el.size = this.pageShort * 0.08;
    syncTextHeight(el);
    this.place(el, at);
    return el;
  }

  /* ---------------------------------------------------------------- misc */

  rename(name: string): void {
    this.update((d) => {
      d.name = name.trim() || 'Untitled collage';
    });
  }

  markSaved(): void {
    this.dirty = false;
  }

  /** All asset ids referenced by the document. */
  usedAssets(): Set<string> {
    const out = new Set<string>();
    for (const p of allPages(this.doc)) {
      for (const e of p.elements) if (e.kind === 'photo' && e.asset) out.add(e.asset);
      if (p.background.image) out.add(p.background.image.asset);
    }
    return out;
  }
}

export { syncTextHeight };

export const editor = new Editor();
