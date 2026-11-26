// Pointer, wheel, double-click and drag-and-drop handling of the canvas editor: what a
// press does depends on the tool and on what is under the pointer (handles, items,
// empty space); moves and drops become commands on the CanvasEditor.
import {
  arrowPath as arrowGeom,
  arrowPoints,
  containsRect,
  ensureCanvas,
  expand,
  intersects,
  newArrow,
  newPath,
  normRect,
  simplify,
  snap,
  snapAngle,
  toWorld,
  union,
  visibleRect,
  zoomAt,
} from './canvas';
import type { CanvasEditor } from './canvasEditor.svelte';
import { snapGuides } from './diagram/guides';
import { dnd, hasMedia, readDrop } from './dnd.svelte';
import { stickers } from '../../stores/stickers.svelte';
import { STICKER_MIME } from '../../components/StickerGrid.svelte';
import type { ArrowEnd, ArrowItem, Rect, StickerItem } from './types';

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
  | { kind: 'resize'; id: string; handle: string; start: Rect; sx: number; sy: number; moved: boolean; minW: number; minH: number; keep?: boolean }
  // turned stickers scale around their centre (the pointer's distance from it), so any
  // corner works whatever the rotation
  | { kind: 'scale'; it: StickerItem; cx: number; cy: number; d0: number; w0: number; h0: number; moved: boolean }
  | { kind: 'rotate'; it: StickerItem; cx: number; cy: number; a0: number; r0: number; moved: boolean }
  | { kind: 'marquee'; x0: number; y0: number; base: Set<string> }
  | { kind: 'create'; x0: number; y0: number; sx: number; sy: number }
  | { kind: 'arrow'; from: ArrowEnd; sx: number; sy: number }
  | { kind: 'end'; a: ArrowItem; which: 'from' | 'to'; moved: boolean }
  | { kind: 'curve'; a: ArrowItem; moved: boolean }
  | { kind: 'ink' }
  | { kind: 'pinch' };

export class CanvasPointer {
  private drag: Drag | null = null;
  private touches = new Map<number, { x: number; y: number }>();
  private pinch: { d: number; mx: number; my: number } | null = null;

  constructor(private ed: CanvasEditor) {}

  down(e: PointerEvent) {
    const ed = this.ed;
    const t = e.target as HTMLElement;
    if (t.closest('.editbox')) return;
    if (ed.editingCard) {
      if (t.closest('.card.editing .edit, .card.editing .toolbar')) return;
      ed.editingCard = null;
    }
    if (ed.editing) ed.commitEdit();
    if (e.pointerType === 'touch') {
      this.touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (this.touches.size === 2) {
        this.drag = { kind: 'pinch' };
        ed.preview = null;
        const [a, b] = [...this.touches.values()];
        this.pinch = { d: Math.hypot(a.x - b.x, a.y - b.y), mx: (a.x + b.x) / 2, my: (a.y + b.y) / 2 };
        return;
      }
    }
    const { sx, sy } = ed.screenPt(e);
    const view = ed.view;
    const w = toWorld(view, sx, sy);
    ed.vp!.setPointerCapture(e.pointerId);

    if (e.button === 1 || (e.button === 0 && (ed.spaceDown || ed.tool === 'hand'))) {
      e.preventDefault();
      this.drag = { kind: 'pan', sx, sy, tx: view.tx, ty: view.ty };
      ed.panning = true;
      return;
    }
    if (e.button !== 0) return;

    const handle = t.closest('[data-handle]') as HTMLElement | null;
    if (handle) {
      const h = handle.dataset.handle!;
      const singleArrow = ed.singleArrow;
      if (h === 'curve' && singleArrow) {
        this.drag = { kind: 'curve', a: singleArrow, moved: false };
        return;
      }
      if ((h === 'from' || h === 'to') && singleArrow) {
        this.drag = { kind: 'end', a: singleArrow, which: h, moved: false };
        return;
      }
      const st = ed.singleSticker;
      if (st) {
        const cx = st.x + st.w / 2,
          cy = st.y + st.h / 2;
        if (h === 'rotate') this.drag = { kind: 'rotate', it: st, cx, cy, a0: (Math.atan2(w.y - cy, w.x - cx) * 180) / Math.PI, r0: st.rotation ?? 0, moved: false };
        else if (st.rotation) this.drag = { kind: 'scale', it: st, cx, cy, d0: Math.max(1, Math.hypot(w.x - cx, w.y - cy)), w0: st.w, h0: st.h, moved: false };
        else this.drag = { kind: 'resize', id: st.id, handle: h, start: { ...st }, sx, sy, moved: false, minW: 16, minH: 16, keep: true };
        return;
      }
      const single = ed.single;
      if (single) {
        const r = ed.rectOf(single)!;
        const card = ed.secMap.has(single);
        this.drag = { kind: 'resize', id: single, handle: h, start: { ...r }, sx, sy, moved: false, minW: card ? 160 : 24, minH: card ? 90 : 24 };
      }
      return;
    }

    const tool = ed.tool;
    if (tool === 'pen' || tool === 'marker') {
      this.drag = { kind: 'ink' };
      ed.inkPath = [[w.x, w.y]];
      return;
    }
    if (tool === 'arrow') {
      const hit = ed.hitAt(e.clientX, e.clientY);
      const from: ArrowEnd = hit && hit.kind !== 'arrow' ? { x: w.x, y: w.y, id: hit.id } : { x: w.x, y: w.y };
      this.drag = { kind: 'arrow', from, sx, sy };
      ed.preview = { kind: 'arrow', p1: w, p2: w, target: null };
      return;
    }
    if (tool !== 'select') {
      this.drag = { kind: 'create', x0: w.x, y0: w.y, sx, sy };
      ed.preview = { kind: 'rect', rect: { x: w.x, y: w.y, w: 0, h: 0 }, shape: tool };
      return;
    }

    const hitEl = t.closest('[data-cid]') as HTMLElement | null;
    if (hitEl) {
      const id = hitEl.dataset.cid!;
      // let native video controls work on a selected card
      if (t.closest('video') && ed.selected.has(id)) {
        this.drag = null;
        ed.vp!.releasePointerCapture(e.pointerId);
        return;
      }
      if (e.shiftKey) {
        const s = new Set(ed.selected);
        const members = e.altKey ? new Set([id]) : ed.withGroups([id]);
        if (s.has(id)) for (const m of members) s.delete(m);
        else for (const m of members) s.add(m);
        ed.select(s);
      } else if (!ed.selected.has(id)) ed.select(e.altKey ? [id] : ed.withGroups([id]));
      if (ed.selected.has(id) && !ed.isLocked(id)) this.startMove(sx, sy, id);
      return;
    }
    if (!e.shiftKey) ed.select([]);
    this.drag = { kind: 'marquee', x0: w.x, y0: w.y, base: new Set(ed.selected) };
  }

  private startMove(sx: number, sy: number, clicked: string) {
    const ed = this.ed;
    const ids = new Set(ed.selected);
    // frames carry what is inside them
    for (const id of ed.selected) {
      const f = ed.itemMap.get(id);
      if (f?.type !== 'frame') continue;
      for (const other of ed.allIds()) {
        if (other === id || ids.has(other)) continue;
        const r = ed.rectOf(other);
        if (r && containsRect(f, r)) ids.add(other);
      }
    }
    const boxes: { o: Rect; x: number; y: number }[] = [];
    const arrows: { a: ArrowItem; from: ArrowEnd; to: ArrowEnd }[] = [];
    for (const id of ids) {
      if (ed.isLocked(id)) continue;
      const s = ed.secMap.get(id);
      if (s?.canvas) boxes.push({ o: s.canvas, x: s.canvas.x, y: s.canvas.y });
      const it = ed.itemMap.get(id);
      if (!it) continue;
      if (it.type === 'arrow') arrows.push({ a: it, from: { ...it.from }, to: { ...it.to } });
      else boxes.push({ o: it, x: it.x, y: it.y });
    }
    // alignment guides: edges and centres of nearby boxes that aren't moving
    const bbox = union(boxes.map((b) => ({ x: b.x, y: b.y, w: b.o.w, h: b.o.h })));
    const cands: Rect[] = [];
    if (bbox) {
      const near = expand(visibleRect(ed.view, ed.vw, ed.vh), 0.2);
      for (const id of ed.allIds()) {
        if (ids.has(id)) continue;
        const it = ed.itemMap.get(id);
        if (it && (it.type === 'arrow' || it.type === 'path')) continue;
        const r = ed.rectOf(id);
        if (r && intersects(r, near)) cands.push({ ...r });
        if (cands.length > 400) break;
      }
    }
    this.drag = { kind: 'move', sx, sy, boxes, arrows, moved: false, clicked, bbox, cands };
  }

  move(e: PointerEvent) {
    const ed = this.ed;
    if (e.pointerType === 'touch' && this.touches.has(e.pointerId)) {
      this.touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (this.drag?.kind === 'pinch' && this.touches.size === 2 && this.pinch) {
        const [a, b] = [...this.touches.values()];
        const d = Math.hypot(a.x - b.x, a.y - b.y);
        const mx = (a.x + b.x) / 2,
          my = (a.y + b.y) / 2;
        const r = ed.vp!.getBoundingClientRect();
        const pinch = this.pinch;
        let v = { ...ed.view, tx: ed.view.tx + mx - pinch.mx, ty: ed.view.ty + my - pinch.my };
        v = zoomAt(v, d / pinch.d, mx - r.left, my - r.top);
        ed.setView(v);
        this.pinch = { d, mx, my };
        return;
      }
    }
    const drag = this.drag;
    if (!drag) return;
    const view = ed.view;
    const { sx, sy } = ed.screenPt(e);
    const w = toWorld(view, sx, sy);
    const snapOn = ed.gridOn && !e.altKey;
    switch (drag.kind) {
      case 'pan':
        ed.setView({ ...view, tx: drag.tx + sx - drag.sx, ty: drag.ty + sy - drag.sy });
        break;
      case 'move': {
        if (!drag.moved) {
          if (Math.hypot(sx - drag.sx, sy - drag.sy) < 3) return;
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        let dx = (sx - drag.sx) / view.z,
          dy = (sy - drag.sy) / view.z;
        const first = drag.boxes[0];
        // smart guides first (edges/centres line up with other boxes), then the grid
        let gx = false,
          gy = false;
        if (drag.bbox && !e.altKey) {
          const g = snapGuides(drag.bbox, dx, dy, drag.cands, 6 / view.z);
          dx = g.dx;
          dy = g.dy;
          gx = g.x.length > 0;
          gy = g.y.length > 0;
          ed.guides = { x: g.x, y: g.y };
        } else ed.guides = { x: [], y: [] };
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
          ed.dragging = true;
          ed.before();
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
        if ((e.shiftKey || drag.keep) && h.length === 2) {
          // keep the aspect ratio
          const ratio = o.w / o.h;
          const nw = x1 - x0;
          const nh = nw / ratio;
          if (h.includes('n')) y0 = y1 - nh;
          else y1 = y0 + nh;
        }
        const r = ed.rectOf(drag.id);
        if (r) {
          r.x = x0;
          r.y = y0;
          r.w = x1 - x0;
          r.h = y1 - y0;
          const it = ed.itemMap.get(drag.id);
          if (it?.type === 'note' && it.autoW && (h.includes('e') || h.includes('w'))) delete it.autoW;
        }
        break;
      }
      case 'scale': {
        if (!drag.moved) {
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        const k = Math.hypot(w.x - drag.cx, w.y - drag.cy) / drag.d0;
        const nw = Math.max(16, drag.w0 * k),
          nh = (nw * drag.h0) / drag.w0;
        drag.it.x = drag.cx - nw / 2;
        drag.it.y = drag.cy - nh / 2;
        drag.it.w = nw;
        drag.it.h = nh;
        break;
      }
      case 'rotate': {
        if (!drag.moved) {
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        let a = drag.r0 + (Math.atan2(w.y - drag.cy, w.x - drag.cx) * 180) / Math.PI - drag.a0;
        // Shift: 15° steps; otherwise only a gentle pull to the quarter turns
        if (e.shiftKey) a = Math.round(a / 15) * 15;
        a = snapAngle(a);
        if (a) drag.it.rotation = a;
        else delete drag.it.rotation;
        break;
      }
      case 'marquee': {
        const m = normRect(drag.x0, drag.y0, w.x, w.y);
        ed.preview = { kind: 'rect', rect: m, shape: 'marquee' };
        const s = new Set(drag.base);
        for (const id of ed.allIds()) {
          const r = ed.rectOf(id);
          if (!r || ed.isLocked(id)) continue;
          const it = ed.itemMap.get(id);
          if (it?.type === 'frame' ? containsRect(m, r) : intersects(m, r)) s.add(id);
        }
        ed.select(e.altKey ? s : ed.withGroups(s));
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
        ed.preview = { kind: 'rect', rect: r, shape: ed.tool };
        break;
      }
      case 'arrow': {
        const hit = ed.hitAt(e.clientX, e.clientY, drag.from.id);
        const target = hit && hit.kind !== 'arrow' ? hit.id : null;
        const a: ArrowItem = { id: '', type: 'arrow', from: drag.from, to: target ? { ...w, id: target } : w, color: 'ink' };
        const { p1, p2 } = arrowPoints(a, ed.tg);
        ed.preview = { kind: 'arrow', p1, p2, target };
        break;
      }
      case 'ink': {
        const ink = ed.inkPath;
        if (ink) {
          const l = ink[ink.length - 1];
          if (Math.hypot(w.x - l[0], w.y - l[1]) * view.z > 1.5) ed.inkPath = [...ink, [w.x, w.y]];
        }
        break;
      }
      case 'curve': {
        if (!drag.moved) {
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        const g0 = arrowGeom({ ...drag.a, curve: 0 }, ed.tg);
        // signed distance of the pointer from the straight line → bend (×2: the curve's
        // middle lies halfway between the line and the control point)
        const d = (w.x - g0.mid.x) * g0.normal.x + (w.y - g0.mid.y) * g0.normal.y;
        drag.a.curve = Math.abs(d) * view.z < 6 ? undefined : Math.round(d * 2);
        break;
      }
      case 'end': {
        if (!drag.moved) {
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        const hit = ed.hitAt(e.clientX, e.clientY, drag.a.id);
        const other = drag.which === 'from' ? drag.a.to : drag.a.from;
        const target = hit && hit.kind !== 'arrow' && hit.id !== other.id ? hit.id : undefined;
        drag.a[drag.which] = target ? { x: w.x, y: w.y, id: target } : { x: w.x, y: w.y };
        break;
      }
    }
  }

  up(e: PointerEvent) {
    const ed = this.ed;
    this.touches.delete(e.pointerId);
    if (this.drag?.kind === 'pinch') {
      if (this.touches.size < 2) {
        this.drag = null;
        this.pinch = null;
      }
      return;
    }
    const d = this.drag;
    this.drag = null;
    ed.panning = false;
    ed.dragging = false;
    ed.guides = { x: [], y: [] };
    if (!d) return;
    const view = ed.view;
    if (d.kind === 'ink') {
      const pts = ed.inkPath;
      ed.inkPath = null;
      if (pts && pts.length > 1) {
        const marker = ed.tool === 'marker';
        const simple = simplify(pts, 0.8 / view.z);
        ed.before();
        const it = newPath(simple, ed.inkColor, (marker ? 18 : 4) / Math.max(0.25, Math.min(view.z, 2)), marker);
        ed.insertItem(it);
        ed.change();
      }
      return;
    }
    if (d.kind === 'curve') {
      if (d.moved) ed.change();
      return;
    }
    const { sx, sy } = ed.screenPt(e);
    const w = toWorld(view, sx, sy);
    switch (d.kind) {
      case 'move':
        if (d.moved) ed.change();
        else if (!e.shiftKey && ed.selected.size > 1) ed.select([d.clicked]);
        break;
      case 'resize':
        if (d.moved) {
          ed.change();
          ed.fitNotes([d.id]);
        }
        break;
      case 'end':
      case 'scale':
      case 'rotate':
        if (d.moved) ed.change();
        break;
      case 'marquee':
        ed.preview = null;
        break;
      case 'create': {
        const p = ed.preview;
        const r = p?.kind === 'rect' ? p.rect : null;
        ed.preview = null;
        const small = !r || r.w * view.z < 10 || r.h * view.z < 10;
        ed.create(ed.tool, small ? null : r, w.x, w.y);
        break;
      }
      case 'arrow': {
        const p = ed.preview?.kind === 'arrow' ? ed.preview : null;
        ed.preview = null;
        if (!p) break;
        if (!p.target && Math.hypot(sx - d.sx, sy - d.sy) < 12) break;
        const to: ArrowEnd = p.target ? { x: w.x, y: w.y, id: p.target } : { x: w.x, y: w.y };
        ed.before();
        const a = newArrow(d.from, to);
        ensureCanvas(ed.doc).items.push(a);
        ed.select([a.id]);
        ed.tool = 'select';
        ed.change();
        break;
      }
    }
  }

  dblclick(e: MouseEvent) {
    const ed = this.ed;
    // pointer capture retargets dblclick to the viewport: find the item under the pointer
    const top = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    if (top?.closest('.editbox, [data-handle], .ctx, .tools, .zoombar, .drawer, .help')) return;
    const hit = ed.hitAt(e.clientX, e.clientY);
    if (hit) {
      const { id, kind } = hit;
      if (kind === 'card') {
        const sec = ed.secMap.get(id);
        // big enough on screen: edit the text in place; otherwise the side editor
        if (sec?.canvas && sec.canvas.w * ed.view.z >= 240 && !ed.isLocked(id)) {
          ed.select([id]);
          ed.editingCard = id;
        } else ed.openDrawer(id);
      } else if (kind === 'note' || kind === 'shape') ed.startEdit(id);
      else if (kind === 'frame' || kind === 'arrow') {
        ed.select([id]);
        ed.focusCtxInput();
      }
      return;
    }
    if (ed.tool === 'select') {
      const w = ed.worldPt(e);
      ed.create('note', null, w.x, w.y);
    }
  }

  wheel(e: WheelEvent) {
    const ed = this.ed;
    const t = e.target as HTMLElement;
    if (t.closest('.editbox')) return;
    // a selected card with a playing video keeps native scrolling off; the canvas owns the wheel
    e.preventDefault();
    const { sx, sy } = ed.screenPt(e);
    const unit = e.deltaMode === 1 ? 33 : e.deltaMode === 2 ? ed.vh : 1;
    const dx = e.deltaX * unit,
      dy = e.deltaY * unit;
    const view = ed.view;
    ed.stopAnim();
    if (e.ctrlKey || e.metaKey || (ed.wheelZoom && !e.shiftKey)) {
      const f = Math.exp(-dy * (e.ctrlKey && Math.abs(dy) < 50 ? 0.01 : 0.0018));
      ed.setView(zoomAt(view, f, sx, sy));
    } else {
      ed.setView({ ...view, tx: view.tx - (e.shiftKey && !dx ? dy : dx), ty: view.ty - (e.shiftKey && !dx ? 0 : dy) });
    }
  }

  dragOver(e: DragEvent) {
    if (e.dataTransfer?.types.includes(STICKER_MIME)) {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
      return;
    }
    if (!hasMedia(e)) return;
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'copy';
    this.ed.dropHint = true;
  }

  drop(e: DragEvent) {
    const ed = this.ed;
    ed.dropHint = false;
    const sid = e.dataTransfer?.getData(STICKER_MIME);
    if (sid) {
      e.preventDefault();
      const info = stickers.info(sid);
      if (info) ed.addSticker(info, ed.worldPt(e));
      return;
    }
    const list = readDrop(e);
    dnd.dragging = false;
    if (!list) return;
    e.preventDefault();
    const refs = list.map((x) => ({ libraryId: x.libraryId, path: x.path }));
    const hit = ed.hitAt(e.clientX, e.clientY);
    if (hit?.kind === 'card') {
      ed.ds.addMedia(hit.id, refs);
      ed.select([hit.id]);
      return;
    }
    ed.addMediaCard(refs, ed.worldPt(e));
  }
}
