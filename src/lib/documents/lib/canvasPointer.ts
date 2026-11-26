// Pointer, wheel, double-click and drag-and-drop handling of the canvas editor: what a
// press does depends on the tool and on what is under the pointer (handles, items,
// empty space); moves and drops become commands on the CanvasEditor.
import {
  arrowPath as arrowGeom,
  containsPoint,
  containsRect,
  ensureCanvas,
  expand,
  intersects,
  isBox,
  newPath,
  normRect,
  pathPoints,
  ports,
  simplify,
  snap,
  snapAngle,
  toWorld,
  union,
  visibleRect,
  zoomAt,
} from './canvas';
import type { CanvasEditor } from './canvasEditor.svelte';
import { guideLines, snapEdge, snapGuides } from './diagram/guides';
import { nearestSide, nearestT } from './diagram/connectors';
import { splitStroke, strokeHit } from './diagram/eraser';
import { dnd, hasMedia, readDrop } from './dnd.svelte';
import { stickers } from '../../stores/stickers.svelte';
import { STICKER_MIME } from '../../components/StickerGrid.svelte';
import { ICON_MIME } from '../components/CanvasIcons.svelte';
import type { ArrowEnd, ArrowItem, PathItem, Rect, Side, StickerItem } from './types';

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
      /** Alt+drag: move a copy (connected to the original when it is one box) */
      dup?: boolean;
    }
  | { kind: 'resize'; id: string; handle: string; start: Rect; sx: number; sy: number; moved: boolean; minW: number; minH: number; keep?: boolean; lines?: { x: number[]; y: number[] } }
  // turned stickers scale around their centre (the pointer's distance from it), so any
  // corner works whatever the rotation
  | { kind: 'scale'; it: StickerItem; cx: number; cy: number; d0: number; w0: number; h0: number; moved: boolean }
  | { kind: 'rotate'; it: StickerItem; cx: number; cy: number; a0: number; r0: number; moved: boolean }
  | { kind: 'marquee'; x0: number; y0: number; base: Set<string> }
  | { kind: 'create'; x0: number; y0: number; sx: number; sy: number }
  | { kind: 'arrow'; from: ArrowEnd; sx: number; sy: number; line: boolean }
  | { kind: 'end'; a: ArrowItem; which: 'from' | 'to'; moved: boolean }
  | { kind: 'curve'; a: ArrowItem; moved: boolean }
  | { kind: 'label'; a: ArrowItem; moved: boolean }
  | { kind: 'erase'; moved: boolean }
  | { kind: 'ink' }
  | { kind: 'pinch' };

export class CanvasPointer {
  private drag: Drag | null = null;
  private touches = new Map<number, { x: number; y: number }>();
  private pinch: { d: number; mx: number; my: number } | null = null;

  constructor(private ed: CanvasEditor) {}

  /**
   * The item a connector end would attach to at this point, and the side port within
   * reach (ports show on the item; near a port the end sticks to that side).
   */
  private attachAt(clientX: number, clientY: number, w: { x: number; y: number }, skip?: string): { id: string; side?: Side } | null {
    const ed = this.ed;
    const reach = 16 / ed.view.z;
    const hit = ed.hitAt(clientX, clientY, skip);
    let id = hit && hit.kind !== 'arrow' && ed.tg.has(hit.id) ? hit.id : null;
    if (!id) {
      // just outside an item, next to one of its ports
      for (const [tid, t] of ed.tg) {
        if (tid === skip || t.frame) continue;
        if (!containsPoint({ x: t.rect.x - reach, y: t.rect.y - reach, w: t.rect.w + 2 * reach, h: t.rect.h + 2 * reach }, w.x, w.y)) continue;
        if (nearestSide(ports(t), w, reach)) {
          id = tid;
          break;
        }
      }
    }
    if (!id) {
      ed.portHint = null;
      return null;
    }
    const ps = ports(ed.tg.get(id)!);
    const side = nearestSide(ps, w, reach);
    ed.portHint = { id, ports: ps, active: side };
    return { id, side };
  }

  /** a connector end at w: attached (with a side when on a port) or free */
  private endAt(clientX: number, clientY: number, w: { x: number; y: number }, skip?: string): ArrowEnd {
    const at = this.attachAt(clientX, clientY, w, skip);
    if (!at) return { x: w.x, y: w.y };
    return at.side ? { x: w.x, y: w.y, id: at.id, side: at.side } : { x: w.x, y: w.y, id: at.id };
  }

  /** eraser at w: whole strokes go (stroke mode) or the touched part is cut out (split mode) */
  private erase(w: { x: number; y: number }, drag: { moved: boolean }) {
    const ed = this.ed;
    const r = 10 / ed.view.z;
    const c = ensureCanvas(ed.doc);
    for (let i = c.items.length - 1; i >= 0; i--) {
      const it = c.items[i];
      if (it.type !== 'path' || it.locked) continue;
      if (w.x < it.x - r || w.y < it.y - r || w.x > it.x + it.w + r || w.y > it.y + it.h + r) continue;
      const pts = pathPoints(it);
      if (ed.eraserMode === 'stroke') {
        if (!strokeHit(pts, it.width, w.x, w.y, r)) continue;
        if (!drag.moved) ed.before();
        drag.moved = true;
        c.items.splice(i, 1);
      } else {
        const pieces = splitStroke(pts, it.width, w.x, w.y, r);
        if (!pieces) continue;
        if (!drag.moved) ed.before();
        drag.moved = true;
        const keep = { group: it.group, z: it.z, reveal: it.reveal, opacity: it.opacity };
        const parts = pieces.map((pc) => {
          const n = newPath(simplify(pc, 0.4), it.color, it.width, !!it.highlighter) as PathItem;
          for (const [k, v] of Object.entries(keep)) if (v !== undefined) (n as unknown as Record<string, unknown>)[k] = v;
          return n;
        });
        c.items.splice(i, 1, ...parts);
      }
      if (ed.selected.has(it.id)) ed.select([...ed.selected].filter((x) => x !== it.id));
    }
  }

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
      if (h === 'label' && singleArrow) {
        this.drag = { kind: 'label', a: singleArrow, moved: false };
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
        const cands = this.nearby(new Set([single]));
        this.drag = {
          kind: 'resize',
          id: single,
          handle: h,
          start: { ...r },
          sx,
          sy,
          moved: false,
          minW: card ? 160 : 24,
          minH: card ? 90 : 24,
          lines: { x: guideLines(cands, 'x'), y: guideLines(cands, 'y') },
        };
      }
      return;
    }

    const tool = ed.tool;
    if (tool === 'pen' || tool === 'marker') {
      this.drag = { kind: 'ink' };
      ed.inkPath = [[w.x, w.y]];
      return;
    }
    if (tool === 'eraser') {
      const d = { kind: 'erase' as const, moved: false };
      this.drag = d;
      this.erase(w, d);
      return;
    }
    if (tool === 'arrow' || tool === 'line') {
      const from = this.endAt(e.clientX, e.clientY, w);
      this.drag = { kind: 'arrow', from, sx, sy, line: tool === 'line' };
      ed.preview = { kind: 'arrow', p1: w, p2: w, target: null };
      return;
    }
    if (tool !== 'select') {
      this.drag = { kind: 'create', x0: w.x, y0: w.y, sx, sy };
      ed.preview = { kind: 'rect', rect: { x: w.x, y: w.y, w: 0, h: 0 }, shape: tool === 'shape' ? ed.shapeKind : tool };
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
      if (ed.selected.has(id) && !ed.isLocked(id)) this.startMove(sx, sy, id, e.altKey && !e.shiftKey);
      return;
    }
    if (!e.shiftKey) ed.select([]);
    this.drag = { kind: 'marquee', x0: w.x, y0: w.y, base: new Set(ed.selected) };
  }

  private startMove(sx: number, sy: number, clicked: string, dup = false) {
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
    const cands = bbox ? this.nearby(ids) : [];
    this.drag = { kind: 'move', sx, sy, boxes, arrows, moved: false, clicked, bbox, cands, dup };
  }

  /** boxes in (and around) the view that guides can line up with, except `skip` */
  private nearby(skip: Set<string>): Rect[] {
    const ed = this.ed;
    const cands: Rect[] = [];
    const near = expand(visibleRect(ed.view, ed.vw, ed.vh), 0.2);
    for (const id of ed.allIds()) {
      if (skip.has(id)) continue;
      const it = ed.itemMap.get(id);
      if (it && (it.type === 'arrow' || it.type === 'path')) continue;
      const r = ed.rectOf(id);
      if (r && intersects(r, near)) cands.push({ ...r });
      if (cands.length > 400) break;
    }
    return cands;
  }

  /** Alt+drag started: copy the selection in place and drag the copies instead */
  private startDuplicate(drag: Extract<Drag, { kind: 'move' }>) {
    const ed = this.ed;
    const orig = [...ed.selected];
    const one = orig.length === 1 ? ed.itemMap.get(orig[0]) : undefined;
    const card = orig.length === 1 && ed.secMap.has(orig[0]);
    const res = ed.duplicateSelection(0, 0, false);
    if (!res || !res.ids.length) return;
    if ((one && isBox(one) && one.type !== 'frame' && one.type !== 'path') || card) ed.connect(orig[0], res.idMap.get(orig[0])!);
    this.startMove(drag.sx, drag.sy, res.ids[0]);
    (this.drag as Extract<Drag, { kind: 'move' }>).moved = true;
  }

  move(e: PointerEvent): void {
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
    if (!drag) {
      // hover: connection points under the connector tools, the eraser circle
      const tool = ed.tool;
      if (tool === 'arrow' || tool === 'line') this.attachAt(e.clientX, e.clientY, ed.worldPt(e));
      else if (ed.portHint) ed.portHint = null;
      ed.eraserAt = tool === 'eraser' ? ed.worldPt(e) : null;
      return;
    }
    const view = ed.view;
    const { sx, sy } = ed.screenPt(e);
    const w = toWorld(view, sx, sy);
    // Ctrl (or Alt, except when Alt+drag copies) moves freely: no grid, no guides
    const free = e.ctrlKey || e.metaKey || (e.altKey && !(drag.kind === 'move' && drag.dup));
    const snapOn = ed.gridOn && !free;
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
          if (drag.dup) {
            this.startDuplicate(drag);
            if (this.drag !== drag) return this.move(e);
          }
        }
        let dx = (sx - drag.sx) / view.z,
          dy = (sy - drag.sy) / view.z;
        const first = drag.boxes[0];
        // smart guides first (edges/centres line up with other boxes), then the grid
        let gx = false,
          gy = false;
        if (drag.bbox && !free) {
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
        // moving edges line up with other boxes first (guides), then the grid
        const thr = 6 / view.z;
        const gx: number[] = [],
          gy: number[] = [];
        const edge = (v: number, axis: 'x' | 'y') => {
          if (!free && drag.lines) {
            const g = snapEdge(v, drag.lines[axis], thr);
            if (g.guide !== null) {
              (axis === 'x' ? gx : gy).push(g.guide);
              return g.v;
            }
          }
          return snap(v, snapOn);
        };
        if (h.includes('w')) x0 = edge(o.x + dx, 'x');
        if (h.includes('e')) x1 = edge(o.x + o.w + dx, 'x');
        if (h.includes('n')) y0 = edge(o.y + dy, 'y');
        if (h.includes('s')) y1 = edge(o.y + o.h + dy, 'y');
        ed.guides = { x: gx, y: gy };
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
        const to = this.endAt(e.clientX, e.clientY, w, drag.from.id);
        const a: ArrowItem = { id: '', type: 'arrow', from: drag.from, to, color: 'ink' };
        if (ed.route !== 'straight') a.route = ed.route;
        const g = arrowGeom(a, ed.tg);
        ed.preview = { kind: 'arrow', p1: g.p1, p2: g.p2, target: to.id ?? null, d: g.d };
        break;
      }
      case 'label': {
        if (!drag.moved) {
          drag.moved = true;
          ed.dragging = true;
          ed.before();
        }
        const g = arrowGeom(drag.a, ed.tg);
        drag.a.labelAt = Math.round(nearestT(g.poly, w) * 1000) / 1000;
        break;
      }
      case 'erase':
        ed.eraserAt = w;
        this.erase(w, drag);
        break;
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
        const other = drag.which === 'from' ? drag.a.to : drag.a.from;
        const end = this.endAt(e.clientX, e.clientY, w, drag.a.id);
        if (end.id && end.id === other.id) {
          ed.portHint = null;
          drag.a[drag.which] = { x: w.x, y: w.y };
        } else drag.a[drag.which] = end;
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
    ed.portHint = null;
    if (!d) return;
    const view = ed.view;
    if (d.kind === 'erase') {
      if (d.moved) ed.change();
      return;
    }
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
    if (d.kind === 'curve' || d.kind === 'label') {
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
        const to = this.endAt(e.clientX, e.clientY, w, d.from.id);
        ed.portHint = null;
        ed.before();
        const a = ed.addConnector(d.from, to.id && to.id === d.from.id ? { x: w.x, y: w.y } : to, d.line);
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
    if (e.dataTransfer?.types.includes(STICKER_MIME) || e.dataTransfer?.types.includes(ICON_MIME)) {
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
    const icon = e.dataTransfer?.getData(ICON_MIME);
    if (icon) {
      e.preventDefault();
      ed.addIcon(icon, ed.worldPt(e));
      return;
    }
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
