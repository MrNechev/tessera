<script lang="ts">
  // Canvas (whiteboard) view of the open document: section cards plus notes, text,
  // shapes, arrows and frames on an infinite pan/zoom surface. Shares the DocState with
  // the document view (autosave, conflicts, history) and adds its own undo stack.
  import { onMount, tick, untrack } from 'svelte';
  import { api, newId } from '../lib/api';
  import {
    arrowPoints,
    arrowRect,
    autoPlace,
    CanvasHistory,
    clampZ,
    cloneItem,
    containsRect,
    contentRect,
    defaultCardSize,
    detachArrows,
    ensureCanvas,
    expand,
    fitView,
    ink,
    intersects,
    isBox,
    newArrow,
    newBox,
    normRect,
    NOTE_SIZES,
    PALETTE,
    paper,
    snap,
    steps,
    targets,
    toWorld,
    union,
    visibleRect,
    whenResolved,
    zoomAt,
    alignRects,
    distributeRects,
    gridLayout,
    CLIP_PREFIX,
    parseClip,
    remapClip,
    offsetClip,
    newPath,
    simplify,
    smoothPath,
    arrowPath as arrowGeom,
    type Align,
    type View,
  } from '../lib/canvas';
  import type { DocState } from '../lib/docstate.svelte';
  import { newSection } from '../lib/docstate.svelte';
  import { dnd, hasMedia, readDrop } from '../lib/dnd.svelte';
  import { prefs } from '../lib/prefs.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { ArrowEnd, ArrowItem, BoxItem, CanvasItem, Layout, MediaRef, NoteItem, Rect, Section, ShapeItem } from '../lib/types';
  import CanvasPanel from './CanvasPanel.svelte';
  import CanvasWorld from './CanvasWorld.svelte';
  import CanvasMinimap from './CanvasMinimap.svelte';
  import { exportCanvasPNG } from '../lib/canvasExport';
  import Icon from './Icon.svelte';
  import SectionView from './SectionView.svelte';

  let {
    ds,
    showPanel,
    onrelink,
    onpresentdoc,
  }: {
    ds: DocState;
    showPanel: boolean;
    onrelink: (sectionId: string, mediaIndex: number) => void;
    onpresentdoc: (sectionIndex: number) => void;
  } = $props();

  type Tool = 'select' | 'hand' | 'card' | 'note' | 'text' | 'rect' | 'ellipse' | 'arrow' | 'frame' | 'pen' | 'marker';
  const TOOLS: { id: Tool; icon: string; label: string; key: string }[] = [
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
  const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as const;

  function pref(k: string, d: string) {
    try {
      return localStorage.getItem('letopis.cv.' + k) ?? d;
    } catch {
      return d;
    }
  }
  function setPref(k: string, v: string) {
    try {
      localStorage.setItem('letopis.cv.' + k, v);
    } catch {}
  }

  const doc = $derived(ds.doc!);
  const items = $derived(doc.canvas?.items ?? []);
  const itemMap = $derived(new Map(items.map((i) => [i.id, i])));
  const secMap = $derived(new Map(doc.sections.map((s) => [s.id, s])));
  const tg = $derived(targets(doc));

  let vp = $state<HTMLDivElement>();
  let vw = $state(0);
  let vh = $state(0);
  let view = $state<View>({ tx: 0, ty: 0, z: 1 });
  let lod = $state(1);
  let renderRect = $state<Rect>({ x: 0, y: 0, w: 0, h: 0 });
  let mountRect = $state<Rect | null>(null);
  let playRect = $state<Rect | null>(null);
  let moving = $state(false);
  let ready = $state(false);

  let tool = $state<Tool>('select');
  let selected = $state(new Set<string>());
  let editing = $state<string | null>(null);
  let fresh: string | null = null; // just created (an empty note is dropped again on blur)
  let drawer = $state<string | null>(null);
  let help = $state(false);
  let spaceDown = $state(false);
  let gridOn = $state(pref('grid', '1') === '1');
  let wheelZoom = $state(pref('wheel', 'zoom') === 'zoom');
  let dropHint = $state(false);
  let preview = $state<
    | { kind: 'rect'; rect: Rect; shape: string }
    | { kind: 'arrow'; p1: { x: number; y: number }; p2: { x: number; y: number }; target: string | null }
    | null
  >(null);
  let editEl = $state<HTMLTextAreaElement>();
  let editingCard = $state<string | null>(null);
  let guides = $state<{ x: number[]; y: number[] }>({ x: [], y: [] });
  let showMinimap = $state(pref('minimap', '1') === '1');
  let inkColor = $state(pref('ink', 'red'));
  let inkPath = $state<[number, number][] | null>(null);
  let arrangeMenu = $state(false);
  let exportMenu = $state(false);
  let ctxW = $state(360);

  const hist = new CanvasHistory();

  // ---------- view / level of detail ----------

  let settleTimer: ReturnType<typeof setTimeout> | undefined;
  let viewKey = '';

  function settle() {
    moving = false;
    if (!vw || !vh) return;
    const live = visibleRect(view, vw, vh);
    lod = view.z;
    renderRect = expand(live, 1);
    mountRect = expand(live, 0.3);
    playRect = live;
    try {
      localStorage.setItem(viewKey, JSON.stringify(view));
    } catch {}
  }

  function setView(v: View, settleNow = false) {
    view = v;
    if (!vw) return;
    const live = visibleRect(v, vw, vh);
    // keep a margin of rendered (cheap) items around the viewport while moving
    if (!containsRect(renderRect, live) || v.z < lod / 1.8) renderRect = expand(live, 1);
    clearTimeout(settleTimer);
    if (settleNow) settle();
    else {
      moving = true;
      settleTimer = setTimeout(settle, 140);
    }
  }

  let anim = 0;
  function animateTo(target: View, ms = 420) {
    cancelAnimationFrame(anim);
    const from = { ...view };
    const t0 = performance.now();
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
      setView({ z, tx: vw / 2 - cx * z, ty: vh / 2 - cy * z });
      if (k < 1) anim = requestAnimationFrame(step);
    };
    anim = requestAnimationFrame(step);
  }

  function zoomToRect(r: Rect, pad = 0.08, maxZ = 1.25) {
    if (!vw) return;
    animateTo(fitView(r, vw, vh, pad, maxZ));
  }

  function fitAll() {
    const r = contentRect(doc);
    if (r) zoomToRect(r, 0.06, 1);
    else animateTo({ z: 1, tx: vw / 2, ty: vh / 2 });
  }

  function zoomBy(f: number) {
    animateTo(zoomAt(view, f, vw / 2, vh / 2), 200);
  }

  $effect(() => {
    // viewport resized
    void vw;
    void vh;
    if (ready) untrack(() => settle());
  });

  // ---------- setup ----------

  onMount(() => {
    viewKey = 'letopis.canvas.view.' + doc.id;
    (async () => {
      const unplaced = doc.sections.filter((s) => !s.canvas);
      if (unplaced.length) {
        await whenResolved(unplaced.map((s) => s.media[0]).filter(Boolean) as MediaRef[]);
        autoPlace(doc); // not saved until the next edit; deterministic if never saved
      }
      await tick();
      let v: View | null = null;
      try {
        const raw = localStorage.getItem(viewKey);
        if (raw) v = JSON.parse(raw);
      } catch {}
      if (v && isFinite(v.z) && v.z > 0) setView({ ...v, z: clampZ(v.z) }, true);
      else {
        const r = contentRect(doc);
        setView(r ? fitView(r, vw, vh, 0.06, 1) : { z: 1, tx: vw / 2, ty: vh / 2 }, true);
      }
      ready = true;
    })();

    const wheel = (e: WheelEvent) => onWheel(e);
    vp!.addEventListener('wheel', wheel, { passive: false });
    return () => {
      vp?.removeEventListener('wheel', wheel);
      clearTimeout(settleTimer);
      cancelAnimationFrame(anim);
      try {
        localStorage.setItem(viewKey, JSON.stringify(view));
      } catch {}
    };
  });

  // sections created elsewhere (drawer: "add section below", duplicate) get a place next
  // to the one before them in the document
  $effect(() => {
    if (!ready) return;
    const un = doc.sections.filter((s) => !s.canvas);
    if (!un.length) return;
    untrack(() => {
      for (const s of un) {
        const i = doc.sections.indexOf(s);
        const prev = doc.sections
          .slice(0, i)
          .reverse()
          .find((p) => p.canvas && !p.canvas.hidden);
        const { w, h } = defaultCardSize(s);
        if (prev) s.canvas = { x: snap(prev.canvas!.x + prev.canvas!.w + 60), y: prev.canvas!.y, w, h };
        else {
          const c = toWorld(view, vw / 2, vh / 2);
          s.canvas = { x: snap(c.x - w / 2), y: snap(c.y - h / 2), w, h };
        }
      }
    });
  });

  // a whole new document (history restore, "load their version"): forget undo + selection
  $effect(() => {
    void ds.revision;
    untrack(() => {
      hist.clear();
      selected = new Set();
      editing = null;
    });
  });

  // ---------- lookup ----------

  function rectOf(id: string): Rect | null {
    const s = secMap.get(id);
    if (s) return s.canvas && !s.canvas.hidden ? s.canvas : null;
    const it = itemMap.get(id);
    if (!it) return null;
    return isBox(it) ? it : arrowRect(it, tg);
  }

  function allIds(): string[] {
    const ids = doc.sections.filter((s) => s.canvas && !s.canvas.hidden).map((s) => s.id);
    for (const it of items) ids.push(it.id);
    return ids;
  }

  const selItems = $derived([...selected].map((id) => itemMap.get(id)).filter(Boolean) as CanvasItem[]);
  const selCards = $derived([...selected].map((id) => secMap.get(id)).filter(Boolean) as Section[]);
  const single = $derived(selected.size === 1 ? [...selected][0] : null);
  const singleArrow = $derived(single && itemMap.get(single)?.type === 'arrow' ? (itemMap.get(single) as ArrowItem) : null);

  function select(ids: Iterable<string>) {
    selected = new Set(ids);
    const cards = [...selected].filter((id) => secMap.has(id));
    ds.activeSection = cards.length === 1 ? cards[0] : null;
  }

  // ---------- groups, locks, reveal ----------
  type Flags = { group?: string; locked?: boolean; reveal?: number };
  function flagsOf(id: string): Flags | undefined {
    const s = secMap.get(id);
    if (s) return s.canvas;
    return itemMap.get(id) as Flags | undefined;
  }
  const isLocked = (id: string) => !!flagsOf(id)?.locked;
  /** selecting one member of a group selects the whole group */
  function withGroups(ids: Iterable<string>): Set<string> {
    const out = new Set(ids);
    const gs = new Set([...out].map((id) => flagsOf(id)?.group).filter(Boolean));
    if (gs.size)
      for (const id of allIds()) {
        const g = flagsOf(id)?.group;
        if (g && gs.has(g)) out.add(id);
      }
    return out;
  }
  function groupSelection() {
    if (selected.size < 2) return;
    before();
    const g = newId();
    for (const id of selected) {
      const f = flagsOf(id);
      if (f) f.group = g;
    }
    change();
    toasts.show(`Grouped ${selected.size} items (Ctrl+Shift+G ungroups)`, 'ok');
  }
  function ungroupSelection() {
    before();
    for (const id of selected) {
      const f = flagsOf(id);
      if (f) delete f.group;
    }
    change();
  }
  function toggleLock() {
    if (!selected.size) return;
    before();
    const lock = ![...selected].every(isLocked);
    for (const id of selected) {
      const f = flagsOf(id);
      if (!f) continue;
      if (lock) f.locked = true;
      else delete f.locked;
    }
    change();
  }
  function setReveal(n: number) {
    before();
    for (const id of selected) {
      const f = flagsOf(id);
      if (!f) continue;
      if (n > 0) f.reveal = n;
      else delete f.reveal;
    }
    change();
  }
  const selReveal = $derived([...selected].map((id) => flagsOf(id)?.reveal ?? 0).reduce((a, b) => Math.max(a, b), 0));
  const selLocked = $derived(selected.size > 0 && [...selected].every((id) => !!flagsOf(id)?.locked));
  const selGrouped = $derived([...selected].some((id) => !!flagsOf(id)?.group));

  /** boxes (cards and box items) of the selection that can move */
  function selBoxes(): Rect[] {
    const out: Rect[] = [];
    for (const id of selected) {
      if (isLocked(id)) continue;
      const s = secMap.get(id);
      if (s?.canvas) out.push(s.canvas);
      const it = itemMap.get(id);
      if (it && isBox(it)) out.push(it);
    }
    return out;
  }
  function align(how: Align) {
    const rs = selBoxes();
    if (rs.length < 2) return;
    before();
    alignRects(rs, how);
    change();
  }
  function distribute(axis: 'x' | 'y') {
    const rs = selBoxes();
    if (rs.length < 3) return;
    before();
    distributeRects(rs, axis);
    change();
  }

  // ---------- auto-arrange ----------
  async function arrange(mode: 'grid' | 'timeline' | 'folder') {
    arrangeMenu = false;
    const chosen = selCards.length >= 2 ? selCards : doc.sections.filter((s) => s.canvas && !s.canvas.hidden && !s.canvas.locked);
    if (chosen.length < 2) return toasts.show('Nothing to arrange', 'info');
    before();
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
          insertItem(fr);
          x += w + 140;
        }
      }
    }
    change();
    zoomToRect(union(chosen.map((s) => s.canvas!))!, 0.06, 1);
  }

  // ---------- clipboard ----------
  function copySelection(cut = false) {
    if (!selected.size) return;
    const items = selItems.map((i) => $state.snapshot(i)) as CanvasItem[];
    const sections = selCards.map((s) => $state.snapshot(s)) as Section[];
    const text = CLIP_PREFIX + JSON.stringify({ v: 1, items, sections });
    navigator.clipboard?.writeText(text).then(
      () => toasts.show(`Copied ${selected.size} item${selected.size === 1 ? '' : 's'} — paste on any canvas or document`, 'ok'),
      () => toasts.error('The browser blocked clipboard access'),
    );
    if (cut) {
      // cards stay in the document (they are only taken off the canvas)
      deleteSelection();
    }
  }

  function pasteClip(text: string): boolean {
    const p0 = parseClip(text);
    if (!p0) return false;
    const p = remapClip(p0);
    const c = toWorld(view, vw / 2, vh / 2);
    // sections from the document view have no canvas position yet
    let x = c.x;
    for (const s of p.sections)
      if (!s.canvas) {
        const { w, h } = defaultCardSize(s);
        s.canvas = { x, y: c.y, w, h };
        x += w + 40;
      }
    offsetClip(p, c.x, c.y);
    before();
    for (const s of p.sections) doc.sections.push(s);
    for (const it of p.items) insertItem(it);
    select([...p.sections.map((s) => s.id), ...p.items.map((i) => i.id)]);
    resolver.prefetch(p.sections.flatMap((s) => s.media));
    change();
    return true;
  }

  function onPaste(e: ClipboardEvent) {
    if (typing(e.target)) return;
    const text = e.clipboardData?.getData('text/plain');
    if (text && pasteClip(text)) {
      e.preventDefault();
      e.stopImmediatePropagation();
    }
  }

  // ---------- inline card editing ----------
  function onCardText(id: string, field: 'title' | 'text', value: any) {
    const s = secMap.get(id);
    if (!s) return;
    if (field === 'title') s.title = value;
    else s.text = value;
    ds.touch();
  }

  function change() {
    ds.touch();
  }
  function before() {
    hist.push(doc);
  }

  // ---------- screen helpers ----------

  function screenPt(e: { clientX: number; clientY: number }) {
    const r = vp!.getBoundingClientRect();
    return { sx: e.clientX - r.left, sy: e.clientY - r.top };
  }
  function worldPt(e: { clientX: number; clientY: number }) {
    const { sx, sy } = screenPt(e);
    return toWorld(view, sx, sy);
  }
  function toScreen(r: Rect): Rect {
    return { x: r.x * view.z + view.tx, y: r.y * view.z + view.ty, w: r.w * view.z, h: r.h * view.z };
  }

  /** item under the pointer (via the DOM, so it matches what is drawn) */
  function hitAt(clientX: number, clientY: number, skip?: string): { id: string; kind: string } | null {
    for (const el of document.elementsFromPoint(clientX, clientY)) {
      const h = (el as HTMLElement).closest?.('[data-cid]') as HTMLElement | null;
      if (h && h.dataset.cid !== skip && vp?.contains(h)) return { id: h.dataset.cid!, kind: h.dataset.kind! };
    }
    return null;
  }

  // ---------- pointer interaction ----------

  type Drag =
    | { kind: 'pan'; sx: number; sy: number; tx: number; ty: number }
    | {
        kind: 'move';
        sx: number;
        sy: number;
        boxes: { o: Rect; x: number; y: number }[];
        arrows: { a: ArrowItem; from: ArrowEnd; to: ArrowEnd }[];
        moved: boolean;
        clicked: string;
        bbox: Rect | null;
        cands: Rect[];
      }
    | { kind: 'resize'; id: string; handle: string; start: Rect; sx: number; sy: number; moved: boolean; minW: number; minH: number }
    | { kind: 'marquee'; x0: number; y0: number; base: Set<string> }
    | { kind: 'create'; x0: number; y0: number; sx: number; sy: number }
    | { kind: 'arrow'; from: ArrowEnd; sx: number; sy: number }
    | { kind: 'end'; a: ArrowItem; which: 'from' | 'to'; moved: boolean }
    | { kind: 'curve'; a: ArrowItem; moved: boolean }
    | { kind: 'ink' }
    | { kind: 'pinch' };
  let drag: Drag | null = null;
  let panning = $state(false);
  let dragging = $state(false); // something is being moved/resized (hides the context toolbar)
  const touches = new Map<number, { x: number; y: number }>();
  let pinch: { d: number; mx: number; my: number } | null = null;

  function onPointerDown(e: PointerEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('.editbox')) return;
    if (editingCard) {
      if (t.closest('.card.editing .edit, .card.editing .toolbar')) return;
      editingCard = null;
    }
    if (editing) commitEdit();
    if (e.pointerType === 'touch') {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size === 2) {
        drag = { kind: 'pinch' };
        preview = null;
        const [a, b] = [...touches.values()];
        pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2 };
        return;
      }
    }
    const { sx, sy } = screenPt(e);
    const w = toWorld(view, sx, sy);
    vp!.setPointerCapture(e.pointerId);

    if (e.button === 1 || (e.button === 0 && (spaceDown || tool === 'hand'))) {
      e.preventDefault();
      drag = { kind: 'pan', sx, sy, tx: view.tx, ty: view.ty };
      panning = true;
      return;
    }
    if (e.button !== 0) return;

    const handle = t.closest('[data-handle]') as HTMLElement | null;
    if (handle) {
      const h = handle.dataset.handle!;
      if (h === 'curve' && singleArrow) {
        drag = { kind: 'curve', a: singleArrow, moved: false };
        return;
      }
      if ((h === 'from' || h === 'to') && singleArrow) {
        drag = { kind: 'end', a: singleArrow, which: h, moved: false };
        return;
      }
      if (single) {
        const r = rectOf(single)!;
        const card = secMap.has(single);
        drag = { kind: 'resize', id: single, handle: h, start: { ...r }, sx, sy, moved: false, minW: card ? 160 : 24, minH: card ? 90 : 24 };
      }
      return;
    }

    if (tool === 'pen' || tool === 'marker') {
      drag = { kind: 'ink' };
      inkPath = [[w.x, w.y]];
      return;
    }
    if (tool === 'arrow') {
      const hit = hitAt(e.clientX, e.clientY);
      const from: ArrowEnd = hit && hit.kind !== 'arrow' ? { x: w.x, y: w.y, id: hit.id } : { x: w.x, y: w.y };
      drag = { kind: 'arrow', from, sx, sy };
      preview = { kind: 'arrow', p1: w, p2: w, target: null };
      return;
    }
    if (tool !== 'select') {
      drag = { kind: 'create', x0: w.x, y0: w.y, sx, sy };
      preview = { kind: 'rect', rect: { x: w.x, y: w.y, w: 0, h: 0 }, shape: tool };
      return;
    }

    const hitEl = t.closest('[data-cid]') as HTMLElement | null;
    if (hitEl) {
      const id = hitEl.dataset.cid!;
      // let native video controls work on a selected card
      if (t.closest('video') && selected.has(id)) {
        drag = null;
        vp!.releasePointerCapture(e.pointerId);
        return;
      }
      if (e.shiftKey) {
        const s = new Set(selected);
        const members = e.altKey ? new Set([id]) : withGroups([id]);
        if (s.has(id)) for (const m of members) s.delete(m);
        else for (const m of members) s.add(m);
        select(s);
      } else if (!selected.has(id)) select(e.altKey ? [id] : withGroups([id]));
      if (selected.has(id) && !isLocked(id)) startMove(sx, sy, id);
      return;
    }
    if (!e.shiftKey) select([]);
    drag = { kind: 'marquee', x0: w.x, y0: w.y, base: new Set(selected) };
  }

  function startMove(sx: number, sy: number, clicked: string) {
    const ids = new Set(selected);
    // frames carry what is inside them
    for (const id of selected) {
      const f = itemMap.get(id);
      if (f?.type !== 'frame') continue;
      for (const other of allIds()) {
        if (other === id || ids.has(other)) continue;
        const r = rectOf(other);
        if (r && containsRect(f, r)) ids.add(other);
      }
    }
    const boxes: { o: Rect; x: number; y: number }[] = [];
    const arrows: { a: ArrowItem; from: ArrowEnd; to: ArrowEnd }[] = [];
    for (const id of ids) {
      if (isLocked(id)) continue;
      const s = secMap.get(id);
      if (s?.canvas) boxes.push({ o: s.canvas, x: s.canvas.x, y: s.canvas.y });
      const it = itemMap.get(id);
      if (!it) continue;
      if (it.type === 'arrow') arrows.push({ a: it, from: { ...it.from }, to: { ...it.to } });
      else boxes.push({ o: it, x: it.x, y: it.y });
    }
    // alignment guides: edges and centres of nearby boxes that aren't moving
    const bbox = union(boxes.map((b) => ({ x: b.x, y: b.y, w: b.o.w, h: b.o.h })));
    const cands: Rect[] = [];
    if (bbox) {
      const near = expand(visibleRect(view, vw, vh), 0.2);
      for (const id of allIds()) {
        if (ids.has(id)) continue;
        const it = itemMap.get(id);
        if (it && (it.type === 'arrow' || it.type === 'path')) continue;
        const r = rectOf(id);
        if (r && intersects(r, near)) cands.push({ ...r });
        if (cands.length > 400) break;
      }
    }
    drag = { kind: 'move', sx, sy, boxes, arrows, moved: false, clicked, bbox, cands };
  }

  function onPointerMove(e: PointerEvent) {
    if (e.pointerType === 'touch' && touches.has(e.pointerId)) {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (drag?.kind === 'pinch' && touches.size === 2 && pinch) {
        const [a, b] = [...touches.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        const mx = (a.x + b.x) / 2,
          my = (a.y + b.y) / 2;
        const r = vp!.getBoundingClientRect();
        let v = { ...view, tx: view.tx + mx - pinch.mx, ty: view.ty + my - pinch.my };
        v = zoomAt(v, d / pinch.d, mx - r.left, my - r.top);
        setView(v);
        pinch = { d, mx, my };
        return;
      }
    }
    if (!drag) return;
    const { sx, sy } = screenPt(e);
    const w = toWorld(view, sx, sy);
    const snapOn = gridOn && !e.altKey;
    switch (drag.kind) {
      case 'pan':
        setView({ ...view, tx: drag.tx + sx - drag.sx, ty: drag.ty + sy - drag.sy });
        break;
      case 'move': {
        if (!drag.moved) {
          if (Math.hypot(sx - drag.sx, sy - drag.sy) < 3) return;
          drag.moved = true;
          dragging = true;
          before();
        }
        let dx = (sx - drag.sx) / view.z,
          dy = (sy - drag.sy) / view.z;
        const first = drag.boxes[0];
        // smart guides first (edges/centres line up with other boxes), then the grid
        const g = { x: [] as number[], y: [] as number[] };
        let gx = false,
          gy = false;
        if (drag.bbox && !e.altKey) {
          const thr = 6 / view.z;
          const b = drag.bbox;
          const xs = [b.x + dx, b.x + dx + b.w / 2, b.x + dx + b.w];
          const ys = [b.y + dy, b.y + dy + b.h / 2, b.y + dy + b.h];
          let bestX = thr,
            bestY = thr,
            adjX = 0,
            adjY = 0;
          for (const c of drag.cands) {
            for (const cx of [c.x, c.x + c.w / 2, c.x + c.w])
              for (const x of xs)
                if (Math.abs(cx - x) < bestX) {
                  bestX = Math.abs(cx - x);
                  adjX = cx - x;
                  g.x = [cx];
                }
            for (const cy of [c.y, c.y + c.h / 2, c.y + c.h])
              for (const y of ys)
                if (Math.abs(cy - y) < bestY) {
                  bestY = Math.abs(cy - y);
                  adjY = cy - y;
                  g.y = [cy];
                }
          }
          if (g.x.length) {
            dx += adjX;
            gx = true;
          }
          if (g.y.length) {
            dy += adjY;
            gy = true;
          }
        }
        guides = g;
        if (snapOn && first) {
          if (!gx) dx = snap(first.x + dx) - first.x;
          if (!gy) dy = snap(first.y + dy) - first.y;
        }
        for (const b of drag.boxes) {
          b.o.x = b.x + dx;
          b.o.y = b.y + dy;
        }
        for (const a of drag.arrows) {
          if (!a.a.from.id) a.a.from = { x: a.from.x + dx, y: a.from.y + dy };
          if (!a.a.to.id) a.a.to = { x: a.to.x + dx, y: a.to.y + dy };
        }
        break;
      }
      case 'resize': {
        if (!drag.moved) {
          drag.moved = true;
          dragging = true;
          before();
        }
        const o = drag.start;
        const dx = (sx - drag.sx) / view.z,
          dy = (sy - drag.sy) / view.z;
        let x0 = o.x,
          y0 = o.y,
          x1 = o.x + o.w,
          y1 = o.y + o.h;
        const h = drag.handle;
        if (h.includes('w')) x0 = snap(o.x + dx, snapOn);
        if (h.includes('e')) x1 = snap(o.x + o.w + dx, snapOn);
        if (h.includes('n')) y0 = snap(o.y + dy, snapOn);
        if (h.includes('s')) y1 = snap(o.y + o.h + dy, snapOn);
        if (x1 - x0 < drag.minW) h.includes('w') ? (x0 = x1 - drag.minW) : (x1 = x0 + drag.minW);
        if (y1 - y0 < drag.minH) h.includes('n') ? (y0 = y1 - drag.minH) : (y1 = y0 + drag.minH);
        if (e.shiftKey && h.length === 2) {
          // keep the aspect ratio
          const ratio = o.w / o.h;
          const nw = x1 - x0;
          const nh = nw / ratio;
          if (h.includes('n')) y0 = y1 - nh;
          else y1 = y0 + nh;
        }
        const r = rectOf(drag.id);
        if (r) {
          r.x = x0;
          r.y = y0;
          r.w = x1 - x0;
          r.h = y1 - y0;
          const it = itemMap.get(drag.id);
          if (it?.type === 'note' && it.autoW && (h.includes('e') || h.includes('w'))) delete it.autoW;
        }
        break;
      }
      case 'marquee': {
        const m = normRect(drag.x0, drag.y0, w.x, w.y);
        preview = { kind: 'rect', rect: m, shape: 'marquee' };
        const s = new Set(drag.base);
        for (const id of allIds()) {
          const r = rectOf(id);
          if (!r || isLocked(id)) continue;
          const it = itemMap.get(id);
          if (it?.type === 'frame' ? containsRect(m, r) : intersects(m, r)) s.add(id);
        }
        select(e.altKey ? s : withGroups(s));
        break;
      }
      case 'create': {
        let x1 = w.x,
          y1 = w.y;
        if (e.shiftKey) {
          const d = Math.max(Math.abs(x1 - drag.x0), Math.abs(y1 - drag.y0));
          x1 = drag.x0 + Math.sign(x1 - drag.x0 || 1) * d;
          y1 = drag.y0 + Math.sign(y1 - drag.y0 || 1) * d;
        }
        const r = normRect(snap(drag.x0, snapOn), snap(drag.y0, snapOn), snap(x1, snapOn), snap(y1, snapOn));
        preview = { kind: 'rect', rect: r, shape: tool };
        break;
      }
      case 'arrow': {
        const hit = hitAt(e.clientX, e.clientY, drag.from.id);
        const target = hit && hit.kind !== 'arrow' ? hit.id : null;
        const a: ArrowItem = { id: '', type: 'arrow', from: drag.from, to: target ? { ...w, id: target } : w, color: 'ink' };
        const { p1, p2 } = arrowPoints(a, tg);
        preview = { kind: 'arrow', p1, p2, target };
        break;
      }
      case 'ink':
        if (inkPath) {
          const l = inkPath[inkPath.length - 1];
          if (Math.hypot(w.x - l[0], w.y - l[1]) * view.z > 1.5) inkPath = [...inkPath, [w.x, w.y]];
        }
        break;
      case 'curve': {
        if (!drag.moved) {
          drag.moved = true;
          dragging = true;
          before();
        }
        const g0 = arrowGeom({ ...drag.a, curve: 0 }, tg);
        // signed distance of the pointer from the straight line → bend (×2: the curve's
        // middle lies halfway between the line and the control point)
        const d = (w.x - g0.mid.x) * g0.normal.x + (w.y - g0.mid.y) * g0.normal.y;
        drag.a.curve = Math.abs(d) * view.z < 6 ? undefined : Math.round(d * 2);
        break;
      }
      case 'end': {
        if (!drag.moved) {
          drag.moved = true;
          dragging = true;
          before();
        }
        const hit = hitAt(e.clientX, e.clientY, drag.a.id);
        const other = drag.which === 'from' ? drag.a.to : drag.a.from;
        const target = hit && hit.kind !== 'arrow' && hit.id !== other.id ? hit.id : undefined;
        drag.a[drag.which] = target ? { x: w.x, y: w.y, id: target } : { x: w.x, y: w.y };
        break;
      }
    }
  }

  function onPointerUp(e: PointerEvent) {
    touches.delete(e.pointerId);
    if (drag?.kind === 'pinch') {
      if (touches.size < 2) {
        drag = null;
        pinch = null;
      }
      return;
    }
    const d = drag;
    drag = null;
    panning = false;
    dragging = false;
    guides = { x: [], y: [] };
    if (!d) return;
    if (d.kind === 'ink') {
      const pts = inkPath;
      inkPath = null;
      if (pts && pts.length > 1) {
        const marker = tool === 'marker';
        const simple = simplify(pts, 0.8 / view.z);
        before();
        const it = newPath(simple, inkColor, (marker ? 18 : 4) / Math.max(0.25, Math.min(view.z, 2)), marker);
        insertItem(it);
        change();
      }
      return;
    }
    if (d.kind === 'curve') {
      if (d.moved) change();
      return;
    }
    const { sx, sy } = screenPt(e);
    const w = toWorld(view, sx, sy);
    switch (d.kind) {
      case 'move':
        if (d.moved) change();
        else if (!e.shiftKey && selected.size > 1) select([d.clicked]);
        break;
      case 'resize':
        if (d.moved) {
          change();
          fitNotes([d.id]);
        }
        break;
      case 'end':
        if (d.moved) change();
        break;
      case 'marquee':
        preview = null;
        break;
      case 'create': {
        const r = preview?.kind === 'rect' ? preview.rect : null;
        preview = null;
        const small = !r || r.w * view.z < 10 || r.h * view.z < 10;
        create(tool, small ? null : r, w.x, w.y);
        break;
      }
      case 'arrow': {
        const p = preview?.kind === 'arrow' ? preview : null;
        preview = null;
        if (!p) break;
        if (!p.target && Math.hypot(sx - d.sx, sy - d.sy) < 12) break;
        const to: ArrowEnd = p.target ? { x: w.x, y: w.y, id: p.target } : { x: w.x, y: w.y };
        before();
        const a = newArrow(d.from, to);
        ensureCanvas(doc).items.push(a);
        select([a.id]);
        tool = 'select';
        change();
        break;
      }
    }
  }

  // ---------- creating ----------

  function create(kind: Tool, r: Rect | null, x: number, y: number) {
    const def: Record<string, [number, number]> = {
      note: [240, 170],
      text: [320, 60],
      rect: [240, 160],
      ellipse: [200, 200],
      frame: [1280, 720],
      card: [420, 220],
    };
    const [dw, dh] = def[kind] ?? [200, 150];
    const rect = r ?? { x: snap(x - dw / 2, gridOn), y: snap(y - dh / 2, gridOn), w: dw, h: dh };
    before();
    if (kind === 'card') {
      const s = newSection('text-only');
      s.canvas = { ...rect, w: Math.max(rect.w, 200), h: Math.max(rect.h, 120) };
      ds.addSection(null, s);
      select([s.id]);
      tool = 'select';
      drawer = s.id;
      return;
    }
    let it: BoxItem;
    if (kind === 'text') it = newBox('note', r ? rect : { ...rect, w: 60 }, { color: 'none', size: 32, autoW: r ? undefined : true } as Partial<NoteItem>);
    else if (kind === 'frame') {
      const n = items.filter((i) => i.type === 'frame').length + 1;
      it = newBox('frame', rect, { title: `Frame ${n}` });
    } else it = newBox(kind as BoxItem['type'], rect);
    insertItem(it);
    select([it.id]);
    tool = 'select';
    change();
    if (it.type === 'note') {
      fresh = it.id;
      startEdit(it.id);
    }
  }

  /** Frames go under everything else (so they never cover what they frame), after the
   *  existing frames (so the presentation order is the creation order). */
  function insertItem(it: CanvasItem) {
    const c = ensureCanvas(doc);
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
  export function addMediaCard(refs: MediaRef[], at?: { x: number; y: number }) {
    if (!refs.length) return;
    prefs.addRecent(refs);
    const layout: Layout = refs.length > 1 ? 'gallery' : 'media-top';
    const s = newSection(layout, refs);
    if (layout === 'gallery') s.gallery = { mode: 'grid', columns: 0 };
    const p = at ?? toWorld(view, vw / 2, vh / 2);
    const place = () => {
      const { w, h } = defaultCardSize(s);
      return { x: snap(p.x - w / 2), y: snap(p.y - h / 2), w, h };
    };
    s.canvas = place();
    before();
    ds.addSection(null, s);
    select([s.id]);
    // media sizes may arrive a moment later: re-fit once (unless the user touched it)
    const sid = s.id;
    whenResolved(refs).then(() => {
      const sec = secMap.get(sid);
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

  function onDragOver(e: DragEvent) {
    if (!hasMedia(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'copy';
    dropHint = true;
  }

  function onDrop(e: DragEvent) {
    dropHint = false;
    const list = readDrop(e);
    dnd.dragging = false;
    if (!list) return;
    e.preventDefault();
    const refs = list.map((x) => ({ libraryId: x.libraryId, path: x.path }));
    const hit = hitAt(e.clientX, e.clientY);
    if (hit?.kind === 'card') {
      ds.addMedia(hit.id, refs);
      select([hit.id]);
      return;
    }
    addMediaCard(refs, worldPt(e));
  }

  // ---------- editing text ----------

  const editItem = $derived(editing ? (itemMap.get(editing) as NoteItem | ShapeItem | undefined) : undefined);

  async function startEdit(id: string) {
    const it = itemMap.get(id);
    if (!it || (it.type !== 'note' && it.type !== 'rect' && it.type !== 'ellipse')) return;
    if (fresh !== id) before();
    editing = id;
    await tick();
    editEl?.focus();
    editEl?.select();
  }

  function commitEdit() {
    const id = editing;
    editing = null;
    if (!id) return;
    const it = itemMap.get(id);
    if (it?.type === 'note' && !it.text.trim() && fresh === id) {
      const c = ensureCanvas(doc);
      c.items = c.items.filter((x) => x.id !== id);
      select([]);
    }
    fresh = null;
    change();
    fitNotes([id]);
  }

  let measureCtx: CanvasRenderingContext2D | null = null;
  function growEdit() {
    const el = editEl;
    const it = editItem;
    if (!el || !it || it.type !== 'note') return;
    if (it.autoW) {
      // free text widens with its longest line (until resized by hand)
      measureCtx ??= document.createElement('canvas').getContext('2d');
      if (measureCtx) {
        measureCtx.font = `${it.size}px ${getComputedStyle(el).fontFamily}`;
        const longest = Math.max(...(it.text || ' ').split('\n').map((l) => measureCtx!.measureText(l || ' ').width));
        it.w = Math.min(2400, Math.max(40, Math.ceil(longest + it.size * 0.6)));
      }
    }
    if (el.scrollHeight > el.clientHeight + 1) it.h = Math.ceil(el.scrollHeight);
  }

  /** Notes grow to fit their text; free text (no background) always hugs it. */
  async function fitNotes(ids: Iterable<string>) {
    await tick();
    for (const id of ids) {
      const it = itemMap.get(id);
      if (it?.type !== 'note') continue;
      const el = vp?.querySelector(`.note[data-cid="${id}"]`) as HTMLElement | null;
      if (!el) continue;
      const h = el.style.height;
      el.style.height = 'auto';
      const need = Math.ceil(el.scrollHeight);
      el.style.height = h;
      if (it.color === 'none' ? Math.abs(need - it.h) > 1 : need > it.h) it.h = need;
    }
  }

  function onEditKey(e: KeyboardEvent) {
    if (e.key === 'Escape' || (e.key === 'Enter' && (e.ctrlKey || e.metaKey))) {
      e.preventDefault();
      e.stopPropagation();
      commitEdit();
    }
  }

  // ---------- double click ----------

  function onDblClick(e: MouseEvent) {
    // pointer capture retargets dblclick to the viewport: find the item under the pointer
    const top = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    if (top?.closest('.editbox, [data-handle], .ctx, .tools, .zoombar, .drawer, .help')) return;
    const hit = hitAt(e.clientX, e.clientY);
    if (hit) {
      const id = hit.id;
      const kind = hit.kind;
      if (kind === 'card') {
        const sec = secMap.get(id);
        // big enough on screen: edit the text in place; otherwise the side editor
        if (sec?.canvas && sec.canvas.w * view.z >= 240 && !isLocked(id)) {
          select([id]);
          editingCard = id;
        } else openDrawer(id);
      }
      else if (kind === 'note' || kind === 'shape') startEdit(id);
      else if (kind === 'frame' || kind === 'arrow') {
        select([id]);
        tick().then(() => (document.querySelector('.ctx input') as HTMLInputElement | null)?.focus());
      }
      return;
    }
    if (tool === 'select') {
      const w = worldPt(e);
      create('note', null, w.x, w.y);
    }
  }

  function openDrawer(id: string) {
    drawer = id;
    ds.activeSection = id;
  }

  // ---------- wheel ----------

  function onWheel(e: WheelEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('.editbox')) return;
    // a selected card with a playing video keeps native scrolling off; the canvas owns the wheel
    e.preventDefault();
    const { sx, sy } = screenPt(e);
    const unit = e.deltaMode === 1 ? 33 : e.deltaMode === 2 ? vh : 1;
    const dx = e.deltaX * unit,
      dy = e.deltaY * unit;
    if (e.ctrlKey || e.metaKey || (wheelZoom && !e.shiftKey)) {
      cancelAnimationFrame(anim);
      const f = Math.exp(-dy * (e.ctrlKey && Math.abs(dy) < 50 ? 0.01 : 0.0018));
      setView(zoomAt(view, f, sx, sy));
    } else {
      cancelAnimationFrame(anim);
      setView({ ...view, tx: view.tx - (e.shiftKey && !dx ? dy : dx), ty: view.ty - (e.shiftKey && !dx ? 0 : dy) });
    }
  }

  // ---------- commands ----------

  function deleteSelection() {
    if (!selected.size) return;
    const removed = new Set([...selected].filter((id) => !isLocked(id)));
    if (!removed.size) return toasts.show('Locked items cannot be deleted — unlock them first', 'info');
    before();
    detachArrows(doc, removed, tg);
    const c = ensureCanvas(doc);
    c.items = c.items.filter((i) => !removed.has(i.id));
    let hidden = 0;
    for (const id of removed) {
      const s = secMap.get(id);
      if (s?.canvas) {
        s.canvas.hidden = true;
        hidden++;
      }
    }
    if (drawer && removed.has(drawer)) drawer = null;
    select([]);
    change();
    if (hidden)
      toasts.show(hidden === 1 ? 'Card removed from the canvas (the section is still in the document)' : `${hidden} cards removed from the canvas (still in the document)`, 'info', 6000, {
        label: 'Undo',
        run: undo,
      });
  }

  function duplicateSelection() {
    if (!selected.size) return;
    before();
    const c = ensureCanvas(doc);
    const next: string[] = [];
    const idMap = new Map<string, string>();
    for (const it of selItems) {
      if (it.type === 'arrow') continue;
      const copy = cloneItem(it, 32, 32);
      idMap.set(it.id, copy.id);
      insertItem(copy);
      next.push(copy.id);
    }
    for (const s of selCards) {
      const i = ds.sectionIndex(s.id);
      const copy = structuredClone($state.snapshot(s)) as Section;
      copy.id = newId();
      copy.canvas = { ...s.canvas!, x: s.canvas!.x + 32, y: s.canvas!.y + 32 };
      doc.sections.splice(i + 1, 0, copy);
      idMap.set(s.id, copy.id);
      next.push(copy.id);
    }
    // arrows: copy, re-attached to the copies where both ends were copied
    for (const it of selItems) {
      if (it.type !== 'arrow') continue;
      const copy = cloneItem(it, 32, 32);
      const { p1, p2 } = arrowPoints(it, tg);
      copy.from = it.from.id && idMap.has(it.from.id) ? { x: p1.x + 32, y: p1.y + 32, id: idMap.get(it.from.id) } : { x: p1.x + 32, y: p1.y + 32 };
      copy.to = it.to.id && idMap.has(it.to.id) ? { x: p2.x + 32, y: p2.y + 32, id: idMap.get(it.to.id) } : { x: p2.x + 32, y: p2.y + 32 };
      c.items.push(copy);
      next.push(copy.id);
    }
    select(next);
    change();
  }

  function undo() {
    if (hist.undo(doc)) {
      selected = new Set([...selected].filter((id) => rectOf(id)));
      change();
    }
  }
  function redo() {
    if (hist.redo(doc)) {
      selected = new Set([...selected].filter((id) => rectOf(id)));
      change();
    }
  }

  function nudge(dx: number, dy: number) {
    if (!selected.size) return;
    before();
    for (const s of selCards) {
      s.canvas!.x += dx;
      s.canvas!.y += dy;
    }
    for (const it of selItems) {
      if (it.type === 'arrow') {
        if (!it.from.id) it.from = { x: it.from.x + dx, y: it.from.y + dy };
        if (!it.to.id) it.to = { x: it.to.x + dx, y: it.to.y + dy };
      } else {
        it.x += dx;
        it.y += dy;
      }
    }
    change();
  }

  function reorder(front: boolean) {
    before();
    const c = ensureCanvas(doc);
    const sel = c.items.filter((i) => selected.has(i.id));
    const rest = c.items.filter((i) => !selected.has(i.id));
    c.items = front ? [...rest, ...sel] : [...sel, ...rest];
    change();
  }

  /** set a property on every selected item that has it */
  function setProp(k: string, v: unknown, kinds?: string[]) {
    before();
    for (const it of selItems) {
      if (kinds && !kinds.includes(it.type)) continue;
      if (v === undefined) delete (it as any)[k];
      else (it as any)[k] = v;
    }
    change();
    fitNotes(selItems.map((i) => i.id));
  }

  function fitCardHeight(id: string) {
    const s = secMap.get(id);
    const el = vp?.querySelector(`.card[data-cid="${id}"]`) as HTMLElement | null;
    if (!s?.canvas) return;
    before();
    const text = el?.querySelector('.ctext') as HTMLElement | null;
    if (text && text.scrollHeight > text.clientHeight) s.canvas.h = Math.ceil(s.canvas.h + text.scrollHeight - text.clientHeight + 8);
    else s.canvas.h = defaultCardSize(s).h;
    change();
  }

  function addFrameAroundView() {
    const live = visibleRect(view, vw, vh);
    const r = { x: snap(live.x + live.w * 0.08), y: snap(live.y + live.h * 0.08), w: snap(live.w * 0.84), h: snap(live.h * 0.84) };
    // keep 16:9 like a screen
    r.h = snap(Math.min(r.h, (r.w * 9) / 16));
    r.w = snap(Math.min(r.w, (r.h * 16) / 9));
    create('frame', r, 0, 0);
  }

  let pendingFocus: string | null = null;
  /** select a card / item and bring it into view (command palette, links with ?s=) */
  export function focusCard(id: string) {
    if (!ready) {
      pendingFocus = id;
      return;
    }
    const r = rectOf(id);
    if (!r) return;
    select([id]);
    zoomToRect(r, 0.08, 1.25);
  }
  $effect(() => {
    if (ready && pendingFocus) {
      const id = pendingFocus;
      pendingFocus = null;
      untrack(() => focusCard(id));
    }
  });

  async function doExport(what: 'all' | 'view' | 'sel') {
    exportMenu = false;
    const r = what === 'all' ? contentRect(doc) : what === 'view' ? visibleRect(view, vw, vh) : union([...selected].map(rectOf).filter(Boolean) as Rect[]);
    if (!r) return toasts.show('Nothing to export', 'info');
    toasts.show('Rendering the PNG…', 'info', 2500);
    try {
      await exportCanvasPNG(doc, r, `${doc.title || 'canvas'}${what === 'sel' ? ' (selection)' : ''}`);
    } catch (e) {
      toasts.error('Export failed: ' + (e as Error).message);
    }
  }

  export async function present(fromSelection = false) {
    const st = steps(doc);
    let start = 0;
    if (fromSelection && single) start = Math.max(0, st.findIndex((s) => s.id === single));
    await ds.save();
    try {
      await document.documentElement.requestFullscreen();
    } catch {
      /* not allowed: present in the window */
    }
    router.go(`/present/${doc.id}?canvas=1&s=${start}`);
  }

  // ---------- keyboard ----------

  function typing(t: EventTarget | null) {
    const el = t as HTMLElement | null;
    return !!el && (el.isContentEditable || el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || !!el.closest?.('.drawer'));
  }

  function onKey(e: KeyboardEvent) {
    if (typing(e.target)) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key;
    if (k === ' ') {
      e.preventDefault();
      spaceDown = true;
      return;
    }
    if (mod && (k === 'z' || k === 'Z')) {
      e.preventDefault();
      e.shiftKey ? redo() : undo();
      return;
    }
    if (mod && (k === 'y' || k === 'Y')) {
      e.preventDefault();
      redo();
      return;
    }
    if (mod && (k === 'd' || k === 'D') && !e.shiftKey) {
      e.preventDefault();
      duplicateSelection();
      return;
    }
    if (mod && (k === 'a' || k === 'A')) {
      e.preventDefault();
      select(allIds().filter((id) => !isLocked(id)));
      return;
    }
    if (mod && (k === 'c' || k === 'C') && selected.size) {
      e.preventDefault();
      copySelection();
      return;
    }
    if (mod && (k === 'x' || k === 'X') && selected.size) {
      e.preventDefault();
      copySelection(true);
      return;
    }
    if (mod && (k === 'g' || k === 'G')) {
      e.preventDefault();
      e.shiftKey ? ungroupSelection() : groupSelection();
      return;
    }
    if (mod && e.shiftKey && (k === 'l' || k === 'L')) {
      e.preventDefault();
      toggleLock();
      return;
    }
    if (mod && k === '0') {
      e.preventDefault();
      animateTo(zoomAt(view, 1 / view.z, vw / 2, vh / 2), 250);
      return;
    }
    if (mod || e.altKey) return;
    if (k === 'Delete' || k === 'Backspace') {
      e.preventDefault();
      deleteSelection();
      return;
    }
    if (k === 'Escape') {
      if (editingCard) editingCard = null;
      else if (help) help = false;
      else if (drawer) drawer = null;
      else if (tool !== 'select') tool = 'select';
      else select([]);
      return;
    }
    if (k === 'Enter' && single) {
      e.preventDefault();
      if (secMap.has(single)) openDrawer(single);
      else startEdit(single);
      return;
    }
    if (k.startsWith('Arrow') && selected.size) {
      e.preventDefault();
      const n = e.shiftKey ? 10 * GRID_STEP : GRID_STEP;
      nudge(k === 'ArrowLeft' ? -n : k === 'ArrowRight' ? n : 0, k === 'ArrowUp' ? -n : k === 'ArrowDown' ? n : 0);
      return;
    }
    if (e.shiftKey && (k === '!' || k === '1')) {
      fitAll();
      return;
    }
    if (e.shiftKey && (k === '@' || k === '2')) {
      const r = union([...selected].map(rectOf).filter(Boolean) as Rect[]);
      if (r) zoomToRect(r);
      return;
    }
    if (k === '+' || k === '=') return zoomBy(1.25);
    if (k === '-' || k === '_') return zoomBy(0.8);
    if (k === '?') {
      help = !help;
      return;
    }
    const t = TOOLS.find((x) => x.key === k.toUpperCase());
    if (t && !e.shiftKey) tool = t.id;
  }
  const GRID_STEP = 8;

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === ' ') spaceDown = false;
  }

  // ---------- derived UI ----------

  const selScreen = $derived(
    [...selected]
      .filter((id) => itemMap.get(id)?.type !== 'arrow')
      .map((id) => {
        const r = rectOf(id);
        return r ? { id, r: toScreen(r) } : null;
      })
      .filter(Boolean) as { id: string; r: Rect }[],
  );

  const selBox = $derived.by(() => {
    const rs = [...selected].map(rectOf).filter(Boolean) as Rect[];
    const u = union(rs);
    return u ? toScreen(u) : null;
  });

  const arrowEnds = $derived.by(() => {
    if (!singleArrow) return null;
    const { p1, p2 } = arrowPoints(singleArrow, tg);
    return { p1: { x: p1.x * view.z + view.tx, y: p1.y * view.z + view.ty }, p2: { x: p2.x * view.z + view.tx, y: p2.y * view.z + view.ty } };
  });

  const selBoxesCount = $derived([...selected].filter((id) => !isLocked(id) && (secMap.has(id) || (itemMap.get(id) && itemMap.get(id)!.type !== 'arrow'))).length);
  const selKinds = $derived(new Set([...selItems.map((i) => i.type), ...(selCards.length ? ['card'] : [])]));
  const only = (k: string) => selKinds.size === 1 && selKinds.has(k);
  const firstNote = $derived(selItems.find((i) => i.type === 'note') as NoteItem | undefined);
  const firstShape = $derived(selItems.find((i) => i.type === 'rect' || i.type === 'ellipse') as ShapeItem | undefined);
  const firstArrow = $derived(selItems.find((i) => i.type === 'arrow') as ArrowItem | undefined);

  const ctxPos = $derived.by(() => {
    if (!selBox || dragging || editing) return null;
    const half = Math.min(ctxW, vw - 16) / 2;
    const x = Math.min(Math.max(selBox.x + selBox.w / 2, half + 8), vw - half - 8);
    const above = selBox.y - 54;
    const y = above > 8 ? above : Math.min(selBox.y + selBox.h + 12, vh - 54);
    return { x, y };
  });

  const bgSize = $derived.by(() => {
    let s = 24 * view.z;
    while (s < 12) s *= 4;
    while (s > 96) s /= 4;
    return s;
  });

  const hiddenCount = $derived(doc.sections.filter((s) => s.canvas?.hidden).length);
  const drawerSection = $derived(drawer ? secMap.get(drawer) : undefined);
  const stepCount = $derived(steps(doc).length);
  const cursor = $derived(
    spaceDown || tool === 'hand' ? (panning ? 'grabbing' : 'grab') : tool === 'select' ? 'default' : 'crosshair',
  );
</script>

<svelte:window onkeydown={onKey} onkeyup={onKeyUp} onblur={() => (spaceDown = false)} onpaste={onPaste} />

<div class="cv" class:with-panel={showPanel}>
  <div class="stage">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="viewport"
      class:drop={dropHint && dnd.dragging}
      bind:this={vp}
      bind:clientWidth={vw}
      bind:clientHeight={vh}
      style="cursor:{cursor}"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={onDblClick}
      ondragover={onDragOver}
      ondragleave={() => (dropHint = false)}
      ondrop={onDrop}
      oncontextmenu={(e) => e.preventDefault()}
    >
      {#if gridOn}
        <!-- dot grid: a slightly oversized layer moved by transform (no repaint while panning) -->
        <div
          class="gridlayer"
          style="background-size:{bgSize}px {bgSize}px;transform:translate({((view.tx % bgSize) + bgSize) % bgSize}px,{((view.ty % bgSize) + bgSize) % bgSize}px)"
        ></div>
      {/if}
      {#if ready}
        <CanvasWorld {doc} {view} {renderRect} {mountRect} {playRect} {lod} {moving} {selected} {editing} editor {editingCard} oncardtext={onCardText} />
      {/if}

      {#if editItem}
        <div class="editlayer" style="transform:translate({view.tx}px,{view.ty}px) scale({view.z})">
          <textarea
            class="editbox {editItem.type}"
            class:free={editItem.type === 'note' && editItem.color === 'none'}
            bind:this={editEl}
            bind:value={editItem.text}
            oninput={growEdit}
            onkeydown={onEditKey}
            onblur={commitEdit}
            spellcheck="true"
            placeholder={editItem.type === 'note' ? (editItem.color === 'none' ? 'Text' : 'Note') : 'Label'}
            style="left:{editItem.x}px;top:{editItem.y}px;width:{editItem.w}px;height:{editItem.h}px;font-size:{editItem.size || 24}px;{editItem.type === 'note'
              ? `background:${paper(editItem.color)};text-align:${editItem.align || 'left'}`
              : ''}"
          ></textarea>
        </div>
      {/if}

      <div class="overlay">
        {#each selScreen as s (s.id)}
          <div class="selbox" style="left:{s.r.x}px;top:{s.r.y}px;width:{s.r.w}px;height:{s.r.h}px"></div>
        {/each}
        {#if single && selScreen.length === 1 && !editing}
          {@const r = selScreen[0].r}
          {#each HANDLES as h (h)}
            <div
              class="handle h-{h}"
              data-handle={h}
              style="left:{r.x + (h.includes('w') ? 0 : h.includes('e') ? r.w : r.w / 2)}px;top:{r.y + (h.includes('n') ? 0 : h.includes('s') ? r.h : r.h / 2)}px"
            ></div>
          {/each}
        {/if}
        {#each guides.x as gx (gx)}<div class="guide v" style="left:{gx * view.z + view.tx}px"></div>{/each}
        {#each guides.y as gy (gy)}<div class="guide h" style="top:{gy * view.z + view.ty}px"></div>{/each}
        {#if inkPath}
          <svg class="pvline"><path d={smoothPath(inkPath.map(([x, y]) => [x * view.z + view.tx, y * view.z + view.ty]))} class="inkpv" class:hl={tool === 'marker'} stroke={ink(inkColor)} /></svg>
        {/if}
        {#if singleArrow && arrowEnds}
          {@const g = arrowGeom(singleArrow, tg)}
          <div class="handle bend" data-handle="curve" title="Drag to bend" style="left:{g.label.x * view.z + view.tx}px;top:{g.label.y * view.z + view.ty}px"></div>
        {/if}
        {#if arrowEnds}
          <div class="handle end" data-handle="from" style="left:{arrowEnds.p1.x}px;top:{arrowEnds.p1.y}px"></div>
          <div class="handle end" data-handle="to" style="left:{arrowEnds.p2.x}px;top:{arrowEnds.p2.y}px"></div>
        {/if}
        {#if preview?.kind === 'rect'}
          {@const r = toScreen(preview.rect)}
          <div class="pv {preview.shape}" style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px"></div>
        {:else if preview?.kind === 'arrow'}
          <svg class="pvline">
            <line
              x1={preview.p1.x * view.z + view.tx}
              y1={preview.p1.y * view.z + view.ty}
              x2={preview.p2.x * view.z + view.tx}
              y2={preview.p2.y * view.z + view.ty}
            />
          </svg>
          {#if preview.target}
            {@const tr = rectOf(preview.target)}
            {#if tr}
              {@const r = toScreen(tr)}
              <div class="pv target" style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px"></div>
            {/if}
          {/if}
        {/if}
      </div>

      {#if ready && !doc.sections.some((s) => s.canvas && !s.canvas.hidden) && !items.length}
        <div class="empty">
          <h3>An empty canvas</h3>
          <p>Drag media from the library here, or pick a tool on the left. Double-click anywhere for a sticky note.</p>
          <p class="muted">Sections of the document show up here as cards. Frames become the steps of a canvas presentation.</p>
        </div>
      {/if}
    </div>

    <!-- tools -->
    <div class="tools">
      {#each TOOLS as t, i (t.id)}
        {#if i === 2 || i === 8}<span class="hsep"></span>{/if}
        <button class="icon-btn" class:on={tool === t.id} title="{t.label} ({t.key})" onclick={() => (tool = t.id)}><Icon name={t.icon} /></button>
      {/each}
      {#if tool === 'pen' || tool === 'marker'}
        <span class="hsep"></span>
        {#each ['red', 'yellow', 'green', 'blue', 'purple', 'ink'] as c (c)}
          <button class="sw tsw" class:on={inkColor === c} style="background:{ink(c)}" title="Colour" aria-label="Ink colour" onclick={() => { inkColor = c; setPref('ink', c); }}></button>
        {/each}
      {/if}
      <span class="hsep"></span>
      <button class="icon-btn" title="Undo (Ctrl+Z)" onclick={undo}><Icon name="undo" /></button>
      <button class="icon-btn" title="Redo (Ctrl+Shift+Z)" onclick={redo}><Icon name="redo" /></button>
      <span class="hsep"></span>
      <button class="icon-btn" class:on={help} title="Shortcuts (?)" onclick={() => (help = !help)}><Icon name="help" /></button>
    </div>

    <!-- zoom -->
    <div class="zoombar">
      <button class="icon-btn" title="Zoom out (−)" onclick={() => zoomBy(0.8)}><Icon name="zoom-out" /></button>
      <button class="zval" title="Zoom to 100% (Ctrl+0)" onclick={() => animateTo(zoomAt(view, 1 / view.z, vw / 2, vh / 2), 250)}>{Math.round(view.z * 100)}%</button>
      <button class="icon-btn" title="Zoom in (+)" onclick={() => zoomBy(1.25)}><Icon name="zoom-in" /></button>
      <button class="icon-btn" title="Show everything (Shift+1)" onclick={fitAll}><Icon name="maximize" /></button>
      <span class="vsep"></span>
      <button class="icon-btn" class:on={showMinimap} title="Minimap" onclick={() => { showMinimap = !showMinimap; setPref('minimap', showMinimap ? '1' : '0'); }}><Icon name="map" /></button>
      <div class="zmenu">
        <button class="icon-btn" title="Arrange cards…" onclick={() => (arrangeMenu = !arrangeMenu)}><Icon name="arrange" /></button>
        {#if arrangeMenu}
          <div class="zpop">
            <div class="zh">Arrange {selCards.length >= 2 ? `${selCards.length} selected cards` : 'all cards'}</div>
            <button onclick={() => arrange('grid')}><Icon name="grid" size={14} /> Grid (document order)</button>
            <button onclick={() => arrange('timeline')}><Icon name="calendar" size={14} /> Timeline (date taken)</button>
            <button onclick={() => arrange('folder')}><Icon name="folder" size={14} /> By folder (with frames)</button>
          </div>
        {/if}
      </div>
      <div class="zmenu">
        <button class="icon-btn" title="Export…" onclick={() => (exportMenu = !exportMenu)}><Icon name="download" /></button>
        {#if exportMenu}
          <div class="zpop">
            <div class="zh">Export</div>
            <button onclick={() => doExport('all')}><Icon name="image2" size={14} /> PNG of the whole canvas</button>
            <button onclick={() => doExport('view')}><Icon name="image2" size={14} /> PNG of the current view</button>
            <button disabled={!selected.size} onclick={() => doExport('sel')}><Icon name="image2" size={14} /> PNG of the selection</button>
            <button onclick={() => { exportMenu = false; window.open(`#/print/${doc.id}?mode=canvas`, '_blank'); }}><Icon name="file-pdf" size={14} /> PDF (one page per frame)</button>
          </div>
        {/if}
      </div>
      <button class="icon-btn" class:on={gridOn} title="Snap to grid (hold Alt to move freely)" onclick={() => { gridOn = !gridOn; setPref('grid', gridOn ? '1' : '0'); }}><Icon name="dots" /></button>
      <button
        class="icon-btn"
        class:on={wheelZoom}
        title={wheelZoom ? 'Mouse wheel zooms (Shift+wheel pans). Click to make the wheel pan.' : 'Mouse wheel pans (Ctrl+wheel zooms). Click to make the wheel zoom.'}
        onclick={() => { wheelZoom = !wheelZoom; setPref('wheel', wheelZoom ? 'zoom' : 'pan'); }}><Icon name="mouse" /></button
      >
      <span class="vsep"></span>
      <button class="btn sm ghost" title="Present the frames (F5) · Shift+F5 from the selected frame" onclick={() => present(true)} disabled={!stepCount}>
        <Icon name="present" size={15} /> {stepCount} step{stepCount === 1 ? '' : 's'}
      </button>
    </div>

    {#if showMinimap && ready && vw}
      <div class="minimap"><CanvasMinimap {doc} {view} {vw} {vh} onpan={(cx, cy) => setView({ ...view, tx: vw / 2 - cx * view.z, ty: vh / 2 - cy * view.z }, true)} /></div>
    {/if}

    <!-- context toolbar for the selection -->
    {#if ctxPos && selected.size}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="ctx" bind:offsetWidth={ctxW} style="left:{ctxPos.x}px;top:{ctxPos.y}px" onpointerdown={(e) => e.stopPropagation()}>
        {#if only('card') && selCards.length === 1}
          <button class="btn sm" onclick={() => openDrawer(selCards[0].id)}><Icon name="pencil" size={14} /> Edit</button>
          <button class="icon-btn" title="Fit height to the content" onclick={() => fitCardHeight(selCards[0].id)}><Icon name="fit-h" /></button>
          <button class="icon-btn" title="Present the document from this section" onclick={() => onpresentdoc(ds.sectionIndex(selCards[0].id))}><Icon name="present" /></button>
          <span class="vsep"></span>
        {/if}
        {#if only('frame') && selItems.length === 1 && selItems[0].type === 'frame'}
          {@const f = selItems[0]}
          <input class="input fname" placeholder="Frame title" value={f.title} onfocus={before} oninput={(e) => { f.title = e.currentTarget.value; change(); }} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
          <span class="muted step">step {items.filter((i) => i.type === 'frame').indexOf(f) + 1}</span>
          <button class="icon-btn" title="Zoom to this frame" onclick={() => zoomToRect(f, 0.04, 4)}><Icon name="maximize" /></button>
          <span class="vsep"></span>
        {/if}
        {#if firstNote && (only('note'))}
          {#each PALETTE as p (p.key)}
            <button class="sw" class:on={firstNote.color === p.key} title={p.label} style="background:{p.paper}" onclick={() => setProp('color', p.key, ['note'])} aria-label={p.label}></button>
          {/each}
          <button class="sw none" class:on={firstNote.color === 'none'} title="No background (text)" onclick={() => setProp('color', 'none', ['note'])} aria-label="No background"></button>
          <span class="vsep"></span>
          {#each NOTE_SIZES as s (s.v)}
            <button class="seg" class:on={firstNote.size === s.v} onclick={() => setProp('size', s.v, ['note'])}>{s.label}</button>
          {/each}
          <span class="vsep"></span>
          {#each ['left', 'center', 'right'] as a (a)}
            <button class="icon-btn" class:on={(firstNote.align || 'left') === a} title="Align {a}" onclick={() => setProp('align', a === 'left' ? undefined : a, ['note'])}><Icon name="align-{a}" /></button>
          {/each}
          <span class="vsep"></span>
        {/if}
        {#if firstShape && only(firstShape.type) || (firstShape && selKinds.size === 2 && selKinds.has('rect') && selKinds.has('ellipse'))}
          {@const sh = firstShape!}
          <button class="sw ink" class:on={sh.color === 'ink'} title="Default" style="background:var(--text)" onclick={() => setProp('color', 'ink')} aria-label="Default colour"></button>
          {#each PALETTE as p (p.key)}
            <button class="sw" class:on={sh.color === p.key} title={p.label} style="background:{p.ink}" onclick={() => setProp('color', p.key)} aria-label={p.label}></button>
          {/each}
          <span class="vsep"></span>
          <button class="icon-btn" class:on={sh.fill} title="Fill" onclick={() => setProp('fill', sh.fill ? undefined : true)}><Icon name="droplet" /></button>
          <button class="icon-btn" class:on={sh.type === 'rect'} title="Rectangle" onclick={() => setProp('type', 'rect', ['rect', 'ellipse'])}><Icon name="square" /></button>
          <button class="icon-btn" class:on={sh.type === 'ellipse'} title="Ellipse" onclick={() => setProp('type', 'ellipse', ['rect', 'ellipse'])}><Icon name="circle" /></button>
          <button class="icon-btn" title="Label (double-click)" onclick={() => startEdit(sh.id)}><Icon name="type" /></button>
          <span class="vsep"></span>
        {/if}
        {#if firstArrow && only('arrow')}
          {@const a = firstArrow}
          <button class="sw ink" class:on={a.color === 'ink'} title="Default" style="background:var(--text)" onclick={() => setProp('color', 'ink')} aria-label="Default colour"></button>
          {#each PALETTE as p (p.key)}
            <button class="sw" class:on={a.color === p.key} title={p.label} style="background:{p.ink}" onclick={() => setProp('color', p.key)} aria-label={p.label}></button>
          {/each}
          <span class="vsep"></span>
          <button class="icon-btn" class:on={a.head === 'none'} title="Line" onclick={() => setProp('head', 'none', ['arrow'])}><Icon name="line" /></button>
          <button class="icon-btn" class:on={!a.head || a.head === 'end'} title="Arrow" onclick={() => setProp('head', 'end', ['arrow'])}><Icon name="arrow-ur" /></button>
          <button class="icon-btn" class:on={a.head === 'both'} title="Arrows at both ends" onclick={() => setProp('head', 'both', ['arrow'])}><Icon name="arrow-both" /></button>
          <button class="icon-btn" class:on={a.dashed} title="Dashed" onclick={() => setProp('dashed', a.dashed ? undefined : true, ['arrow'])}><Icon name="dash" /></button>
          <button class="icon-btn" class:on={!!a.curve} title={a.curve ? 'Straighten' : 'Curve (or drag the middle handle)'} onclick={() => setProp('curve', a.curve ? undefined : 120, ['arrow'])}><Icon name="curve" /></button>
          {#if selItems.length === 1}
            <input class="input fname" placeholder="Label" value={a.text ?? ''} onfocus={before} oninput={(e) => { a.text = e.currentTarget.value || undefined; change(); }} onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()} />
          {/if}
          <span class="vsep"></span>
        {/if}
        {#if selItems.some((i) => i.type === 'path') && only('path')}
          {#each ['red', 'yellow', 'green', 'blue', 'purple', 'ink'] as c (c)}
            <button class="sw" style="background:{ink(c)}" title="Colour" aria-label="Colour" onclick={() => setProp('color', c, ['path'])}></button>
          {/each}
          <span class="vsep"></span>
        {/if}
        {#if selBoxesCount >= 2}
          <button class="icon-btn" title="Align left" onclick={() => align('left')}><Icon name="align-l" /></button>
          <button class="icon-btn" title="Align centres" onclick={() => align('center')}><Icon name="align-c" /></button>
          <button class="icon-btn" title="Align right" onclick={() => align('right')}><Icon name="align-r" /></button>
          <button class="icon-btn" title="Align top" onclick={() => align('top')}><Icon name="align-t" /></button>
          <button class="icon-btn" title="Align middles" onclick={() => align('middle')}><Icon name="align-m" /></button>
          <button class="icon-btn" title="Align bottom" onclick={() => align('bottom')}><Icon name="align-b" /></button>
          {#if selBoxesCount >= 3}
            <button class="icon-btn" title="Distribute horizontally" onclick={() => distribute('x')}><Icon name="dist-h" /></button>
            <button class="icon-btn" title="Distribute vertically" onclick={() => distribute('y')}><Icon name="dist-v" /></button>
          {/if}
          <span class="vsep"></span>
        {/if}
        {#if selected.size >= 2 && !selGrouped}<button class="icon-btn" title="Group (Ctrl+G)" onclick={groupSelection}><Icon name="group" /></button>{/if}
        {#if selGrouped}<button class="icon-btn on" title="Ungroup (Ctrl+Shift+G)" onclick={ungroupSelection}><Icon name="group" /></button>{/if}
        <button class="icon-btn" class:on={selLocked} title={selLocked ? 'Unlock (Ctrl+Shift+L)' : 'Lock in place (Ctrl+Shift+L)'} onclick={toggleLock}><Icon name={selLocked ? 'lock' : 'unlock'} /></button>
        <span class="reveal" title="Canvas presentation: show on the n-th click inside its frame (– = always visible)">
          <Icon name="eye" size={14} />
          <button class="mini" onclick={() => setReveal(Math.max(0, selReveal - 1))} disabled={!selReveal}>−</button>
          <span>{selReveal || '–'}</span>
          <button class="mini" onclick={() => setReveal(selReveal + 1)}>+</button>
        </span>
        <span class="vsep"></span>
        {#if selItems.length}
          <button class="icon-btn" title="Bring to front" onclick={() => reorder(true)}><Icon name="arrow-up" /></button>
          <button class="icon-btn" title="Send to back" onclick={() => reorder(false)}><Icon name="arrow-down" /></button>
        {/if}
        <button class="icon-btn" title="Duplicate (Ctrl+D)" onclick={duplicateSelection}><Icon name="copy" /></button>
        <button class="icon-btn danger" title={selCards.length && !selItems.length ? 'Remove from the canvas (Delete) — stays in the document' : 'Delete (Del)'} onclick={deleteSelection}>
          <Icon name={selCards.length && !selItems.length ? 'eye-off' : 'trash'} />
        </button>
      </div>
    {/if}

    {#if help}
      <div class="help">
        <header><b>Canvas shortcuts</b><button class="icon-btn" onclick={() => (help = false)}><Icon name="x" /></button></header>
        <div class="hgrid">
          <span><span class="kbd">V</span> select</span><span><span class="kbd">H</span> / <span class="kbd">Space</span>+drag / middle mouse: pan</span>
          <span><span class="kbd">C</span> section card</span><span><span class="kbd">N</span> sticky note · <span class="kbd">T</span> text</span>
          <span><span class="kbd">R</span> rectangle · <span class="kbd">O</span> ellipse</span><span><span class="kbd">A</span> arrow (drag between items)</span>
          <span><span class="kbd">F</span> frame (presentation step)</span><span>Wheel: zoom · <span class="kbd">Shift</span>+wheel: pan</span>
          <span><span class="kbd">Shift 1</span> show all · <span class="kbd">Shift 2</span> zoom to selection</span><span><span class="kbd">Ctrl 0</span> 100% · <span class="kbd">+</span>/<span class="kbd">−</span> zoom</span>
          <span><span class="kbd">Ctrl Z</span> undo · <span class="kbd">Ctrl Shift Z</span> redo</span><span><span class="kbd">Ctrl D</span> duplicate · <span class="kbd">Del</span> delete</span>
          <span><span class="kbd">P</span> pen · <span class="kbd">M</span> highlighter</span><span>Double-click a card: edit its text in place</span>
          <span><span class="kbd">Ctrl C</span>/<span class="kbd">V</span>/<span class="kbd">X</span> copy, paste, cut (also between canvases and documents)</span><span><span class="kbd">Ctrl G</span> group · <span class="kbd">Ctrl Shift G</span> ungroup</span>
          <span><span class="kbd">Ctrl Shift L</span> lock / unlock</span><span>Drag the middle of an arrow to bend it</span>
          <span>Double-click a note or shape: edit text</span><span>Double-click empty space: sticky note</span>
          <span>Alt+click: select one item of a group</span><span>Pink lines: alignment guides (Alt = off)</span>
          <span><span class="kbd">Shift</span>+drag a corner: keep proportions</span><span><span class="kbd">Alt</span>+drag: no snapping</span>
          <span><span class="kbd">F5</span> present frames</span><span>Drag a frame: moves what is inside it</span>
        </div>
      </div>
    {/if}

    {#if drawerSection}
      <aside class="drawer" data-scroll-root>
        <header>
          <b>Edit section</b>
          <span class="grow"></span>
          <button class="icon-btn" title="Show on the canvas" onclick={() => drawerSection.canvas && zoomToRect(drawerSection.canvas)}><Icon name="maximize" /></button>
          <button class="icon-btn" title="Close (Esc)" onclick={() => (drawer = null)}><Icon name="x" /></button>
        </header>
        <div class="dbody">
          {#key drawerSection.id}
            <SectionView
              section={drawerSection}
              index={ds.sectionIndex(drawerSection.id)}
              total={doc.sections.length}
              {ds}
              {onrelink}
              onpresent={(i) => onpresentdoc(i)}
            />
          {/key}
        </div>
      </aside>
    {/if}
  </div>

  {#if showPanel}
    <CanvasPanel
      {ds}
      {selected}
      {hiddenCount}
      onbefore={before}
      onzoom={(r) => zoomToRect(r, 0.05, 2)}
      onselect={(id) => select([id])}
      onpresent={(i) => router.go(`/present/${doc.id}?canvas=1&s=${i}`)}
      onaddframe={addFrameAroundView}
      oneditcard={openDrawer}
    />
  {/if}
</div>

<style>
  .cv {
    position: relative;
    height: 100%;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  .cv.with-panel {
    grid-template-columns: minmax(0, 1fr) 260px;
  }
  .stage {
    position: relative;
    min-height: 0;
    overflow: hidden;
  }
  .viewport {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background-color: var(--bg);
    touch-action: none;
    user-select: none;
    outline: none;
  }
  .gridlayer {
    position: absolute;
    left: -100px;
    top: -100px;
    right: 0;
    bottom: 0;
    background-image: radial-gradient(circle, var(--line-strong) 1px, transparent 1.2px);
    background-position: 0 0;
    will-change: transform;
    pointer-events: none;
  }
  .viewport.drop {
    box-shadow: inset 0 0 0 3px var(--accent);
  }
  .editlayer {
    position: absolute;
    left: 0;
    top: 0;
    width: 0;
    height: 0;
    transform-origin: 0 0;
    z-index: 2;
  }
  .editbox {
    position: absolute;
    margin: 0;
    border: none;
    outline: 2px solid var(--accent);
    border-radius: 4px;
    resize: none;
    overflow: hidden;
    line-height: 1.3;
    font-family: var(--ui);
    padding: 0.55em 0.65em;
    color: #1d1b22;
    background: transparent;
  }
  .editbox.free {
    color: var(--text);
    padding: 0.1em 0.15em;
  }
  .editbox.rect,
  .editbox.ellipse {
    color: var(--text);
    text-align: center;
    padding: 8%;
    line-height: 1.25;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
  }
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 3;
  }
  .selbox {
    position: absolute;
    border: 1.5px solid var(--accent);
    border-radius: 3px;
    margin: -1px 0 0 -1px;
  }
  .handle {
    position: absolute;
    width: 11px;
    height: 11px;
    margin: -6px 0 0 -6px;
    background: var(--bg);
    border: 2px solid var(--accent);
    border-radius: 3px;
    pointer-events: auto;
  }
  .handle.end {
    border-radius: 50%;
    width: 13px;
    height: 13px;
    margin: -7px 0 0 -7px;
    cursor: move;
  }
  .h-nw,
  .h-se {
    cursor: nwse-resize;
  }
  .h-ne,
  .h-sw {
    cursor: nesw-resize;
  }
  .h-n,
  .h-s {
    cursor: ns-resize;
  }
  .h-e,
  .h-w {
    cursor: ew-resize;
  }
  .pv {
    position: absolute;
    border: 1.5px dashed var(--accent);
    border-radius: 3px;
  }
  .pv.marquee {
    background: var(--accent-soft);
    border-style: solid;
  }
  .pv.ellipse {
    border-radius: 50%;
  }
  .pv.target {
    border: 2px solid var(--blue);
    background: var(--blue-soft);
  }
  .pvline {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .guide {
    position: absolute;
    background: #ff3dbb;
    pointer-events: none;
  }
  .guide.v {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .guide.h {
    left: 0;
    right: 0;
    height: 1px;
  }
  .inkpv {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .inkpv.hl {
    stroke-width: 16;
    opacity: 0.38;
  }
  .handle.bend {
    border-radius: 50%;
    background: var(--accent);
    cursor: grab;
  }
  .minimap {
    position: absolute;
    left: 64px;
    bottom: 12px;
    z-index: 5;
  }
  .zmenu {
    position: relative;
  }
  .zpop {
    position: absolute;
    bottom: 40px;
    right: 0;
    min-width: 240px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .zpop .zh {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    padding: 6px 8px 4px;
  }
  .zpop button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border: none;
    background: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    font-size: 13px;
  }
  .zpop button:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .zpop button:disabled {
    opacity: 0.4;
  }
  .tsw {
    margin: 2px auto;
  }
  .reveal {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    color: var(--text-2);
    padding: 0 4px;
  }
  .reveal span {
    min-width: 12px;
    text-align: center;
  }
  .mini {
    width: 20px;
    height: 20px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    background: var(--surface-2);
    cursor: pointer;
    padding: 0;
    line-height: 1;
  }
  .pvline line {
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-dasharray: 6 5;
  }
  .empty {
    position: absolute;
    left: 50%;
    top: 42%;
    transform: translate(-50%, -50%);
    text-align: center;
    max-width: 460px;
    pointer-events: none;
    color: var(--text-2);
  }
  .empty h3 {
    font: 650 22px var(--serif);
    color: var(--text);
    margin: 0 0 8px;
  }
  .tools {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 5;
  }
  .hsep {
    height: 1px;
    background: var(--line-strong);
    margin: 3px 4px;
  }
  .vsep {
    width: 1px;
    height: 20px;
    background: var(--line-strong);
    margin: 0 4px;
    flex: none;
  }
  .zoombar {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 5;
  }
  .zval {
    min-width: 54px;
    height: 30px;
    border: none;
    background: none;
    font: 12px var(--mono);
    color: var(--text-2);
    cursor: pointer;
    border-radius: 6px;
  }
  .zval:hover {
    background: var(--surface-2);
  }
  .ctx {
    position: absolute;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 5px 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 11px;
    box-shadow: var(--shadow);
    z-index: 6;
    white-space: nowrap;
    max-width: calc(100% - 24px);
    overflow-x: auto;
  }
  .ctx .icon-btn.danger:hover {
    color: var(--red);
  }
  .sw {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
    flex: none;
    box-shadow: inset 0 0 0 1px #0002;
  }
  .sw.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--bg) inset;
  }
  .sw.none {
    background: repeating-linear-gradient(45deg, var(--surface-3) 0 4px, var(--surface) 4px 8px);
  }
  .seg {
    height: 26px;
    min-width: 28px;
    padding: 0 6px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface-2);
    font-size: 12px;
    cursor: pointer;
  }
  .seg.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .fname {
    height: 28px;
    width: 180px;
  }
  .step {
    font-size: 12px;
    margin: 0 4px;
  }
  .help {
    position: absolute;
    left: 64px;
    top: 50%;
    transform: translateY(-50%);
    width: min(620px, calc(100% - 90px));
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 7;
    padding: 10px 14px 14px;
    font-size: 13px;
  }
  .help header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .hgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px 18px;
    color: var(--text-2);
  }
  .drawer {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(620px, 92%);
    background: var(--bg);
    border-left: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    z-index: 8;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .drawer header {
    position: sticky;
    top: 0;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }
  .grow {
    flex: 1;
  }
  .dbody {
    padding: 14px 16px 80px;
  }
</style>
