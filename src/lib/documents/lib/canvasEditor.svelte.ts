// State and commands of the canvas editor (Canvas.svelte and its toolbars, overlay,
// pointer and keyboard handling share one CanvasEditor). Everything that changes the
// board goes through `before()` (undo snapshot) and `change()` (autosave).
import { tick } from 'svelte';
import { api, newId } from './api';
import {
  alignRects,
  arrowRect,
  autoPlace,
  CanvasHistory,
  clampZ,
  cloneItem,
  CLIP_PREFIX,
  containsRect,
  contentRect,
  defaultCardSize,
  detachArrows,
  distributeRects,
  ensureCanvas,
  expand,
  fitView,
  gridLayout,
  isBox,
  newBox,
  newSticker,
  offsetClip,
  parseClip,
  remapClip,
  snap,
  snapAngle,
  steps,
  targets,
  toWorld,
  union,
  visibleRect,
  whenResolved,
  zoomAt,
  zOf,
  arrowPoints,
  type Align,
  type View,
} from './canvas';
import { exportCanvasPNG } from './canvasExport';
import { ensureFont } from '../../assets/fonts';
import type { StickerInfo } from '../../stores/stickers.svelte';
import type { TextLook } from '../../text/styles';
import type { DocState } from './docstate.svelte';
import { newSection } from './docstate.svelte';
import { middleClickPaste } from '../../platform/middleclick';
import { prefs } from './prefs.svelte';
import { resolver } from './resolver.svelte';
import { router } from './router.svelte';
import { toasts } from './toast.svelte';
import type { ArrowEnd, ArrowItem, BoxItem, CanvasItem, Layout, MediaRef, NoteItem, Rect, Section, ShapeItem, StickerItem } from './types';

export type Tool = 'select' | 'hand' | 'card' | 'note' | 'text' | 'rect' | 'ellipse' | 'arrow' | 'frame' | 'pen' | 'marker';
export const TOOLS: { id: Tool; icon: string; label: string; key: string }[] = [
  { id: 'select', icon: 'pointer', label: 'Select', key: 'V' },
  { id: 'hand', icon: 'hand', label: 'Pan (or hold Space / middle mouse)', key: 'H' },
  { id: 'card', icon: 'card', label: 'Section card', key: 'C' },
  { id: 'note', icon: 'sticky', label: 'Sticky note', key: 'N' },
  { id: 'text', icon: 'type', label: 'Text', key: 'T' },
  { id: 'rect', icon: 'square', label: 'Rectangle', key: 'R' },
  { id: 'ellipse', icon: 'circle', label: 'Ellipse', key: 'O' },
  { id: 'arrow', icon: 'arrow-ur', label: 'Arrow (drag from one item to another to connect them)', key: 'A' },
  { id: 'frame', icon: 'frame', label: 'Frame (presentation step)', key: 'F' },
  { id: 'pen', icon: 'pen', label: 'Pen (freehand)', key: 'P' },
  { id: 'marker', icon: 'marker', label: 'Highlighter', key: 'M' },
];

export function pref(k: string, d: string) {
  try {
    return localStorage.getItem('letopis.cv.' + k) ?? d;
  } catch {
    return d;
  }
}
export function setPref(k: string, v: string) {
  try {
    localStorage.setItem('letopis.cv.' + k, v);
  } catch {}
}

/** focus is in a text field (or the section drawer): keys belong to it */
export function typing(t: EventTarget | null) {
  const el = t as HTMLElement | null;
  return !!el && (el.isContentEditable || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || !!el.closest?.('.drawer'));
}

export type Preview =
  | { kind: 'rect'; rect: Rect; shape: string }
  | { kind: 'arrow'; p1: { x: number; y: number }; p2: { x: number; y: number }; target: string | null }
  | null;

type Flags = { group?: string; locked?: boolean; reveal?: number };

export class CanvasEditor {
  readonly ds: DocState;
  readonly hist = new CanvasHistory();

  constructor(ds: DocState) {
    this.ds = ds;
  }

  // ---------- document lookups ----------
  doc = $derived.by(() => this.ds.doc!);
  items = $derived(this.doc.canvas?.items ?? []);
  itemMap = $derived(new Map(this.items.map((i) => [i.id, i])));
  secMap = $derived(new Map(this.doc.sections.map((s) => [s.id, s])));
  tg = $derived(targets(this.doc));

  // ---------- view ----------
  vp = $state<HTMLDivElement>();
  vw = $state(0);
  vh = $state(0);
  view = $state<View>({ tx: 0, ty: 0, z: 1 });
  lod = $state(1);
  renderRect = $state<Rect>({ x: 0, y: 0, w: 0, h: 0 });
  mountRect = $state<Rect | null>(null);
  playRect = $state<Rect | null>(null);
  moving = $state(false);
  ready = $state(false);

  // ---------- editing state ----------
  tool = $state<Tool>('select');
  selected = $state(new Set<string>());
  editing = $state<string | null>(null);
  /** just created (an empty note is dropped again on blur) */
  fresh: string | null = null;
  drawer = $state<string | null>(null);
  help = $state(false);
  spaceDown = $state(false);
  gridOn = $state(pref('grid', '1') === '1');
  wheelZoom = $state(pref('wheel', 'zoom') === 'zoom');
  dropHint = $state(false);
  preview = $state<Preview>(null);
  editEl = $state<HTMLTextAreaElement>();
  editingCard = $state<string | null>(null);
  guides = $state<{ x: number[]; y: number[] }>({ x: [], y: [] });
  showMinimap = $state(pref('minimap', '1') === '1');
  inkColor = $state(pref('ink', 'red'));
  inkPath = $state<[number, number][] | null>(null);
  arrangeMenu = $state(false);
  exportMenu = $state(false);
  stickerPicker = $state(false);
  styleMenu = $state(false);
  ctxW = $state(360);
  panning = $state(false);
  /** something is being moved/resized (hides the context toolbar) */
  dragging = $state(false);

  // ---------- selection ----------
  selItems = $derived([...this.selected].map((id) => this.itemMap.get(id)).filter(Boolean) as CanvasItem[]);
  selCards = $derived([...this.selected].map((id) => this.secMap.get(id)).filter(Boolean) as Section[]);
  single = $derived(this.selected.size === 1 ? [...this.selected][0] : null);
  singleArrow = $derived(this.single && this.itemMap.get(this.single)?.type === 'arrow' ? (this.itemMap.get(this.single) as ArrowItem) : null);
  singleSticker = $derived(this.single && this.itemMap.get(this.single)?.type === 'sticker' ? (this.itemMap.get(this.single) as StickerItem) : null);
  selReveal = $derived([...this.selected].map((id) => this.flagsOf(id)?.reveal ?? 0).reduce((a, b) => Math.max(a, b), 0));
  selLocked = $derived(this.selected.size > 0 && [...this.selected].every((id) => !!this.flagsOf(id)?.locked));
  selGrouped = $derived([...this.selected].some((id) => !!this.flagsOf(id)?.group));
  editItem = $derived(this.editing ? (this.itemMap.get(this.editing) as NoteItem | ShapeItem | undefined) : undefined);

  rectOf(id: string): Rect | null {
    const s = this.secMap.get(id);
    if (s) return s.canvas && !s.canvas.hidden ? s.canvas : null;
    const it = this.itemMap.get(id);
    if (!it) return null;
    return isBox(it) ? it : arrowRect(it, this.tg);
  }

  allIds(): string[] {
    const ids = this.doc.sections.filter((s) => s.canvas && !s.canvas.hidden).map((s) => s.id);
    for (const it of this.items) ids.push(it.id);
    return ids;
  }

  select(ids: Iterable<string>) {
    this.selected = new Set(ids);
    const cards = [...this.selected].filter((id) => this.secMap.has(id));
    this.ds.activeSection = cards.length === 1 ? cards[0] : null;
  }

  change() {
    this.ds.touch();
  }
  before() {
    this.hist.push(this.doc);
  }

  // ---------- view / level of detail ----------
  private settleTimer: ReturnType<typeof setTimeout> | undefined;
  private viewKey = '';
  private anim = 0;

  settle() {
    this.moving = false;
    if (!this.vw || !this.vh) return;
    const live = visibleRect(this.view, this.vw, this.vh);
    this.lod = this.view.z;
    this.renderRect = expand(live, 1);
    this.mountRect = expand(live, 0.3);
    this.playRect = live;
    try {
      localStorage.setItem(this.viewKey, JSON.stringify(this.view));
    } catch {}
  }

  setView(v: View, settleNow = false) {
    this.view = v;
    if (!this.vw) return;
    const live = visibleRect(v, this.vw, this.vh);
    // keep a margin of rendered (cheap) items around the viewport while moving
    if (!containsRect(this.renderRect, live) || v.z < this.lod / 1.8) this.renderRect = expand(live, 1);
    clearTimeout(this.settleTimer);
    if (settleNow) this.settle();
    else {
      this.moving = true;
      this.settleTimer = setTimeout(() => this.settle(), 140);
    }
  }

  stopAnim() {
    cancelAnimationFrame(this.anim);
  }

  animateTo(target: View, ms = 420) {
    cancelAnimationFrame(this.anim);
    const from = { ...this.view };
    const t0 = performance.now();
    const { vw, vh } = this;
    const step = (t: number) => {
      const k = Math.min(1, (t - t0) / ms);
      const e = 1 - Math.pow(1 - k, 3);
      const lz = Math.log(from.z) + (Math.log(target.z) - Math.log(from.z)) * e;
      const z = Math.exp(lz);
      // interpolate the world point at the viewport centre so the motion is straight
      const c0 = { x: (vw / 2 - from.tx) / from.z, y: (vh / 2 - from.ty) / from.z };
      const c1 = { x: (vw / 2 - target.tx) / target.z, y: (vh / 2 - target.ty) / target.z };
      const cx = c0.x + (c1.x - c0.x) * e,
        cy = c0.y + (c1.y - c0.y) * e;
      this.setView({ z, tx: vw / 2 - cx * z, ty: vh / 2 - cy * z });
      if (k < 1) this.anim = requestAnimationFrame(step);
    };
    this.anim = requestAnimationFrame(step);
  }

  zoomToRect(r: Rect, pad = 0.08, maxZ = 1.25) {
    if (!this.vw) return;
    this.animateTo(fitView(r, this.vw, this.vh, pad, maxZ));
  }

  fitAll() {
    const r = contentRect(this.doc);
    if (r) this.zoomToRect(r, 0.06, 1);
    else this.animateTo({ z: 1, tx: this.vw / 2, ty: this.vh / 2 });
  }

  zoomBy(f: number) {
    this.animateTo(zoomAt(this.view, f, this.vw / 2, this.vh / 2), 200);
  }

  zoom100() {
    this.animateTo(zoomAt(this.view, 1 / this.view.z, this.vw / 2, this.vh / 2), 250);
  }

  /** world point at the middle of the viewport */
  viewCenter() {
    return toWorld(this.view, this.vw / 2, this.vh / 2);
  }

  /** first layout: place unplaced cards, restore the saved view (or fit everything) */
  async start() {
    const doc = this.doc;
    this.viewKey = 'letopis.canvas.view.' + doc.id;
    const unplaced = doc.sections.filter((s) => !s.canvas);
    if (unplaced.length) {
      await whenResolved(unplaced.map((s) => s.media[0]).filter(Boolean) as MediaRef[]);
      autoPlace(doc); // not saved until the next edit; deterministic if never saved
    }
    await tick();
    let v: View | null = null;
    try {
      const raw = localStorage.getItem(this.viewKey);
      if (raw) v = JSON.parse(raw);
    } catch {}
    if (v && isFinite(v.z) && v.z > 0) this.setView({ ...v, z: clampZ(v.z) }, true);
    else {
      const r = contentRect(doc);
      this.setView(r ? fitView(r, this.vw, this.vh, 0.06, 1) : { z: 1, tx: this.vw / 2, ty: this.vh / 2 }, true);
    }
    this.ready = true;
  }

  stop() {
    clearTimeout(this.settleTimer);
    cancelAnimationFrame(this.anim);
    try {
      localStorage.setItem(this.viewKey, JSON.stringify(this.view));
    } catch {}
  }

  /** sections created elsewhere (drawer: "add section below", duplicate) get a place next
   *  to the one before them in the document */
  placeNewSections(un: Section[]) {
    const doc = this.doc;
    for (const s of un) {
      const i = doc.sections.indexOf(s);
      const prev = doc.sections
        .slice(0, i)
        .reverse()
        .find((p) => p.canvas && !p.canvas.hidden);
      const { w, h } = defaultCardSize(s);
      if (prev) s.canvas = { x: snap(prev.canvas!.x + prev.canvas!.w + 60), y: prev.canvas!.y, w, h };
      else {
        const c = this.viewCenter();
        s.canvas = { x: snap(c.x - w / 2), y: snap(c.y - h / 2), w, h };
      }
    }
  }

  /** a whole new document (history restore, "load their version"): forget undo + selection */
  reset() {
    this.hist.clear();
    this.selected = new Set();
    this.editing = null;
  }

  // ---------- screen helpers ----------
  screenPt(e: { clientX: number; clientY: number }) {
    const r = this.vp!.getBoundingClientRect();
    return { sx: e.clientX - r.left, sy: e.clientY - r.top };
  }
  worldPt(e: { clientX: number; clientY: number }) {
    const { sx, sy } = this.screenPt(e);
    return toWorld(this.view, sx, sy);
  }
  toScreen(r: Rect): Rect {
    const v = this.view;
    return { x: r.x * v.z + v.tx, y: r.y * v.z + v.ty, w: r.w * v.z, h: r.h * v.z };
  }
  toScreenPt(p: { x: number; y: number }) {
    return { x: p.x * this.view.z + this.view.tx, y: p.y * this.view.z + this.view.ty };
  }

  /** item under the pointer (via the DOM, so it matches what is drawn) */
  hitAt(clientX: number, clientY: number, skip?: string): { id: string; kind: string } | null {
    for (const el of document.elementsFromPoint(clientX, clientY)) {
      const h = (el as HTMLElement).closest?.('[data-cid]') as HTMLElement | null;
      if (h && h.dataset.cid !== skip && this.vp?.contains(h)) return { id: h.dataset.cid!, kind: h.dataset.kind! };
    }
    return null;
  }

  // ---------- groups, locks, reveal ----------
  flagsOf(id: string): Flags | undefined {
    const s = this.secMap.get(id);
    if (s) return s.canvas;
    return this.itemMap.get(id) as Flags | undefined;
  }
  isLocked(id: string) {
    return !!this.flagsOf(id)?.locked;
  }
  /** selecting one member of a group selects the whole group */
  withGroups(ids: Iterable<string>): Set<string> {
    const out = new Set(ids);
    const gs = new Set([...out].map((id) => this.flagsOf(id)?.group).filter(Boolean));
    if (gs.size)
      for (const id of this.allIds()) {
        const g = this.flagsOf(id)?.group;
        if (g && gs.has(g)) out.add(id);
      }
    return out;
  }
  groupSelection() {
    if (this.selected.size < 2) return;
    this.before();
    const g = newId();
    for (const id of this.selected) {
      const f = this.flagsOf(id);
      if (f) f.group = g;
    }
    this.change();
    toasts.show(`Grouped ${this.selected.size} items (Ctrl+Shift+G ungroups)`, 'ok');
  }
  ungroupSelection() {
    this.before();
    for (const id of this.selected) {
      const f = this.flagsOf(id);
      if (f) delete f.group;
    }
    this.change();
  }
  toggleLock() {
    if (!this.selected.size) return;
    this.before();
    const lock = ![...this.selected].every((id) => this.isLocked(id));
    for (const id of this.selected) {
      const f = this.flagsOf(id);
      if (!f) continue;
      if (lock) f.locked = true;
      else delete f.locked;
    }
    this.change();
  }
  setReveal(n: number) {
    this.before();
    for (const id of this.selected) {
      const f = this.flagsOf(id);
      if (!f) continue;
      if (n > 0) f.reveal = n;
      else delete f.reveal;
    }
    this.change();
  }

  /** boxes (cards and box items) of the selection that can move */
  selBoxes(): Rect[] {
    const out: Rect[] = [];
    for (const id of this.selected) {
      if (this.isLocked(id)) continue;
      const s = this.secMap.get(id);
      if (s?.canvas) out.push(s.canvas);
      const it = this.itemMap.get(id);
      if (it && isBox(it)) out.push(it);
    }
    return out;
  }
  align(how: Align) {
    const rs = this.selBoxes();
    if (rs.length < 2) return;
    this.before();
    alignRects(rs, how);
    this.change();
  }
  distribute(axis: 'x' | 'y') {
    const rs = this.selBoxes();
    if (rs.length < 3) return;
    this.before();
    distributeRects(rs, axis);
    this.change();
  }

  // ---------- auto-arrange ----------
  async arrange(mode: 'grid' | 'timeline' | 'folder') {
    this.arrangeMenu = false;
    const doc = this.doc;
    const chosen = this.selCards.length >= 2 ? this.selCards : doc.sections.filter((s) => s.canvas && !s.canvas.hidden && !s.canvas.locked);
    if (chosen.length < 2) return toasts.show('Nothing to arrange', 'info');
    this.before();
    const u = union(chosen.map((s) => s.canvas!))!;
    if (mode === 'grid') gridLayout(chosen.map((s) => s.canvas!), u.x, u.y);
    else {
      const res = await api.resolve(chosen.filter((s) => s.media[0]).map((s) => ({ libraryId: s.media[0].libraryId, path: s.media[0].path })));
      const by = new Map(res.map((r) => [r.libraryId + '\u0000' + r.path, r.file]));
      const fileOf = (s: Section) => (s.media[0] ? by.get(s.media[0].libraryId + '\u0000' + s.media[0].path) : undefined);
      if (mode === 'timeline') {
        const sorted = [...chosen].sort((a, b) => {
          const fa = fileOf(a),
            fb = fileOf(b);
          return (fa ? fa.taken || fa.mtime : Infinity) - (fb ? fb.taken || fb.mtime : Infinity);
        });
        gridLayout(sorted.map((s) => s.canvas!), u.x, u.y, Math.min(sorted.length, 6));
      } else {
        // one column per folder, with a frame titled after the folder
        const groups = new Map<string, Section[]>();
        for (const s of chosen) {
          const d = fileOf(s)?.dir ?? '(no media)';
          groups.set(d, [...(groups.get(d) ?? []), s]);
        }
        let x = u.x;
        for (const [dir, secs] of [...groups.entries()].sort()) {
          const w = Math.max(...secs.map((s) => s.canvas!.w));
          let y = u.y;
          for (const s of secs) {
            s.canvas!.x = snap(x);
            s.canvas!.y = snap(y);
            y += s.canvas!.h + 40;
          }
          const fr = newBox('frame', { x: snap(x - 30), y: snap(u.y - 40), w: snap(w + 60), h: snap(y - u.y + 40) }, { title: dir.split('/').pop() || dir });
          this.insertItem(fr);
          x += w + 140;
        }
      }
    }
    this.change();
    this.zoomToRect(union(chosen.map((s) => s.canvas!))!, 0.06, 1);
  }

  // ---------- clipboard ----------
  copySelection(cut = false) {
    if (!this.selected.size) return;
    const items = this.selItems.map((i) => $state.snapshot(i)) as CanvasItem[];
    const sections = this.selCards.map((s) => $state.snapshot(s)) as Section[];
    const text = CLIP_PREFIX + JSON.stringify({ v: 1, items, sections });
    const n = this.selected.size;
    navigator.clipboard?.writeText(text).then(
      () => toasts.show(`Copied ${n} item${n === 1 ? '' : 's'} — paste on any canvas or story`, 'ok'),
      () => toasts.error('The browser blocked clipboard access'),
    );
    // cards stay in the document (they are only taken off the canvas)
    if (cut) this.deleteSelection();
  }

  pasteClip(text: string): boolean {
    const p0 = parseClip(text);
    if (!p0) return false;
    const p = remapClip(p0);
    const c = this.viewCenter();
    // sections from the document view have no canvas position yet
    let x = c.x;
    for (const s of p.sections)
      if (!s.canvas) {
        const { w, h } = defaultCardSize(s);
        s.canvas = { x, y: c.y, w, h };
        x += w + 40;
      }
    offsetClip(p, c.x, c.y);
    this.before();
    for (const s of p.sections) this.doc.sections.push(s);
    for (const it of p.items) this.insertItem(it);
    this.select([...p.sections.map((s) => s.id), ...p.items.map((i) => i.id)]);
    resolver.prefetch(p.sections.flatMap((s) => s.media));
    this.change();
    return true;
  }

  onPaste(e: ClipboardEvent) {
    if (typing(e.target) || middleClickPaste()) return;
    const text = e.clipboardData?.getData('text/plain');
    if (text && this.pasteClip(text)) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  }

  // ---------- inline card editing ----------
  onCardText(id: string, field: 'title' | 'text', value: any) {
    const s = this.secMap.get(id);
    if (!s) return;
    if (field === 'title') s.title = value;
    else s.text = value;
    this.ds.touch();
  }

  // ---------- creating ----------
  create(kind: Tool, r: Rect | null, x: number, y: number) {
    const def: Record<string, [number, number]> = {
      note: [240, 170],
      text: [320, 60],
      rect: [240, 160],
      ellipse: [200, 200],
      frame: [1280, 720],
      card: [420, 220],
    };
    const [dw, dh] = def[kind] ?? [200, 150];
    const rect = r ?? { x: snap(x - dw / 2, this.gridOn), y: snap(y - dh / 2, this.gridOn), w: dw, h: dh };
    this.before();
    if (kind === 'card') {
      const s = newSection('text-only');
      s.canvas = { ...rect, w: Math.max(rect.w, 200), h: Math.max(rect.h, 120) };
      this.ds.addSection(null, s);
      this.select([s.id]);
      this.tool = 'select';
      this.drawer = s.id;
      return;
    }
    let it: BoxItem;
    if (kind === 'text') it = newBox('note', r ? rect : { ...rect, w: 60 }, { color: 'none', size: 32, autoW: r ? undefined : true } as Partial<NoteItem>);
    else if (kind === 'frame') {
      const n = this.items.filter((i) => i.type === 'frame').length + 1;
      it = newBox('frame', rect, { title: `Frame ${n}` });
    } else it = newBox(kind as BoxItem['type'], rect);
    this.insertItem(it);
    this.select([it.id]);
    this.tool = 'select';
    this.change();
    if (it.type === 'note') {
      this.fresh = it.id;
      this.startEdit(it.id);
    }
  }

  /** Frames go under everything else (so they never cover what they frame), after the
   *  existing frames (so the presentation order is the creation order). */
  insertItem(it: CanvasItem) {
    const c = ensureCanvas(this.doc);
    if (it.type !== 'frame') {
      c.items.push(it);
      return;
    }
    let at = 0;
    c.items.forEach((x, i) => {
      if (x.type === 'frame') at = i + 1;
    });
    c.items.splice(at, 0, it);
  }

  /** Library → canvas: a new card with the media, at a point (default: the view centre). */
  addMediaCard(refs: MediaRef[], at?: { x: number; y: number }) {
    if (!refs.length) return;
    prefs.addRecent(refs);
    const layout: Layout = refs.length > 1 ? 'gallery' : 'media-top';
    const s = newSection(layout, refs);
    if (layout === 'gallery') s.gallery = { mode: 'grid', columns: 0 };
    const p = at ?? this.viewCenter();
    const { w, h } = defaultCardSize(s);
    s.canvas = { x: snap(p.x - w / 2), y: snap(p.y - h / 2), w, h };
    this.before();
    this.ds.addSection(null, s);
    this.select([s.id]);
    // media sizes may arrive a moment later: re-fit once (unless the user touched it)
    const sid = s.id;
    whenResolved(refs).then(() => {
      const sec = this.secMap.get(sid);
      const c = sec?.canvas;
      if (!sec || !c) return;
      const { w, h } = defaultCardSize(sec);
      if (c.w === s.canvas!.w && c.h === s.canvas!.h && (w !== c.w || h !== c.h)) {
        c.x = snap(c.x + c.w / 2 - w / 2);
        c.w = w;
        c.h = h;
      }
    });
  }

  /** Picker → canvas: a sticker about 160 screen px big, at a point (default: the view
   *  centre, nudged a little per sticker so repeated clicks don't stack exactly). */
  addSticker(info: StickerInfo, at?: { x: number; y: number }) {
    let p = at;
    const z = this.view.z;
    if (!p) {
      const c = this.viewCenter();
      const n = this.items.filter((i) => i.type === 'sticker').length % 6;
      p = { x: c.x + (n * 24) / z, y: c.y + (n * 24) / z };
    }
    this.before();
    const it = newSticker(info, p.x, p.y, 160 / z);
    this.insertItem(it);
    this.select([it.id]);
    this.change();
  }

  // ---------- editing text ----------
  async startEdit(id: string) {
    const it = this.itemMap.get(id);
    if (!it || (it.type !== 'note' && it.type !== 'rect' && it.type !== 'ellipse')) return;
    if (this.fresh !== id) this.before();
    this.editing = id;
    await tick();
    this.editEl?.focus();
    this.editEl?.select();
  }

  commitEdit() {
    const id = this.editing;
    this.editing = null;
    if (!id) return;
    const it = this.itemMap.get(id);
    if (it?.type === 'note' && !it.text.trim() && this.fresh === id) {
      const c = ensureCanvas(this.doc);
      c.items = c.items.filter((x) => x.id !== id);
      this.select([]);
    }
    this.fresh = null;
    this.change();
    this.fitNotes([id]);
  }

  private measureCtx: CanvasRenderingContext2D | null = null;
  growEdit() {
    const el = this.editEl;
    const it = this.editItem;
    if (!el || !it || it.type !== 'note') return;
    if (it.autoW) {
      // free text widens with its longest line (until resized by hand)
      this.measureCtx ??= document.createElement('canvas').getContext('2d');
      const ctx = this.measureCtx;
      if (ctx) {
        // measure in the text's own style: font, weight, caps and letter-spacing
        const cs = getComputedStyle(el);
        ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${it.size}px ${cs.fontFamily}`;
        const spacing = (it.look?.letterSpacing ?? 0) * it.size;
        const width = (l: string) => {
          const t = (it.look?.uppercase ? l.toUpperCase() : l) || ' ';
          return ctx.measureText(t).width + spacing * [...t].length;
        };
        const longest = Math.max(...(it.text || ' ').split('\n').map(width));
        const pad = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) || it.size * 0.3;
        it.w = Math.min(2400, Math.max(40, Math.ceil(longest + pad + it.size * 0.3)));
      }
    }
    if (el.scrollHeight > el.clientHeight + 1) it.h = Math.ceil(el.scrollHeight);
  }

  /** Notes grow to fit their text; free text (no background) always hugs it. */
  async fitNotes(ids: Iterable<string>) {
    await tick();
    for (const id of ids) {
      const it = this.itemMap.get(id);
      if (it?.type !== 'note') continue;
      const el = this.vp?.querySelector(`.note[data-cid="${id}"]`) as HTMLElement | null;
      if (!el) continue;
      if (it.autoW && it.color === 'none') {
        // free text that was never resized by hand hugs its longest line (a new style
        // changes the font, so its width too)
        el.style.width = 'max-content';
        it.w = Math.min(2400, Math.max(40, Math.ceil(el.offsetWidth) + 2));
        el.style.width = `${it.w}px`;
      }
      const h = el.style.height;
      el.style.height = 'auto';
      const need = Math.ceil(el.scrollHeight);
      el.style.height = h;
      if (it.color === 'none' ? Math.abs(need - it.h) > 1 : need > it.h) it.h = need;
    }
  }

  onEditKey(e: KeyboardEvent) {
    if (e.key === 'Escape' || (e.key === 'Enter' && (e.ctrlKey || e.metaKey))) {
      e.preventDefault();
      e.stopPropagation();
      this.commitEdit();
    }
  }

  /** focus the text field of the context toolbar (frame title, arrow label) */
  focusCtxInput() {
    tick().then(() => (document.querySelector('.ctx input') as HTMLInputElement | null)?.focus());
  }

  openDrawer(id: string) {
    this.drawer = id;
    this.ds.activeSection = id;
  }

  // ---------- commands ----------
  deleteSelection() {
    if (!this.selected.size) return;
    const removed = new Set([...this.selected].filter((id) => !this.isLocked(id)));
    if (!removed.size) return toasts.show('Locked items cannot be deleted — unlock them first', 'info');
    this.before();
    detachArrows(this.doc, removed, this.tg);
    const c = ensureCanvas(this.doc);
    c.items = c.items.filter((i) => !removed.has(i.id));
    let hidden = 0;
    for (const id of removed) {
      const s = this.secMap.get(id);
      if (s?.canvas) {
        s.canvas.hidden = true;
        hidden++;
      }
    }
    if (this.drawer && removed.has(this.drawer)) this.drawer = null;
    this.select([]);
    this.change();
    if (hidden)
      toasts.show(hidden === 1 ? 'Card removed from the canvas (the section is still in the story)' : `${hidden} cards removed from the canvas (still in the story)`, 'info', 6000, {
        label: 'Undo',
        run: () => this.undo(),
      });
  }

  /**
   * Copies of the selection moved by (dx, dy); arrows are re-attached to the copies where
   * both ends were copied. Returns the new ids (selected) and the id map old → new.
   */
  duplicateSelection(dx = 32, dy = 32, pushUndo = true) {
    if (!this.selected.size) return null;
    if (pushUndo) this.before();
    const c = ensureCanvas(this.doc);
    const next: string[] = [];
    const idMap = new Map<string, string>();
    const selItems = this.selItems;
    for (const it of selItems) {
      if (it.type === 'arrow') continue;
      const copy = cloneItem(it, dx, dy);
      idMap.set(it.id, copy.id);
      this.insertItem(copy);
      next.push(copy.id);
    }
    for (const s of this.selCards) {
      const i = this.ds.sectionIndex(s.id);
      const copy = structuredClone($state.snapshot(s)) as Section;
      copy.id = newId();
      copy.canvas = { ...s.canvas!, x: s.canvas!.x + dx, y: s.canvas!.y + dy };
      this.doc.sections.splice(i + 1, 0, copy);
      idMap.set(s.id, copy.id);
      next.push(copy.id);
    }
    for (const it of selItems) {
      if (it.type !== 'arrow') continue;
      const copy = cloneItem(it, dx, dy);
      const { p1, p2 } = arrowPoints(it, this.tg);
      const end = (e: ArrowEnd, p: { x: number; y: number }): ArrowEnd =>
        e.id && idMap.has(e.id) ? { ...e, x: p.x + dx, y: p.y + dy, id: idMap.get(e.id) } : { x: p.x + dx, y: p.y + dy };
      copy.from = end(it.from, p1);
      copy.to = end(it.to, p2);
      c.items.push(copy);
      next.push(copy.id);
    }
    this.select(next);
    this.change();
    return { ids: next, idMap };
  }

  undo() {
    if (this.hist.undo(this.doc)) {
      this.selected = new Set([...this.selected].filter((id) => this.rectOf(id)));
      this.change();
    }
  }
  redo() {
    if (this.hist.redo(this.doc)) {
      this.selected = new Set([...this.selected].filter((id) => this.rectOf(id)));
      this.change();
    }
  }

  nudge(dx: number, dy: number) {
    if (!this.selected.size) return;
    this.before();
    for (const s of this.selCards) {
      s.canvas!.x += dx;
      s.canvas!.y += dy;
    }
    for (const it of this.selItems) {
      if (it.type === 'arrow') {
        if (!it.from.id) it.from = { x: it.from.x + dx, y: it.from.y + dy };
        if (!it.to.id) it.to = { x: it.to.x + dx, y: it.to.y + dy };
      } else {
        it.x += dx;
        it.y += dy;
      }
    }
    this.change();
  }

  /** Bring the selection in front of (or behind) everything else on the canvas, cards included. */
  reorder(front: boolean) {
    const c = ensureCanvas(this.doc);
    const selected = this.selected;
    const all = [
      ...c.items.filter((i): i is BoxItem => i.type !== 'arrow').map((i) => ({ id: i.id, x: i as { z?: number }, kind: i.type as string })),
      ...this.doc.sections.filter((s) => s.canvas && !s.canvas.hidden).map((s) => ({ id: s.id, x: s.canvas as { z?: number }, kind: 'card' })),
    ];
    const sel = all.filter((e) => selected.has(e.id));
    if (!sel.length) return;
    this.before();
    const others = all.filter((e) => !selected.has(e.id)).map((e) => zOf(e.x, e.kind));
    // keep the selection's own stacking among itself
    sel.sort((a, b) => zOf(a.x, a.kind) - zOf(b.x, b.kind));
    let z = front ? Math.max(5, ...others) + 1 : Math.min(0, ...others) - sel.length;
    for (const e of sel) e.x.z = z++;
    const selIt = c.items.filter((i) => selected.has(i.id));
    const rest = c.items.filter((i) => !selected.has(i.id));
    c.items = front ? [...rest, ...selIt] : [...selIt, ...rest];
    this.change();
  }

  /** set a property on every selected item that has it */
  setProp(k: string, v: unknown, kinds?: string[]) {
    this.before();
    for (const it of this.selItems) {
      if (kinds && !kinds.includes(it.type)) continue;
      if (v === undefined) delete (it as any)[k];
      else (it as any)[k] = v;
    }
    this.change();
    this.fitNotes(this.selItems.map((i) => i.id));
  }

  // text style of the selected notes; changes within a moment (sliders, colour pickers)
  // are one undo step
  private lookAt = 0;
  setLook(look: TextLook | undefined) {
    const now = performance.now();
    if (now - this.lookAt > 800) this.before();
    this.lookAt = now;
    const ids: string[] = [];
    for (const it of this.selItems) {
      if (it.type !== 'note') continue;
      if (look) it.look = $state.snapshot(look) as TextLook;
      else delete it.look;
      ids.push(it.id);
    }
    this.change();
    // refit once the face is there (fonts load on first use)
    const ready = look ? ensureFont(look.font, look.weight ?? 400) : Promise.resolve();
    ready.then(() => this.fitNotes(ids));
  }

  /** flip or turn the selected stickers */
  stickerOp(op: 'flipX' | 'flipY' | 'turn' | 'straighten') {
    this.before();
    for (const it of this.selItems) {
      if (it.type !== 'sticker' || it.locked) continue;
      if (op === 'flipX' || op === 'flipY') {
        if (it[op]) delete it[op];
        else it[op] = true;
      } else {
        const r = op === 'turn' ? snapAngle((it.rotation ?? 0) + 90) : 0;
        if (r) it.rotation = r;
        else delete it.rotation;
      }
    }
    this.change();
  }

  fitCardHeight(id: string) {
    const s = this.secMap.get(id);
    const el = this.vp?.querySelector(`.card[data-cid="${id}"]`) as HTMLElement | null;
    if (!s?.canvas) return;
    this.before();
    const text = el?.querySelector('.ctext') as HTMLElement | null;
    if (text && text.scrollHeight > text.clientHeight) s.canvas.h = Math.ceil(s.canvas.h + text.scrollHeight - text.clientHeight + 8);
    else s.canvas.h = defaultCardSize(s).h;
    this.change();
  }

  addFrameAroundView() {
    const live = visibleRect(this.view, this.vw, this.vh);
    const r = { x: snap(live.x + live.w * 0.08), y: snap(live.y + live.h * 0.08), w: snap(live.w * 0.84), h: snap(live.h * 0.84) };
    // keep 16:9 like a screen
    r.h = snap(Math.min(r.h, (r.w * 9) / 16));
    r.w = snap(Math.min(r.w, (r.h * 16) / 9));
    this.create('frame', r, 0, 0);
  }

  pendingFocus = $state<string | null>(null);
  /** select a card / item and bring it into view (command palette, links with ?s=) */
  focusCard(id: string) {
    if (!this.ready) {
      this.pendingFocus = id;
      return;
    }
    const r = this.rectOf(id);
    if (!r) return;
    this.select([id]);
    this.zoomToRect(r, 0.08, 1.25);
  }

  async doExport(what: 'all' | 'view' | 'sel') {
    this.exportMenu = false;
    const doc = this.doc;
    const r =
      what === 'all'
        ? contentRect(doc)
        : what === 'view'
          ? visibleRect(this.view, this.vw, this.vh)
          : union([...this.selected].map((id) => this.rectOf(id)).filter(Boolean) as Rect[]);
    if (!r) return toasts.show('Nothing to export', 'info');
    toasts.show('Rendering the PNG…', 'info', 2500);
    try {
      await exportCanvasPNG(doc, r, `${doc.title || 'canvas'}${what === 'sel' ? ' (selection)' : ''}`);
    } catch (e) {
      toasts.error('Export failed: ' + (e as Error).message);
    }
  }

  async present(fromSelection = false) {
    const doc = this.doc;
    const st = steps(doc);
    let start = 0;
    if (fromSelection && this.single) start = Math.max(0, st.findIndex((s) => s.id === this.single));
    await this.ds.save();
    try {
      await document.documentElement.requestFullscreen();
    } catch {
      /* not allowed: present in the window */
    }
    router.go(`/present/${doc.id}?canvas=1&s=${start}`);
  }

  zoomToSelection() {
    const r = union([...this.selected].map((id) => this.rectOf(id)).filter(Boolean) as Rect[]);
    if (r) this.zoomToRect(r);
  }
}
