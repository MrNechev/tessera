<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { onFontsLoaded, cssFont } from '../../assets/fonts';
  import { editor } from '../../editor/editor.svelte';
  import {
    HANDLES,
    boundsOf,
    center,
    corners,
    handlePoint,
    hitElement,
    normRect,
    rectsIntersect,
    resizeBox,
    rotatePoint,
    type Handle,
    type Pt,
  } from '../../editor/geometry';
  import type { CollageElement, PhotoElement, Rect, TextElement } from '../../model/types';
  import { AdjustCache } from '../../render/adjust';
  import { photoWindow } from '../../render/frameGeometry';
  import { coverRect, enterElement, renderDocument, type RenderResources } from '../../render/renderer';
  import { layoutText } from '../../render/text';
  import { drag } from '../../stores/drag.svelte';
  import { library } from '../../stores/library.svelte';
  import { stickers } from '../../stores/stickers.svelte';
  import { AUTO_PLACEHOLDER } from '../../templates/instantiate';
  import FloatingToolbar from './FloatingToolbar.svelte';

  interface Props {
    onRequestPhoto?: (elementId: string) => void;
  }
  let { onRequestPhoto }: Props = $props();

  let wrap: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let textarea: HTMLTextAreaElement | undefined = $state();
  let cw = $state(0);
  let ch = $state(0);
  let dpr = $state(window.devicePixelRatio || 1);
  let fontTick = $state(0);
  let tick = $state(0);
  let spaceDown = $state(false);
  let cursor = $state('default');
  let dragging = $state(false);

  const resources: RenderResources = {
    image: (id, need) => library.entry(id, need),
    sticker: (id) => stickers.image(id),
    adjustCache: new AdjustCache(16),
  };

  /* ------------------------------------------------------------------ view */

  const pageW = $derived(editor.doc.page.width);
  const pageH = $derived(editor.doc.page.height);

  export function fitView(): void {
    if (!cw || !ch) return;
    const pad = 56;
    const z = Math.max(0.005, Math.min((cw - pad * 2) / pageW, (ch - pad * 2) / pageH));
    editor.zoom = z;
    editor.panX = (cw - pageW * z) / 2;
    editor.panY = (ch - pageH * z) / 2;
    editor.fit = true;
  }

  export function zoomBy(factor: number, cx = cw / 2, cy = ch / 2): void {
    const z = editor.zoom;
    const nz = Math.max(0.005, Math.min(8, z * factor));
    editor.panX = cx - ((cx - editor.panX) * nz) / z;
    editor.panY = cy - ((cy - editor.panY) * nz) / z;
    editor.zoom = nz;
    editor.fit = false;
  }

  export function zoomTo(z: number): void {
    zoomBy(z / editor.zoom);
  }

  $effect(() => {
    void cw;
    void ch;
    void pageW;
    void pageH;
    if (editor.fit) untrack(fitView);
  });

  function toPage(clientX: number, clientY: number): Pt {
    const r = canvas!.getBoundingClientRect();
    return { x: (clientX - r.left - editor.panX) / editor.zoom, y: (clientY - r.top - editor.panY) / editor.zoom };
  }

  function toScreen(p: Pt): Pt {
    return { x: editor.panX + p.x * editor.zoom, y: editor.panY + p.y * editor.zoom };
  }

  function insideCanvas(clientX: number, clientY: number): boolean {
    if (!canvas) return false;
    const r = canvas.getBoundingClientRect();
    return clientX >= r.left && clientX <= r.right && clientY >= r.top && clientY <= r.bottom;
  }

  /* ------------------------------------------------------- interaction gesture */

  type DragState =
    | { kind: 'none' }
    | { kind: 'pan'; sx: number; sy: number; px: number; py: number }
    | { kind: 'move'; p0: Pt; start: Map<string, Pt>; bounds: Rect; moved: boolean; clickId: string | null; swap: boolean }
    | { kind: 'resize'; id: string; handle: Handle; fx: number; fy: number; start: CollageElement }
    | { kind: 'rotate'; id: string; c: Pt; a0: number; r0: number }
    | { kind: 'slot'; id: string; p0: Pt; crop0: { x: number; y: number; zoom: number }; inside: boolean }
    | { kind: 'crop'; id: string; p0: Pt; crop0: { x: number; y: number; zoom: number } }
    | { kind: 'marquee'; p0: Pt; p1: Pt; base: string[] };

  let gesture: DragState = { kind: 'none' };
  let hoverId: string | null = null;
  let dropTargetId: string | null = null;
  let guides: { x?: number; y?: number }[] = [];
  let pointer: Pt = { x: 0, y: 0 };

  let frame = 0;
  function requestDraw(): void {
    if (!frame) frame = requestAnimationFrame(draw);
  }

  $effect(() => {
    void editor.doc;
    void editor.selection;
    void editor.zoom;
    void editor.panX;
    void editor.panY;
    void editor.cropTarget;
    void editor.editingText;
    void library.version;
    void stickers.version;
    void fontTick;
    void tick;
    void cw;
    void ch;
    requestDraw();
  });

  /* ------------------------------------------------------------- hit tests */

  function elementAt(p: Pt, filter: (e: CollageElement) => boolean = () => true): CollageElement | null {
    const els = editor.doc.elements;
    const slop = 2 / editor.zoom;
    for (let i = els.length - 1; i >= 0; i--) {
      const e = els[i];
      if (e.hidden || e.locked || !filter(e)) continue;
      if (hitElement(e, p, slop)) return e;
    }
    return null;
  }

  function photoAt(p: Pt, exclude?: string): PhotoElement | null {
    return elementAt(p, (e) => e.kind === 'photo' && e.id !== exclude) as PhotoElement | null;
  }

  function isFree(e: CollageElement): boolean {
    return !(e.kind === 'photo' && e.slot) && !e.locked;
  }

  function handlesFor(e: CollageElement): typeof HANDLES {
    if (e.kind === 'text') return HANDLES.filter((h) => h.id !== 'n' && h.id !== 's');
    return HANDLES;
  }

  function rotationHandle(e: CollageElement): Pt {
    const top = toScreen(handlePoint(e, 0.5, 0));
    const a = (e.rotation * Math.PI) / 180;
    return { x: top.x + Math.sin(a) * 26, y: top.y - Math.cos(a) * 26 };
  }

  function handleAt(clientX: number, clientY: number): { el: CollageElement; handle: Handle | 'rotate'; fx: number; fy: number } | null {
    const el = editor.single;
    if (!el || !isFree(el) || editor.cropTarget) return null;
    const r = canvas!.getBoundingClientRect();
    const sx = clientX - r.left;
    const sy = clientY - r.top;
    const rh = rotationHandle(el);
    if (Math.hypot(sx - rh.x, sy - rh.y) <= 9) return { el, handle: 'rotate', fx: 0.5, fy: 0 };
    for (const h of handlesFor(el)) {
      const p = toScreen(handlePoint(el, h.fx, h.fy));
      if (Math.abs(sx - p.x) <= 7 && Math.abs(sy - p.y) <= 7) return { el, handle: h.id, fx: h.fx, fy: h.fy };
    }
    return null;
  }

  const RESIZE_CURSORS = ['ns-resize', 'nesw-resize', 'ew-resize', 'nwse-resize'];
  function cursorForHandle(h: Handle, rotation: number): string {
    const base: Record<Handle, number> = { n: 0, ne: 1, e: 2, se: 3, s: 0, sw: 1, w: 2, nw: 3 };
    const steps = Math.round((((rotation % 180) + 180) % 180) / 45);
    return RESIZE_CURSORS[(base[h] + steps) % 4];
  }

  /* -------------------------------------------------------------- snapping */

  function snap(ids: Set<string>, b: Rect, dx: number, dy: number, enabled: boolean): { dx: number; dy: number } {
    guides = [];
    if (!enabled) return { dx, dy };
    const th = 6 / editor.zoom;
    const xs = [0, pageW / 2, pageW];
    const ys = [0, pageH / 2, pageH];
    for (const e of editor.doc.elements) {
      if (ids.has(e.id) || e.hidden) continue;
      const eb = boundsOf([e])!;
      xs.push(eb.x, eb.x + eb.w / 2, eb.x + eb.w);
      ys.push(eb.y, eb.y + eb.h / 2, eb.y + eb.h);
    }
    const ex = [b.x + dx, b.x + b.w / 2 + dx, b.x + b.w + dx];
    const ey = [b.y + dy, b.y + b.h / 2 + dy, b.y + b.h + dy];
    let bestX = { d: th, adj: 0, at: NaN };
    for (const c of xs) for (const e of ex) if (Math.abs(c - e) < bestX.d) bestX = { d: Math.abs(c - e), adj: c - e, at: c };
    let bestY = { d: th, adj: 0, at: NaN };
    for (const c of ys) for (const e of ey) if (Math.abs(c - e) < bestY.d) bestY = { d: Math.abs(c - e), adj: c - e, at: c };
    if (!Number.isNaN(bestX.at)) guides.push({ x: bestX.at });
    if (!Number.isNaN(bestY.at)) guides.push({ y: bestY.at });
    return { dx: dx + bestX.adj, dy: dy + bestY.adj };
  }

  /* --------------------------------------------------------------- pointer */

  function onPointerDown(e: PointerEvent): void {
    if (!canvas) return;
    canvas.focus();
    if (editor.editingText) {
      editor.editingText = null;
    }
    const p = toPage(e.clientX, e.clientY);
    pointer = p;
    if (e.button === 1 || (e.button === 0 && spaceDown)) {
      gesture = { kind: 'pan', sx: e.clientX, sy: e.clientY, px: editor.panX, py: editor.panY };
      capture(e);
      return;
    }
    if (e.button !== 0) return;

    const h = handleAt(e.clientX, e.clientY);
    if (h) {
      editor.begin();
      if (h.handle === 'rotate') {
        const c = center(h.el);
        gesture = { kind: 'rotate', id: h.el.id, c, a0: Math.atan2(p.y - c.y, p.x - c.x), r0: h.el.rotation };
      } else {
        gesture = { kind: 'resize', id: h.el.id, handle: h.handle, fx: h.fx, fy: h.fy, start: structuredClone($state.snapshot(h.el) as CollageElement) };
      }
      capture(e);
      return;
    }

    // Crop mode: drag pans the photo inside its frame.
    if (editor.cropTarget) {
      const t = editor.element(editor.cropTarget);
      if (t && t.kind === 'photo' && hitElement(t, p)) {
        editor.begin();
        gesture = { kind: 'crop', id: t.id, p0: p, crop0: { ...t.crop } };
        capture(e);
        return;
      }
      editor.cropTarget = null;
    }

    const el = elementAt(p);
    if (el) {
      if (e.shiftKey) {
        editor.select([el.id], true);
        return;
      }
      if (el.kind === 'photo' && el.slot && el.asset) {
        editor.select([el.id]);
        editor.begin();
        gesture = { kind: 'slot', id: el.id, p0: p, crop0: { ...el.crop }, inside: true };
        capture(e);
        return;
      }
      const already = editor.selection.includes(el.id);
      if (!already) editor.select([el.id]);
      const ids = editor.selection.filter((id) => {
        const x = editor.element(id);
        return x && isFree(x);
      });
      const start = new Map<string, Pt>();
      for (const id of ids) {
        const x = editor.element(id)!;
        start.set(id, { x: x.x, y: x.y });
      }
      const bounds = boundsOf(ids.map((id) => editor.element(id)!)) ?? { x: 0, y: 0, w: 0, h: 0 };
      editor.begin();
      gesture = { kind: 'move', p0: p, start, bounds, moved: false, clickId: already ? el.id : null, swap: false };
      capture(e);
      return;
    }

    if (!e.shiftKey) editor.clearSelection();
    gesture = { kind: 'marquee', p0: p, p1: p, base: e.shiftKey ? [...editor.selection] : [] };
    capture(e);
  }

  function capture(e: PointerEvent): void {
    canvas?.setPointerCapture(e.pointerId);
    dragging = true;
  }

  function cropPan(el: PhotoElement, p0: Pt, p: Pt, crop0: { x: number; y: number; zoom: number }) {
    const a = el.asset ? editor.doc.assets[el.asset] : null;
    if (!a) return crop0;
    const win = photoWindow(el.frame, el.w, el.h);
    const r = coverRect(a.width, a.height, win, crop0);
    const d = rotatePoint({ x: p.x - p0.x, y: p.y - p0.y }, { x: 0, y: 0 }, -el.rotation);
    const ox = r.w - win.w;
    const oy = r.h - win.h;
    const sx = el.flipX ? -1 : 1;
    const sy = el.flipY ? -1 : 1;
    return {
      x: ox > 0.5 ? Math.max(0, Math.min(1, crop0.x - (sx * d.x) / ox)) : crop0.x,
      y: oy > 0.5 ? Math.max(0, Math.min(1, crop0.y - (sy * d.y) / oy)) : crop0.y,
      zoom: crop0.zoom,
    };
  }

  function onPointerMove(e: PointerEvent): void {
    if (!canvas) return;
    const p = toPage(e.clientX, e.clientY);
    pointer = p;
    switch (gesture.kind) {
      case 'none': {
        const h = handleAt(e.clientX, e.clientY);
        let hov: CollageElement | null = null;
        if (spaceDown) cursor = 'grab';
        else if (h) cursor = h.handle === 'rotate' ? 'crosshair' : cursorForHandle(h.handle, h.el.rotation);
        else {
          hov = elementAt(p);
          cursor = editor.cropTarget && hov?.id === editor.cropTarget ? 'grab' : hov ? (isFree(hov) ? 'move' : 'grab') : 'default';
        }
        const id = hov?.id ?? null;
        if (id !== hoverId) {
          hoverId = id;
          requestDraw();
        }
        return;
      }
      case 'pan':
        editor.panX = gesture.px + (e.clientX - gesture.sx);
        editor.panY = gesture.py + (e.clientY - gesture.sy);
        editor.fit = false;
        return;
      case 'move': {
        let dx = p.x - gesture.p0.x;
        let dy = p.y - gesture.p0.y;
        if (!gesture.moved && Math.hypot(dx, dy) * editor.zoom < 3) return;
        gesture.moved = true;
        // Alt-drag a photo onto another photo to swap them.
        const single = gesture.start.size === 1 ? editor.element([...gesture.start.keys()][0]) : null;
        if (e.altKey && single?.kind === 'photo') {
          gesture.swap = true;
          const t = photoAt(p, single.id);
          dropTargetId = t?.id ?? null;
          const s0 = gesture.start.get(single.id)!;
          editor.patch(single.id, { x: s0.x, y: s0.y }, { history: false });
          requestDraw();
          return;
        }
        gesture.swap = false;
        dropTargetId = null;
        if (e.shiftKey) {
          if (Math.abs(dx) > Math.abs(dy)) dy = 0;
          else dx = 0;
        }
        const ids = new Set(gesture.start.keys());
        const s = snap(ids, gesture.bounds, dx, dy, !(e.ctrlKey || e.metaKey));
        const start = gesture.start;
        editor.update(
          (d) => {
            for (const el of d.elements) {
              const s0 = start.get(el.id);
              if (s0) {
                el.x = s0.x + s.dx;
                el.y = s0.y + s.dy;
              }
            }
          },
          { history: false },
        );
        return;
      }
      case 'resize': {
        const st = gesture.start;
        const corner = gesture.fx !== 0.5 && gesture.fy !== 0.5;
        const keep = corner && (st.kind === 'photo' || st.kind === 'sticker' || st.kind === 'text') ? !e.shiftKey : e.shiftKey;
        const r = resizeBox(st, gesture.fx, gesture.fy, p, keep, 4 / editor.zoom);
        if (st.kind === 'text') {
          const t = st as TextElement;
          if (corner) {
            const k = r.w / t.w;
            editor.patch(t.id, {
              x: r.x,
              y: r.y,
              w: r.w,
              size: t.size * k,
              stroke: t.stroke ? { ...t.stroke, width: t.stroke.width * k } : null,
            } as Partial<TextElement>, { history: false });
          } else editor.patch(t.id, { x: r.x, w: r.w } as Partial<TextElement>, { history: false });
        } else {
          const patch: Partial<CollageElement> = { x: r.x, y: r.y, w: r.w, h: r.h };
          if (st.kind === 'photo' && st.border && corner) (patch as Partial<PhotoElement>).border = { ...st.border, width: (st.border.width * r.w) / st.w };
          editor.patch(st.id, patch, { history: false });
        }
        return;
      }
      case 'rotate': {
        const a = Math.atan2(p.y - gesture.c.y, p.x - gesture.c.x);
        let deg = gesture.r0 + ((a - gesture.a0) * 180) / Math.PI;
        deg = ((deg % 360) + 540) % 360 - 180;
        if (e.shiftKey) deg = Math.round(deg / 15) * 15;
        else for (const snapTo of [-180, -90, 0, 90, 180]) if (Math.abs(deg - snapTo) < 3) deg = snapTo;
        editor.patch(gesture.id, { rotation: deg }, { history: false });
        return;
      }
      case 'slot': {
        const el = editor.element(gesture.id) as PhotoElement | undefined;
        if (!el) return;
        const inside = hitElement(el, p);
        if (inside) {
          dropTargetId = null;
          editor.patch(el.id, { crop: cropPan(el, gesture.p0, p, gesture.crop0) } as Partial<PhotoElement>, { history: false });
          cursor = 'grabbing';
        } else {
          if (gesture.inside) editor.patch(el.id, { crop: gesture.crop0 } as Partial<PhotoElement>, { history: false });
          const t = photoAt(p, el.id);
          dropTargetId = t?.id ?? null;
          cursor = t ? 'copy' : 'grabbing';
        }
        gesture.inside = inside;
        requestDraw();
        return;
      }
      case 'crop': {
        const el = editor.element(gesture.id) as PhotoElement | undefined;
        if (el) editor.patch(el.id, { crop: cropPan(el, gesture.p0, p, gesture.crop0) } as Partial<PhotoElement>, { history: false });
        cursor = 'grabbing';
        return;
      }
      case 'marquee': {
        gesture.p1 = p;
        const r = normRect(gesture.p0, gesture.p1);
        const hits = editor.doc.elements.filter((x) => !x.hidden && !x.locked && rectsIntersect(r, boundsOf([x])!)).map((x) => x.id);
        editor.selection = [...new Set([...gesture.base, ...hits])];
        requestDraw();
        return;
      }
    }
  }

  function onPointerUp(e: PointerEvent): void {
    if (!canvas) return;
    canvas.releasePointerCapture?.(e.pointerId);
    dragging = false;
    const s = gesture;
    gesture = { kind: 'none' };
    guides = [];
    switch (s.kind) {
      case 'move':
        if (s.swap && dropTargetId) {
          const a = [...s.start.keys()][0];
          editor.end();
          editor.swapPhotos(a, dropTargetId);
        } else {
          editor.end();
          if (!s.moved && s.clickId) editor.select([s.clickId]);
        }
        break;
      case 'slot':
        if (!s.inside && dropTargetId) {
          editor.end();
          editor.swapPhotos(s.id, dropTargetId);
        } else editor.end();
        break;
      case 'resize':
      case 'rotate':
      case 'crop':
        editor.end();
        break;
    }
    dropTargetId = null;
    cursor = 'default';
    requestDraw();
  }

  function onDoubleClick(e: MouseEvent): void {
    const p = toPage(e.clientX, e.clientY);
    const el = elementAt(p);
    if (!el) return;
    if (el.kind === 'photo') {
      if (!el.asset) onRequestPhoto?.(el.id);
      else {
        editor.select([el.id]);
        editor.cropTarget = el.id;
      }
    } else if (el.kind === 'text') {
      editor.select([el.id]);
      editor.editingText = el.id;
    }
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault();
    const r = canvas!.getBoundingClientRect();
    const cx = e.clientX - r.left;
    const cy = e.clientY - r.top;
    const crop = editor.cropTarget ? (editor.element(editor.cropTarget) as PhotoElement | undefined) : undefined;
    if (crop && hitElement(crop, toPage(e.clientX, e.clientY)) && !e.ctrlKey) {
      const z = Math.max(1, Math.min(6, crop.crop.zoom * Math.exp(-e.deltaY * 0.0015)));
      editor.patch(crop.id, { crop: { ...crop.crop, zoom: z } } as Partial<PhotoElement>, { key: `crop-zoom:${crop.id}` });
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      zoomBy(Math.exp(-e.deltaY * 0.0022), cx, cy);
    } else {
      editor.panX -= e.shiftKey ? e.deltaY : e.deltaX;
      editor.panY -= e.shiftKey ? 0 : e.deltaY;
      editor.fit = false;
    }
  }

  /* ------------------------------------------------------------ drag & drop */

  /** Drop library photos at a screen position. Returns false if outside the canvas. */
  export function dropAssets(ids: string[], clientX: number, clientY: number): boolean {
    if (!ids.length || !insideCanvas(clientX, clientY)) return false;
    const p = toPage(clientX, clientY);
    const target = photoAt(p);
    if (target) {
      if (target.name === AUTO_PLACEHOLDER) editor.addToLayout(ids);
      else {
        editor.setPhotoAsset(target.id, ids[0]);
        editor.select([target.id]);
        const rest = ids.slice(1);
        if (rest.length) {
          if (editor.hasLayout) editor.addToLayout(rest);
          else editor.addFreePhotos(rest, p);
        }
      }
    } else if (editor.hasLayout) editor.addToLayout(ids);
    else editor.addFreePhotos(ids, p);
    return true;
  }

  /** Highlight the frame under a pointer while something is dragged over the canvas. */
  export function dragOver(clientX: number, clientY: number): void {
    const id = insideCanvas(clientX, clientY) ? (photoAt(toPage(clientX, clientY))?.id ?? null) : null;
    if (id !== dropTargetId) {
      dropTargetId = id;
      requestDraw();
    }
  }

  $effect(() => {
    if (!drag.session) return;
    const move = (e: PointerEvent) => {
      drag.move(e.clientX, e.clientY);
      dragOver(e.clientX, e.clientY);
    };
    const up = (e: PointerEvent) => {
      const s = drag.end();
      dropTargetId = null;
      requestDraw();
      if (s) dropAssets(s.assetIds, e.clientX, e.clientY);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up, { once: true });
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
    };
  });

  /* ------------------------------------------------------------------- draw */

  function pathFor(c: CanvasRenderingContext2D, el: CollageElement): void {
    const pts = corners(el).map(toScreen);
    c.beginPath();
    pts.forEach((q, i) => (i ? c.lineTo(q.x, q.y) : c.moveTo(q.x, q.y)));
    c.closePath();
  }

  function draw(): void {
    frame = 0;
    if (!canvas) return;
    const c = canvas.getContext('2d');
    if (!c) return;
    const z = editor.zoom;
    const doc = editor.doc;
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.clearRect(0, 0, canvas.width, canvas.height);

    // Page drop shadow.
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    c.save();
    c.shadowColor = 'rgba(0,0,0,0.42)';
    c.shadowBlur = 28 * dpr;
    c.shadowOffsetY = 8 * dpr;
    c.fillStyle = '#ffffff';
    c.fillRect(editor.panX, editor.panY, doc.page.width * z, doc.page.height * z);
    c.restore();

    // The document itself.
    c.setTransform(1, 0, 0, 1, editor.panX * dpr, editor.panY * dpr);
    const hidden = new Set<string>();
    if (editor.editingText) hidden.add(editor.editingText);
    renderDocument(c, doc, { scale: z * dpr, resources, placeholders: true, hidden });

    // Crop mode: show the whole photo faintly around its frame.
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    const crop = editor.cropTarget ? editor.element(editor.cropTarget) : undefined;
    if (crop && crop.kind === 'photo' && crop.asset) {
      const a = doc.assets[crop.asset];
      const img = a ? library.entry(crop.asset) : null;
      if (a && img) {
        const win = photoWindow(crop.frame, crop.w, crop.h);
        const r = coverRect(a.width, a.height, win, crop.crop);
        c.save();
        c.translate(editor.panX, editor.panY);
        c.scale(z, z);
        enterElement(c, crop);
        const clip = new Path2D();
        clip.rect(r.x, r.y, r.w, r.h);
        clip.rect(win.x, win.y, win.w, win.h);
        c.save();
        c.clip(clip, 'evenodd');
        c.globalAlpha = 0.42;
        c.drawImage(img.source, r.x, r.y, r.w, r.h);
        c.restore();
        c.lineWidth = 1.5 / z;
        c.setLineDash([6 / z, 4 / z]);
        c.strokeStyle = '#ffffff';
        c.strokeRect(r.x, r.y, r.w, r.h);
        c.setLineDash([]);
        c.strokeStyle = '#ff5d8f';
        c.lineWidth = 2 / z;
        c.strokeRect(win.x, win.y, win.w, win.h);
        c.restore();
      }
    }

    // Hover outline.
    const sel = new Set(editor.selection);
    if (hoverId && !sel.has(hoverId) && gesture.kind === 'none') {
      const el = editor.element(hoverId);
      if (el) {
        pathFor(c, el);
        c.strokeStyle = 'rgba(255,93,143,0.6)';
        c.lineWidth = 1.5;
        c.stroke();
      }
    }

    // Drop / swap target.
    if (dropTargetId) {
      const el = editor.element(dropTargetId);
      if (el) {
        pathFor(c, el);
        c.fillStyle = 'rgba(255,93,143,0.22)';
        c.fill();
        c.setLineDash([7, 5]);
        c.strokeStyle = '#ff5d8f';
        c.lineWidth = 2.5;
        c.stroke();
        c.setLineDash([]);
      }
    }

    // Selection.
    for (const el of editor.selected) {
      pathFor(c, el);
      c.strokeStyle = el.locked ? 'rgba(255,255,255,0.6)' : '#ff5d8f';
      c.setLineDash(el.locked ? [4, 4] : []);
      c.lineWidth = 1.6;
      c.stroke();
      c.setLineDash([]);
    }
    const single = editor.single;
    if (single && isFree(single) && !editor.cropTarget && !editor.editingText) {
      const top = toScreen(handlePoint(single, 0.5, 0));
      const rh = rotationHandle(single);
      c.beginPath();
      c.moveTo(top.x, top.y);
      c.lineTo(rh.x, rh.y);
      c.strokeStyle = '#ff5d8f';
      c.lineWidth = 1.4;
      c.stroke();
      c.beginPath();
      c.arc(rh.x, rh.y, 6, 0, Math.PI * 2);
      c.fillStyle = '#ffffff';
      c.fill();
      c.stroke();
      for (const h of handlesFor(single)) {
        const q = toScreen(handlePoint(single, h.fx, h.fy));
        c.save();
        c.translate(q.x, q.y);
        c.rotate((single.rotation * Math.PI) / 180);
        c.fillStyle = '#ffffff';
        c.strokeStyle = '#ff5d8f';
        c.lineWidth = 1.5;
        const corner = h.fx !== 0.5 && h.fy !== 0.5;
        c.beginPath();
        if (corner) c.rect(-4.5, -4.5, 9, 9);
        else if (h.fx === 0.5) c.roundRect(-7, -3, 14, 6, 3);
        else c.roundRect(-3, -7, 6, 14, 3);
        c.fill();
        c.stroke();
        c.restore();
      }
    }

    // Snap guides.
    c.strokeStyle = '#e040fb';
    c.lineWidth = 1;
    for (const g of guides) {
      c.beginPath();
      if (g.x !== undefined) {
        const x = toScreen({ x: g.x, y: 0 }).x;
        c.moveTo(x, editor.panY - 2000);
        c.lineTo(x, editor.panY + doc.page.height * z + 2000);
      } else if (g.y !== undefined) {
        const y = toScreen({ x: 0, y: g.y }).y;
        c.moveTo(editor.panX - 2000, y);
        c.lineTo(editor.panX + doc.page.width * z + 2000, y);
      }
      c.stroke();
    }

    // Marquee.
    if (gesture.kind === 'marquee') {
      const a = toScreen(gesture.p0);
      const b = toScreen(gesture.p1);
      const r = normRect(a, b);
      c.fillStyle = 'rgba(255,93,143,0.1)';
      c.fillRect(r.x, r.y, r.w, r.h);
      c.strokeStyle = 'rgba(255,93,143,0.8)';
      c.strokeRect(r.x + 0.5, r.y + 0.5, r.w, r.h);
    }

    // Ghost of the photo being swapped.
    if (gesture.kind === 'slot' && !gesture.inside) {
      const el = editor.element(gesture.id);
      if (el?.kind === 'photo' && el.asset) {
        const img = library.entry(el.asset, 200);
        if (img) {
          const s = toScreen(pointer);
          const gw = 120;
          const gh = (gw * img.height) / img.width;
          c.save();
          c.globalAlpha = 0.85;
          c.shadowColor = 'rgba(0,0,0,0.4)';
          c.shadowBlur = 16 * dpr;
          c.drawImage(img.source, s.x + 10, s.y + 10, gw, gh);
          c.restore();
        }
      }
    }
  }

  /* --------------------------------------------------------- text editing */

  const editingEl = $derived(editor.editingText ? (editor.element(editor.editingText) as TextElement | undefined) : undefined);

  $effect(() => {
    if (editingEl && textarea) {
      const ta = textarea;
      queueMicrotask(() => {
        ta.focus();
        ta.select();
      });
    }
  });

  const editStyle = $derived.by(() => {
    const el = editingEl;
    if (!el) return '';
    const z = editor.zoom;
    const c = toScreen(center(el));
    const L = layoutText(el);
    return [
      `left:${c.x}px`,
      `top:${c.y}px`,
      `width:${el.w * z}px`,
      `height:${Math.max(L.height, el.size * el.lineHeight) * z}px`,
      `transform:translate(-50%,-50%) rotate(${el.rotation}deg)`,
      `font:${cssFont(el.font, el.weight, el.size * z, el.italic)}`,
      `line-height:${el.lineHeight}`,
      `letter-spacing:${el.letterSpacing}em`,
      `text-align:${el.align}`,
      `color:${el.fill}`,
      `padding:${L.pad * z}px`,
      `text-transform:${el.uppercase ? 'uppercase' : 'none'}`,
    ].join(';');
  });

  /* ------------------------------------------------------------- lifecycle */

  const toolbarRect = $derived.by(() => {
    void tick;
    if (dragging || editor.editingText) return null;
    const sel = editor.selected;
    if (!sel.length) return null;
    const b = boundsOf(sel)!;
    const a = toScreen({ x: b.x, y: b.y });
    return { x: a.x, y: a.y, w: b.w * editor.zoom, h: b.h * editor.zoom };
  });

  onMount(() => {
    const ro = new ResizeObserver(() => {
      if (!wrap || !canvas) return;
      dpr = window.devicePixelRatio || 1;
      cw = wrap.clientWidth;
      ch = wrap.clientHeight;
      canvas.width = Math.round(cw * dpr);
      canvas.height = Math.round(ch * dpr);
      canvas.style.width = `${cw}px`;
      canvas.style.height = `${ch}px`;
      requestDraw();
    });
    ro.observe(wrap!);
    const off = onFontsLoaded(() => {
      fontTick++;
    });
    const kd = (e: KeyboardEvent) => {
      if (e.code === 'Space' && !(e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement)) {
        if (!spaceDown) {
          spaceDown = true;
          cursor = 'grab';
        }
        e.preventDefault();
      }
    };
    const ku = (e: KeyboardEvent) => {
      if (e.code === 'Space') {
        spaceDown = false;
        cursor = 'default';
      }
    };
    window.addEventListener('keydown', kd);
    window.addEventListener('keyup', ku);
    return () => {
      ro.disconnect();
      off();
      window.removeEventListener('keydown', kd);
      window.removeEventListener('keyup', ku);
      if (frame) cancelAnimationFrame(frame);
    };
  });
</script>

<div class="wrap" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    style:cursor
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    ondblclick={onDoubleClick}
    onwheel={onWheel}
    onpointerleave={() => {
      if (hoverId) {
        hoverId = null;
        requestDraw();
      }
    }}
  ></canvas>

  {#if editingEl}
    <textarea
      bind:this={textarea}
      class="text-edit"
      style={editStyle}
      value={editingEl.text}
      spellcheck="false"
      oninput={(e) => editor.patch(editingEl!.id, { text: (e.target as HTMLTextAreaElement).value } as Partial<TextElement>, { key: `text:${editingEl!.id}` })}
      onkeydown={(e) => {
        if (e.key === 'Escape' || (e.key === 'Enter' && (e.ctrlKey || e.metaKey))) {
          e.preventDefault();
          editor.editingText = null;
          canvas?.focus();
        }
        e.stopPropagation();
      }}
      onblur={() => (editor.editingText = null)}
    ></textarea>
  {/if}

  {#if toolbarRect && !drag.session}
    <FloatingToolbar rect={toolbarRect} {cw} {onRequestPhoto} />
  {/if}

  {#if editor.cropTarget}
    <div class="crop-hint">
      Drag to reposition · scroll to zoom · <span class="kbd">Enter</span> or <span class="kbd">Esc</span> when done
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    flex: 1;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background-color: var(--canvas-bg);
    background-image: radial-gradient(rgba(255, 255, 255, 0.045) 1px, transparent 1px);
    background-size: 18px 18px;
  }
  :global([data-theme='light']) .wrap {
    background-image: radial-gradient(rgba(40, 20, 80, 0.07) 1px, transparent 1px);
  }
  canvas {
    display: block;
    outline: none;
    touch-action: none;
  }
  .text-edit {
    position: absolute;
    margin: 0;
    border: 1.5px dashed var(--accent);
    background: rgba(255, 255, 255, 0.04);
    outline: none;
    resize: none;
    overflow: hidden;
    caret-color: var(--accent);
    white-space: pre-wrap;
    box-sizing: border-box;
  }
  .crop-hint {
    position: absolute;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    padding: 8px 14px;
    border-radius: 99px;
    background: rgba(20, 16, 28, 0.86);
    color: #eee;
    font-size: 12px;
    box-shadow: var(--shadow);
    pointer-events: none;
  }
</style>
