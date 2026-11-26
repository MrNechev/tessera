/**
 * Paint's tools: pointer handlers working in image coordinates, plus an
 * optional overlay each tool draws on top of the view (marquees, the lasso,
 * the crop box, text boxes). Strokes and shapes are drawn into the
 * document's scratch overlay and committed to the layer as one undo step.
 */
import { getStroke } from 'perfect-freehand';
import { ensureFont } from '../assets/fonts';
import { createText } from '../model/factory';
import type { TextElement } from '../model/types';
import { naturalTextWidth } from '../render/text';
import { presetElement, textPreset } from '../text/styles';
import { toasts } from '../stores/toasts.svelte';
import { cloneCanvas, ctxOf, newCanvas, type PaintDoc } from './doc.svelte';
import { fillMasked, matchColor } from './lib/flood';
import { arrowPolygon, boxOf, linePixels, snapAngle, squareBox, type Pt, type View } from './lib/geom';
import { clipRect, combine, ellipseMask, polygonMask, rectMask, unionRect, type Combine, type Rect } from './lib/mask';
import { hexOf, paint, rgbaOf, type ToolId } from './state.svelte';

export interface PointerInfo {
  /** Image coordinates. */
  x: number;
  y: number;
  /** Screen coordinates (CSS px, relative to the viewport). */
  sx: number;
  sy: number;
  pressure: number;
  pen: boolean;
  button: number;
  shift: boolean;
  alt: boolean;
  ctrl: boolean;
}

export interface Tool {
  down(p: PointerInfo): void;
  move(p: PointerInfo, all: PointerInfo[]): void;
  up(p: PointerInfo): void;
  cancel?(): void;
  /** Drawn over the image; the context is set up for screen (CSS px) coordinates. */
  overlay?(c: CanvasRenderingContext2D, v: View): void;
  /** Hover feedback without a button down. */
  hover?(p: PointerInfo): void;
}

const doc = (): PaintDoc | null => paint.doc;

/** The active layer, if it can be painted on (with a friendly hint when not). */
function paintable(d: PaintDoc): boolean {
  const a = d.active;
  if (!a) return false;
  if (a.locked) {
    toasts.push({ kind: 'info', title: `“${a.name}” is locked`, message: 'Unlock it in the Layers panel to paint on it.' });
    return false;
  }
  if (!a.visible) {
    toasts.push({ kind: 'info', title: `“${a.name}” is hidden`, message: 'Show it in the Layers panel to paint on it.' });
    return false;
  }
  return true;
}

function colorFor(p: PointerInfo): string {
  return p.button === 2 ? paint.secondary : paint.primary;
}

function bboxOf(pts: number[][] | Pt[], pad: number): Rect | null {
  if (!pts.length) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of pts) {
    const x = Array.isArray(p) ? p[0] : p.x;
    const y = Array.isArray(p) ? p[1] : p.y;
    if (x < x0) x0 = x;
    if (y < y0) y0 = y;
    if (x > x1) x1 = x;
    if (y > y1) y1 = y;
  }
  return { x: Math.floor(x0 - pad), y: Math.floor(y0 - pad), w: Math.ceil(x1 - x0 + 2 * pad) + 1, h: Math.ceil(y1 - y0 + 2 * pad) + 1 };
}

/** Pick the colour under the point (all visible layers) as primary (right button: secondary). */
export function pickColor(p: PointerInfo): void {
  const d = doc();
  if (!d) return;
  const x = Math.floor(p.x);
  const y = Math.floor(p.y);
  if (x < 0 || y < 0 || x >= d.width || y >= d.height) return;
  const src = paint.opts.sampleAll || !d.activeCanvas ? d.composite : d.activeCanvas;
  const px = src.getContext('2d')!.getImageData(x, y, 1, 1).data;
  const hex = px[3] === 0 ? '#ffffff' : hexOf(px[0], px[1], px[2], px[3]);
  if (p.button === 2) paint.setSecondary(hex);
  else paint.setPrimary(hex);
}

/* ------------------------------------------------------------ brush & eraser */

function brush(kind: 'brush' | 'eraser'): Tool {
  let pts: number[][] = [];
  let pen = false;
  let color = '#000';
  let prev: Rect | null = null;
  let frame = 0;
  let live = false;

  function render(last: boolean) {
    frame = 0;
    const d = doc();
    const ov = d?.overlay;
    if (!d || !ov) return;
    const size = paint.size;
    const s = paint.opts.smoothing;
    const outline = getStroke(pts, {
      size,
      thinning: pen ? 0.65 : 0.4,
      smoothing: 0.35 + 0.5 * s,
      streamline: 0.1 + 0.75 * s,
      simulatePressure: !pen,
      last,
      start: { cap: true },
      end: { cap: true },
    });
    const box = bboxOf(outline, 2);
    if (!box) return;
    const c = ctxOf(ov.canvas);
    if (prev) c.clearRect(prev.x, prev.y, prev.w, prev.h);
    c.fillStyle = color;
    c.beginPath();
    outline.forEach(([x, y], i) => (i ? c.lineTo(x, y) : c.moveTo(x, y)));
    c.closePath();
    c.fill();
    const all = unionRect(prev, box)!;
    d.clipToSelection(c, all);
    ov.rect = unionRect(ov.rect, box);
    // Only the end of the stroke changes; recomposite around the latest points.
    const tail = bboxOf(pts.slice(-12), size + 4);
    d.invalidate(last || !prev ? all : unionRect(tail, prev && box.w * box.h < prev.w * prev.h ? all : null));
    prev = box;
  }

  return {
    down(p) {
      const d = doc();
      if (!d || !paintable(d)) return;
      const [r, g, b, a] = rgbaOf(colorFor(p));
      const alpha = kind === 'eraser' ? paint.opts.opacity : paint.opts.opacity * (a / 255);
      if (!d.beginOverlay(alpha, kind === 'eraser' ? 'destination-out' : 'source-over')) return;
      color = kind === 'eraser' ? '#000000' : hexOf(r, g, b);
      pen = p.pen;
      pts = [[p.x, p.y, pen ? p.pressure : 0.5]];
      prev = null;
      live = true;
      render(false);
    },
    move(_p, all) {
      if (!live) return;
      for (const q of all) pts.push([q.x, q.y, pen ? q.pressure : 0.5]);
      if (!frame) frame = requestAnimationFrame(() => render(false));
    },
    up() {
      if (!live) return;
      live = false;
      if (frame) cancelAnimationFrame(frame);
      render(true);
      doc()?.endOverlay(kind === 'eraser' ? 'Erase' : 'Brush');
    },
    cancel() {
      live = false;
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      doc()?.cancelOverlay();
    },
  };
}

/* ------------------------------------------------------------ pencil */

function pencil(): Tool {
  let last: Pt | null = null;
  let live = false;
  const dab = (d: PaintDoc, a: Pt, b: Pt) => {
    const ov = d.overlay!;
    const s = paint.size;
    const h = Math.floor(s / 2);
    const c = ctxOf(ov.canvas);
    const px = linePixels(a.x, a.y, b.x, b.y);
    for (const q of px) c.fillRect(q.x - h, q.y - h, s, s);
    const box = { x: Math.floor(Math.min(a.x, b.x)) - h - 1, y: Math.floor(Math.min(a.y, b.y)) - h - 1, w: Math.abs(Math.floor(b.x) - Math.floor(a.x)) + s + 2, h: Math.abs(Math.floor(b.y) - Math.floor(a.y)) + s + 2 };
    const clipped = clipRect(box, d.width, d.height);
    if (!clipped) return;
    d.clipToSelection(c, clipped);
    ov.rect = unionRect(ov.rect, clipped);
    d.invalidate(clipped);
  };
  return {
    down(p) {
      const d = doc();
      if (!d || !paintable(d)) return;
      const [r, g, b, a] = rgbaOf(colorFor(p));
      if (!d.beginOverlay(paint.opts.opacity * (a / 255), 'source-over')) return;
      ctxOf(d.overlay!.canvas).fillStyle = hexOf(r, g, b);
      last = { x: p.x, y: p.y };
      live = true;
      dab(d, last, last);
    },
    move(_p, all) {
      const d = doc();
      if (!live || !d?.overlay) return;
      for (const q of all) {
        dab(d, last!, q);
        last = { x: q.x, y: q.y };
      }
    },
    up() {
      if (!live) return;
      live = false;
      doc()?.endOverlay('Pencil');
    },
    cancel() {
      live = false;
      doc()?.cancelOverlay();
    },
  };
}

/* ------------------------------------------------------------ fill bucket */

function fill(): Tool {
  return {
    down(p) {
      const d = doc();
      if (!d || !paintable(d)) return;
      const src = paint.opts.sampleAll ? d.composite : d.activeCanvas!;
      const data = src.getContext('2d')!.getImageData(0, 0, d.width, d.height).data;
      const res = matchColor(data, d.width, d.height, p.x, p.y, paint.opts.tolerance, paint.opts.contiguous);
      let r = res.bounds;
      if (!r) return;
      if (d.selBounds) r = clipRect({ x: Math.max(r.x, d.selBounds.x), y: Math.max(r.y, d.selBounds.y), w: Math.min(r.x + r.w, d.selBounds.x + d.selBounds.w) - Math.max(r.x, d.selBounds.x), h: Math.min(r.y + r.h, d.selBounds.y + d.selBounds.h) - Math.max(r.y, d.selBounds.y) }, d.width, d.height);
      if (!r) return;
      const rgba = rgbaOf(colorFor(p));
      rgba[3] = Math.round(rgba[3] * paint.opts.opacity);
      const rect = r;
      d.editPixels(d.activeId, rect, 'Fill', (c) => {
        const img = c.getImageData(rect.x, rect.y, rect.w, rect.h);
        fillMasked(img.data, rect, d.width, res.mask, rgba, d.selection);
        c.putImageData(img, rect.x, rect.y);
      });
    },
    move() {},
    up() {},
  };
}

/* ------------------------------------------------------------ shapes */

function shapes(): Tool {
  let a: Pt | null = null;
  let prev: Rect | null = null;
  let button = 0;
  const draw = (b: Pt, shift: boolean) => {
    const d = doc();
    const ov = d?.overlay;
    if (!d || !ov || !a) return;
    const o = paint.opts;
    const kind = o.shape;
    const lw = Math.max(1, o.shapeWidth);
    let end = b;
    if (shift) end = kind === 'line' || kind === 'arrow' ? snapAngle(a, b) : squareBox(a, b);
    const c = ctxOf(ov.canvas);
    if (prev) c.clearRect(prev.x, prev.y, prev.w, prev.h);
    const stroke = button === 2 ? paint.secondary : paint.primary;
    const fillC = o.shapeStyle === 'both' ? (button === 2 ? paint.primary : paint.secondary) : stroke;
    c.save();
    c.lineWidth = lw;
    c.lineJoin = 'round';
    c.lineCap = 'round';
    c.strokeStyle = stroke;
    c.fillStyle = fillC;
    let box: Rect;
    if (kind === 'line') {
      c.beginPath();
      c.moveTo(a.x, a.y);
      c.lineTo(end.x, end.y);
      c.stroke();
      box = boxOf(a, end);
    } else if (kind === 'arrow') {
      const poly = arrowPolygon(a, end, lw);
      c.fillStyle = stroke;
      c.beginPath();
      poly.forEach((q, i) => (i ? c.lineTo(q.x, q.y) : c.moveTo(q.x, q.y)));
      c.closePath();
      c.fill();
      box = bboxOf(poly, 0) ?? boxOf(a, end);
    } else {
      const r = boxOf(a, end);
      box = r;
      c.beginPath();
      if (kind === 'rect') c.rect(r.x, r.y, r.w, r.h);
      else c.ellipse(r.x + r.w / 2, r.y + r.h / 2, r.w / 2, r.h / 2, 0, 0, Math.PI * 2);
      if (o.shapeStyle !== 'outline') c.fill();
      if (o.shapeStyle !== 'fill') c.stroke();
    }
    c.restore();
    const pad = lw + 2;
    const grown = { x: Math.floor(box.x - pad), y: Math.floor(box.y - pad), w: Math.ceil(box.w + 2 * pad) + 1, h: Math.ceil(box.h + 2 * pad) + 1 };
    const all = unionRect(prev, grown)!;
    d.clipToSelection(c, all);
    ov.rect = unionRect(ov.rect, grown);
    d.invalidate(all);
    prev = grown;
  };
  return {
    down(p) {
      const d = doc();
      if (!d || !paintable(d)) return;
      if (!d.beginOverlay(paint.opts.opacity, 'source-over')) return;
      a = { x: p.x, y: p.y };
      prev = null;
      button = p.button;
    },
    move(p) {
      if (a) draw(p, p.shift);
    },
    up(p) {
      if (!a) return;
      draw(p, p.shift);
      a = null;
      const names = { line: 'Line', arrow: 'Arrow', rect: 'Rectangle', ellipse: 'Ellipse' } as const;
      doc()?.endOverlay(names[paint.opts.shape]);
    },
    cancel() {
      a = null;
      doc()?.cancelOverlay();
    },
  };
}

/* ------------------------------------------------------------ selections */

function modeOf(p: PointerInfo): Combine {
  if (p.shift && p.alt) return 'intersect';
  if (p.shift) return 'add';
  if (p.alt) return 'subtract';
  return paint.opts.selectMode;
}

function applySelection(d: PaintDoc, next: Uint8Array, mode: Combine): void {
  d.commitFloating();
  d.setSelection(combine(mode === 'replace' ? null : d.selection, next, mode));
}

function marquee(): Tool {
  let a: Pt | null = null;
  let b: Pt | null = null;
  let mode: Combine = 'replace';
  let shift = false;
  const end = () => (a && b ? (shift && mode === 'replace' ? squareBox(a, b) : b) : null);
  return {
    down(p) {
      a = { x: Math.round(p.x), y: Math.round(p.y) };
      b = a;
      mode = modeOf(p);
      shift = p.shift;
    },
    move(p) {
      if (!a) return;
      b = { x: Math.round(p.x), y: Math.round(p.y) };
      shift = p.shift;
      doc()?.repaint();
    },
    up() {
      const d = doc();
      const e = end();
      if (!d || !a || !e) return;
      const r = boxOf(a, e);
      a = null;
      b = null;
      if (r.w < 2 && r.h < 2) {
        if (mode === 'replace') d.deselect();
        return;
      }
      const m = paint.opts.selectShape === 'ellipse' ? ellipseMask(d.width, d.height, r) : rectMask(d.width, d.height, r);
      applySelection(d, m, mode);
    },
    cancel() {
      a = null;
      b = null;
    },
    overlay(c, v) {
      const e = end();
      if (!a || !e) return;
      const r = boxOf(a, e);
      c.save();
      c.translate(v.x, v.y);
      c.scale(v.zoom, v.zoom);
      c.lineWidth = 1 / v.zoom;
      c.beginPath();
      if (paint.opts.selectShape === 'ellipse') c.ellipse(r.x + r.w / 2, r.y + r.h / 2, r.w / 2, r.h / 2, 0, 0, Math.PI * 2);
      else c.rect(r.x, r.y, r.w, r.h);
      c.fillStyle = 'rgba(80, 140, 255, 0.12)';
      c.fill();
      c.strokeStyle = '#fff';
      c.stroke();
      c.setLineDash([4 / v.zoom, 4 / v.zoom]);
      c.strokeStyle = '#000';
      c.stroke();
      c.restore();
    },
  };
}

function lasso(): Tool {
  let pts: Pt[] = [];
  let mode: Combine = 'replace';
  return {
    down(p) {
      pts = [{ x: p.x, y: p.y }];
      mode = modeOf(p);
    },
    move(_p, all) {
      if (!pts.length) return;
      for (const q of all) pts.push({ x: q.x, y: q.y });
      doc()?.repaint();
    },
    up() {
      const d = doc();
      const poly = pts;
      pts = [];
      if (!d) return;
      const box = bboxOf(poly, 0);
      if (!box || (box.w < 3 && box.h < 3)) {
        if (mode === 'replace') d.deselect();
        return;
      }
      applySelection(d, polygonMask(d.width, d.height, poly), mode);
    },
    cancel() {
      pts = [];
    },
    overlay(c, v) {
      if (pts.length < 2) return;
      c.save();
      c.beginPath();
      pts.forEach((q, i) => (i ? c.lineTo(v.x + q.x * v.zoom, v.y + q.y * v.zoom) : c.moveTo(v.x + q.x * v.zoom, v.y + q.y * v.zoom)));
      c.lineWidth = 1;
      c.strokeStyle = '#fff';
      c.stroke();
      c.setLineDash([4, 4]);
      c.strokeStyle = '#000';
      c.stroke();
      c.restore();
    },
  };
}

function wand(): Tool {
  return {
    down(p) {
      const d = doc();
      if (!d) return;
      const src = paint.opts.sampleAll || !d.activeCanvas ? d.composite : d.activeCanvas;
      const data = src.getContext('2d')!.getImageData(0, 0, d.width, d.height).data;
      const res = matchColor(data, d.width, d.height, p.x, p.y, paint.opts.tolerance, paint.opts.contiguous);
      if (!res.bounds) return;
      applySelection(d, res.mask, modeOf(p));
    },
    move() {},
    up() {},
  };
}

/* ------------------------------------------------------------ move */

function mover(): Tool {
  let start: Pt | null = null;
  let origin: Pt | null = null;
  return {
    down(p) {
      const d = doc();
      if (!d) return;
      let f = d.floating;
      if (!f) {
        if (!paintable(d)) return;
        if (!d.selection) d.setSelection(rectMask(d.width, d.height, { x: 0, y: 0, w: d.width, h: d.height }));
        f = d.lift();
      }
      if (!f) return;
      start = { x: p.x, y: p.y };
      origin = { x: f.x, y: f.y };
    },
    move(p) {
      const d = doc();
      if (!d || !start || !origin) return;
      let dx = p.x - start.x;
      let dy = p.y - start.y;
      if (p.shift) {
        if (Math.abs(dx) > Math.abs(dy)) dy = 0;
        else dx = 0;
      }
      d.moveFloating(origin.x + dx, origin.y + dy);
    },
    up() {
      start = null;
      origin = null;
    },
  };
}

/* ------------------------------------------------------------ hand */

function hand(): Tool {
  let s: { sx: number; sy: number; x: number; y: number } | null = null;
  return {
    down(p) {
      s = { sx: p.sx, sy: p.sy, x: paint.view.x, y: paint.view.y };
    },
    move(p) {
      if (!s) return;
      paint.view = { ...paint.view, x: s.x + p.sx - s.sx, y: s.y + p.sy - s.sy };
    },
    up() {
      s = null;
    },
  };
}

/* ------------------------------------------------------------ crop */

function crop(): Tool {
  let a: Pt | null = null;
  let drag: { start: Pt; rect: Rect } | null = null;
  const inside = (p: Pt, r: Rect) => p.x >= r.x && p.y >= r.y && p.x <= r.x + r.w && p.y <= r.y + r.h;
  return {
    down(p) {
      const d = doc();
      if (!d) return;
      const cur = paint.crop;
      if (cur && inside(p, cur)) drag = { start: { x: p.x, y: p.y }, rect: { ...cur } };
      else a = { x: Math.round(p.x), y: Math.round(p.y) };
    },
    move(p) {
      const d = doc();
      if (!d) return;
      if (drag) {
        const r = drag.rect;
        paint.crop = {
          x: Math.round(Math.max(0, Math.min(d.width - r.w, r.x + p.x - drag.start.x))),
          y: Math.round(Math.max(0, Math.min(d.height - r.h, r.y + p.y - drag.start.y))),
          w: r.w,
          h: r.h,
        };
      } else if (a) {
        const b = p.shift ? squareBox(a, p) : p;
        paint.crop = clipRect(boxOf(a, { x: Math.round(b.x), y: Math.round(b.y) }), d.width, d.height);
      }
      d.repaint();
    },
    up() {
      a = null;
      drag = null;
    },
    overlay(c, v) {
      const r = paint.crop;
      const d = doc();
      if (!r || !d) return;
      c.save();
      c.fillStyle = 'rgba(0,0,0,0.5)';
      c.beginPath();
      c.rect(v.x, v.y, d.width * v.zoom, d.height * v.zoom);
      c.rect(v.x + r.x * v.zoom, v.y + r.y * v.zoom, r.w * v.zoom, r.h * v.zoom);
      c.fill('evenodd');
      c.strokeStyle = '#fff';
      c.lineWidth = 1.5;
      c.strokeRect(v.x + r.x * v.zoom, v.y + r.y * v.zoom, r.w * v.zoom, r.h * v.zoom);
      // rule of thirds
      c.globalAlpha = 0.5;
      c.beginPath();
      for (const k of [1, 2]) {
        c.moveTo(v.x + (r.x + (r.w * k) / 3) * v.zoom, v.y + r.y * v.zoom);
        c.lineTo(v.x + (r.x + (r.w * k) / 3) * v.zoom, v.y + (r.y + r.h) * v.zoom);
        c.moveTo(v.x + r.x * v.zoom, v.y + (r.y + (r.h * k) / 3) * v.zoom);
        c.lineTo(v.x + (r.x + r.w) * v.zoom, v.y + (r.y + (r.h * k) / 3) * v.zoom);
      }
      c.lineWidth = 1;
      c.stroke();
      c.restore();
    },
  };
}

/* ------------------------------------------------------------ text */

/** The topmost live text layer under the point. */
function textAt(d: PaintDoc, p: Pt): string | null {
  for (let i = d.layers.length - 1; i >= 0; i--) {
    const l = d.layers[i];
    const t = l.text;
    if (!t || !l.visible) continue;
    if (p.x >= t.x - 6 && p.x <= t.x + t.w + 6 && p.y >= t.y - 6 && p.y <= t.y + t.h + 6) return l.id;
  }
  return null;
}

export function beginTextEdit(id: string): void {
  const d = doc();
  const l = d?.layer(id);
  if (!d || !l?.text) return;
  if (paint.textEdit && paint.textEdit.layerId !== id) finishTextEdit();
  d.activeId = id;
  paint.textEdit = { layerId: id, before: $state.snapshot(l.text) as TextElement };
  d.repaint();
}

/** End text editing: one undo step (an empty text removes its layer). */
export function finishTextEdit(): void {
  const d = doc();
  const t = paint.textEdit;
  paint.textEdit = null;
  if (!d || !t) return;
  const l = d.layer(t.layerId);
  if (!l?.text) return;
  if (!l.text.text.trim()) {
    if (d.layers.length > 1) d.deleteLayer(l.id);
  } else d.commitText(l.id, t.before);
  d.repaint();
}

/** A text element in the current style at image point p. */
export function newTextElement(d: PaintDoc, p: Pt, text = 'Your text'): TextElement {
  const preset = textPreset(paint.opts.textPreset) ?? textPreset('heading')!;
  const size = paint.opts.textSize;
  const el = createText({ x: p.x, y: p.y, w: 100, ...presetElement(preset, size, Math.min(d.width, d.height)), text, align: 'left' });
  if (preset.id !== 'meme') el.fill = paint.primary.slice(0, 7);
  el.w = Math.ceil(naturalTextWidth(el));
  el.y = Math.round(p.y - size * 0.6);
  return el;
}

function textTool(): Tool {
  let drag: { start: Pt; x: number; y: number; id: string } | null = null;
  return {
    down(p) {
      const d = doc();
      if (!d) return;
      const hit = textAt(d, p);
      const editing = paint.textEdit?.layerId;
      if (hit) {
        if (hit !== editing) beginTextEdit(hit);
        const t = d.layer(hit)!.text!;
        drag = { start: { x: p.x, y: p.y }, x: t.x, y: t.y, id: hit };
        return;
      }
      if (editing) {
        finishTextEdit();
        return;
      }
      // Measure with the real font, so the box fits the words.
      const preset = textPreset(paint.opts.textPreset) ?? textPreset('heading')!;
      const at = { x: p.x, y: p.y };
      void ensureFont(preset.look.font, preset.look.weight ?? 400).then(() => {
        if (paint.doc !== d) return;
        const l = d.addTextLayer(newTextElement(d, at), 'Text');
        beginTextEdit(l.id);
        // the editor's text field selects "Your text" so typing replaces it
        paint.textEdit = { layerId: l.id, before: null };
      });
    },
    move(p) {
      const d = doc();
      if (!d || !drag) return;
      const l = d.layer(drag.id);
      if (!l?.text) return;
      d.setText(drag.id, { ...($state.snapshot(l.text) as TextElement), x: Math.round(drag.x + p.x - drag.start.x), y: Math.round(drag.y + p.y - drag.start.y) });
    },
    up() {
      drag = null;
    },
    overlay(c, v) {
      const d = doc();
      if (!d) return;
      for (const l of d.layers) {
        const t = l.text;
        if (!t || !l.visible) continue;
        const on = paint.textEdit?.layerId === l.id;
        c.save();
        c.strokeStyle = on ? '#ff5d8f' : 'rgba(127,127,127,0.7)';
        c.lineWidth = on ? 1.5 : 1;
        c.setLineDash(on ? [] : [4, 3]);
        c.strokeRect(v.x + (t.x - 4) * v.zoom, v.y + (t.y - 4) * v.zoom, (t.w + 8) * v.zoom, (t.h + 8) * v.zoom);
        c.restore();
      }
    },
  };
}

/* ------------------------------------------------------------ retouch: blur, smudge, clone */

let filterOk: boolean | null = null;
/** Canvas `filter` (blur) isn't available in every WebKit; feature-detect it once. */
function canFilter(): boolean {
  if (filterOk === null) {
    try {
      const c = newCanvas(1, 1).getContext('2d')!;
      c.filter = 'blur(2px)';
      filterOk = c.filter === 'blur(2px)';
    } catch {
      filterOk = false;
    }
  }
  return filterOk;
}

/** Two passes of a separable box blur on RGBA (fallback for `filter: blur`). */
function boxBlur(d: Uint8ClampedArray, w: number, h: number, r: number): void {
  if (r < 1) return;
  const tmp = new Float32Array(d.length);
  for (let pass = 0; pass < 2; pass++) {
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++)
        for (let c = 0; c < 4; c++) {
          let s = 0;
          let n = 0;
          for (let k = -r; k <= r; k++) {
            const xx = x + k;
            if (xx < 0 || xx >= w) continue;
            s += d[(y * w + xx) * 4 + c];
            n++;
          }
          tmp[(y * w + x) * 4 + c] = s / n;
        }
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++)
        for (let c = 0; c < 4; c++) {
          let s = 0;
          let n = 0;
          for (let k = -r; k <= r; k++) {
            const yy = y + k;
            if (yy < 0 || yy >= h) continue;
            s += tmp[(yy * w + x) * 4 + c];
            n++;
          }
          d[(y * w + x) * 4 + c] = s / n;
        }
  }
}

function soft(r: number): HTMLCanvasElement {
  const s = Math.max(2, Math.ceil(r * 2));
  const c = newCanvas(s, s);
  const g = ctxOf(c);
  const grad = g.createRadialGradient(s / 2, s / 2, 0, s / 2, s / 2, s / 2);
  grad.addColorStop(0, 'rgba(0,0,0,1)');
  grad.addColorStop(0.55, 'rgba(0,0,0,0.85)');
  grad.addColorStop(1, 'rgba(0,0,0,0)');
  g.fillStyle = grad;
  g.fillRect(0, 0, s, s);
  return c;
}

function retouch(kind: 'blur' | 'smudge' | 'clone'): Tool {
  let ctx: CanvasRenderingContext2D | null = null;
  let last: Pt | null = null;
  let tip: HTMLCanvasElement | null = null;
  let buf: HTMLCanvasElement | null = null;
  let source: HTMLCanvasElement | null = null;
  let cloneFrom: Pt | null = null;
  let offset: Pt | null = null;
  let hoverAt: Pt | null = null;

  const dab = (d: PaintDoc, at: Pt) => {
    const size = paint.size;
    const r = size / 2;
    const x = Math.round(at.x - r);
    const y = Math.round(at.y - r);
    const s = tip!.width;
    d.directTouch({ x, y, w: s, h: s });
    const piece = newCanvas(s, s);
    const pc = ctxOf(piece);
    const strength = paint.opts.strength;
    if (kind === 'blur') {
      const radius = Math.max(1, (size / 8) * strength);
      if (canFilter()) {
        pc.filter = `blur(${radius}px)`;
        pc.drawImage(ctx!.canvas, x, y, s, s, 0, 0, s, s);
        pc.filter = 'none';
      } else {
        pc.drawImage(ctx!.canvas, x, y, s, s, 0, 0, s, s);
        const img = pc.getImageData(0, 0, s, s);
        boxBlur(img.data, s, s, Math.round(radius));
        pc.putImageData(img, 0, 0);
      }
    } else if (kind === 'smudge') {
      pc.drawImage(buf!, 0, 0);
    } else {
      pc.drawImage(source!, x + offset!.x, y + offset!.y, s, s, 0, 0, s, s);
    }
    pc.globalCompositeOperation = 'destination-in';
    pc.drawImage(tip!, 0, 0);
    if (d.selCanvas) pc.drawImage(d.selCanvas, x, y, s, s, 0, 0, s, s);
    ctx!.save();
    ctx!.globalAlpha = kind === 'clone' ? paint.opts.opacity : kind === 'smudge' ? strength : 1;
    ctx!.drawImage(piece, x, y);
    ctx!.restore();
    if (kind === 'smudge') {
      // pick up some of what's under the brush now
      const bc = ctxOf(buf!);
      bc.globalAlpha = 1 - strength * 0.9;
      bc.drawImage(ctx!.canvas, x, y, s, s, 0, 0, s, s);
      bc.globalAlpha = 1;
    }
    d.invalidate({ x, y, w: s, h: s });
  };

  const along = (d: PaintDoc, a: Pt, b: Pt) => {
    const step = Math.max(1, paint.size * 0.2);
    const dist = Math.hypot(b.x - a.x, b.y - a.y);
    const n = Math.max(1, Math.floor(dist / step));
    for (let i = 1; i <= n; i++) dab(d, { x: a.x + ((b.x - a.x) * i) / n, y: a.y + ((b.y - a.y) * i) / n });
  };

  return {
    down(p) {
      const d = doc();
      if (!d) return;
      if (kind === 'clone' && p.alt) {
        cloneFrom = { x: p.x, y: p.y };
        offset = null;
        toasts.push({ kind: 'info', title: 'Clone source set', message: 'Now paint where the copy should go.' });
        return;
      }
      if (!paintable(d)) return;
      if (kind === 'clone' && !cloneFrom) {
        toasts.push({ kind: 'info', title: 'Choose what to copy first', message: 'Alt+click the spot to copy from, then paint.' });
        return;
      }
      const s = d.beginDirect();
      if (!s) return;
      ctx = s.ctx;
      tip = soft(paint.size / 2);
      if (kind === 'clone') {
        source = cloneCanvas(ctx.canvas);
        offset ??= { x: cloneFrom!.x - p.x, y: cloneFrom!.y - p.y };
      }
      if (kind === 'smudge') {
        buf = newCanvas(tip.width, tip.height);
        ctxOf(buf).drawImage(ctx.canvas, Math.round(p.x - paint.size / 2), Math.round(p.y - paint.size / 2), tip.width, tip.height, 0, 0, tip.width, tip.height);
      }
      last = { x: p.x, y: p.y };
      if (kind !== 'smudge') dab(d, last);
    },
    move(p, all) {
      const d = doc();
      hoverAt = { x: p.x, y: p.y };
      if (!d || !ctx || !last) {
        if (kind === 'clone' && d) d.repaint();
        return;
      }
      for (const q of all) {
        along(d, last, q);
        last = { x: q.x, y: q.y };
      }
    },
    hover(p) {
      hoverAt = { x: p.x, y: p.y };
    },
    up() {
      const d = doc();
      if (!ctx || !d) return;
      ctx = null;
      last = null;
      source = null;
      buf = null;
      d.endDirect(kind === 'blur' ? 'Blur' : kind === 'smudge' ? 'Smudge' : 'Clone');
    },
    cancel() {
      if (ctx) doc()?.endDirect('Retouch');
      ctx = null;
      last = null;
    },
    overlay(c, v) {
      if (kind !== 'clone' || !cloneFrom) return;
      const at = offset && hoverAt ? { x: hoverAt.x + offset.x, y: hoverAt.y + offset.y } : cloneFrom;
      const sx = v.x + at.x * v.zoom;
      const sy = v.y + at.y * v.zoom;
      c.save();
      c.strokeStyle = '#fff';
      c.lineWidth = 1.5;
      c.beginPath();
      c.moveTo(sx - 7, sy);
      c.lineTo(sx + 7, sy);
      c.moveTo(sx, sy - 7);
      c.lineTo(sx, sy + 7);
      c.stroke();
      c.strokeStyle = '#000';
      c.lineWidth = 0.75;
      c.stroke();
      c.restore();
    },
  };
}

/* ------------------------------------------------------------ picker */

function picker(): Tool {
  let on = false;
  return {
    down(p) {
      on = true;
      pickColor(p);
    },
    move(p) {
      if (on) pickColor(p);
    },
    up() {
      on = false;
    },
  };
}

const made = new Map<ToolId, Tool>();

export function toolFor(id: ToolId): Tool {
  let t = made.get(id);
  if (!t) {
    t =
      id === 'brush' || id === 'eraser'
        ? brush(id)
        : id === 'pencil'
          ? pencil()
          : id === 'fill'
            ? fill()
            : id === 'picker'
              ? picker()
              : id === 'shape'
                ? shapes()
                : id === 'select'
                  ? marquee()
                  : id === 'lasso'
                    ? lasso()
                    : id === 'wand'
                      ? wand()
                      : id === 'move'
                        ? mover()
                        : id === 'hand'
                          ? hand()
                          : id === 'crop'
                            ? crop()
                            : id === 'text'
                              ? textTool()
                              : retouch(id);
    made.set(id, t);
  }
  return t;
}

/** Tools that paint with a round brush (cursor outline, [ ] sizes). */
export const SIZED: ToolId[] = ['brush', 'pencil', 'eraser', 'blur', 'smudge', 'clone'];
/** Tools where Alt+click picks a colour instead. */
export const ALT_PICKS: ToolId[] = ['brush', 'pencil', 'fill', 'shape'];
