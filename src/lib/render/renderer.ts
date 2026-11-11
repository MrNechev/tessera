/**
 * The one and only collage renderer. The editor uses it with low-resolution
 * proxies; the exporter uses it, tile by tile, with full-resolution photos.
 */
import type {
  Background,
  CollageDoc,
  CollageElement,
  Crop,
  PhotoElement,
  Rect,
  Shadow,
  ShapeElement,
  StickerElement,
} from '../model/types';
import { AdjustCache, hasPixelAdjust } from './adjust';
import { paintFor, withAlpha } from './fills';
import { captionArea, photoWindow } from './frameGeometry';
import { drawPageFrame } from './pageFrames';
import { ctx2d, makeCanvas, patternTile, textureTile, type AnyCtx } from './patterns';
import { maskPath, shapePath } from './shapes';
import { drawText } from './text';
import { drawCalendar } from './calendar';
import { cssFont, ensureFont } from '../assets/fonts';

export interface ImageEntry {
  source: CanvasImageSource;
  /** Pixel size of `source`. */
  width: number;
  height: number;
  /** Stable cache key for derived bitmaps (e.g. "asset@proxy"). */
  key: string;
}

export interface RenderResources {
  /** `neededPx`: largest on-screen dimension the photo is drawn at (device px). */
  image(assetId: string, neededPx?: number): ImageEntry | null;
  sticker(id: string): CanvasImageSource | null;
  adjustCache: AdjustCache;
}

export interface RenderOptions {
  /** Device pixels per page unit. */
  scale: number;
  resources: RenderResources;
  /** Editor mode: draw empty frames as drop targets. */
  placeholders: boolean;
  /** Elements to skip (e.g. text being edited in place). */
  hidden?: ReadonlySet<string>;
  /** Restrict drawing to this page-space rectangle (export tiles). */
  clip?: Rect;
}

function shortSide(doc: CollageDoc): number {
  return Math.min(doc.page.width, doc.page.height);
}

export function applyShadow(c: AnyCtx, s: Shadow | null | undefined, scale: number, pageShort: number): void {
  if (!s) return;
  c.shadowColor = s.color;
  c.shadowBlur = s.blur * pageShort * scale;
  c.shadowOffsetX = s.x * pageShort * scale;
  c.shadowOffsetY = s.y * pageShort * scale;
}

function clearShadow(c: AnyCtx): void {
  c.shadowColor = 'transparent';
  c.shadowBlur = 0;
  c.shadowOffsetX = 0;
  c.shadowOffsetY = 0;
}

/* ------------------------------------------------------------- background */

export function drawBackground(c: AnyCtx, doc: CollageDoc, opts: RenderOptions): void {
  const { width: W, height: H } = doc.page;
  const bg: Background = doc.background;
  const S = shortSide(doc);
  c.save();
  c.fillStyle = paintFor(c, bg.fill, 0, 0, W, H);
  c.fillRect(0, 0, W, H);

  if (bg.image) {
    const img = opts.resources.image(bg.image.asset, Math.max(W, H) * opts.scale);
    if (img) {
      c.save();
      c.globalAlpha = bg.image.opacity;
      const blur = bg.image.blur;
      if (blur > 0) {
        // Cheap, resolution-independent blur: draw tiny, scale up smoothly.
        const factor = Math.max(4, Math.round(4 + blur * 60));
        const tw = Math.max(2, Math.round(W / factor / 4));
        const th = Math.max(2, Math.round(H / factor / 4));
        const tmp = makeCanvas(tw, th);
        const tc = ctx2d(tmp);
        tc.imageSmoothingQuality = 'high';
        drawCover(tc, img, { x: 0, y: 0, w: tw, h: th }, { x: 0.5, y: 0.5, zoom: 1 });
        c.imageSmoothingEnabled = true;
        c.imageSmoothingQuality = 'high';
        c.drawImage(tmp as CanvasImageSource, -W * 0.02, -H * 0.02, W * 1.04, H * 1.04);
      } else {
        drawCover(c, img, { x: 0, y: 0, w: W, h: H }, { x: 0.5, y: 0.5, zoom: 1 });
      }
      c.restore();
      if (bg.image.dim > 0) {
        c.fillStyle = `rgba(0,0,0,${bg.image.dim})`;
        c.fillRect(0, 0, W, H);
      }
    }
  }

  if (bg.pattern && bg.pattern.opacity > 0) {
    const tilePage = Math.max(4, bg.pattern.scale * S);
    const res = Math.min(512, Math.max(16, tilePage * opts.scale));
    const tile = patternTile(bg.pattern.id, bg.pattern.color, res);
    const pattern = c.createPattern(tile as CanvasImageSource, 'repeat');
    if (pattern) {
      const k = tilePage / tile.width;
      pattern.setTransform(new DOMMatrix().scaleSelf(k, k));
      c.globalAlpha = bg.pattern.opacity;
      c.fillStyle = pattern;
      c.fillRect(0, 0, W, H);
      c.globalAlpha = 1;
    }
  }

  if (bg.texture && bg.texture.intensity > 0) {
    const tile = textureTile(bg.texture.id);
    const pattern = c.createPattern(tile as CanvasImageSource, 'repeat');
    if (pattern) {
      const k = (S * 0.35) / tile.width;
      pattern.setTransform(new DOMMatrix().scaleSelf(k, k));
      c.globalCompositeOperation = 'multiply';
      c.globalAlpha = bg.texture.intensity;
      c.fillStyle = pattern;
      c.fillRect(0, 0, W, H);
      c.globalAlpha = 1;
      c.globalCompositeOperation = 'source-over';
    }
  }

  if (bg.vignette > 0) {
    const g = c.createRadialGradient(W / 2, H / 2, Math.min(W, H) * 0.3, W / 2, H / 2, Math.hypot(W, H) / 2);
    g.addColorStop(0, 'rgba(0,0,0,0)');
    g.addColorStop(1, `rgba(0,0,0,${0.65 * bg.vignette})`);
    c.fillStyle = g;
    c.fillRect(0, 0, W, H);
  }
  c.restore();
}

/* ------------------------------------------------------------------ photos */

export function coverRect(sw: number, sh: number, win: Rect, crop: Crop): Rect {
  const s = Math.max(win.w / sw, win.h / sh) * Math.max(1, crop.zoom);
  const dw = sw * s;
  const dh = sh * s;
  return { x: win.x - (dw - win.w) * crop.x, y: win.y - (dh - win.h) * crop.y, w: dw, h: dh };
}

function drawCover(c: AnyCtx, img: ImageEntry, win: Rect, crop: Crop, source: CanvasImageSource = img.source): void {
  const r = coverRect(img.width, img.height, win, crop);
  c.drawImage(source, r.x, r.y, r.w, r.h);
}

function drawPlaceholder(c: AnyCtx, win: Rect, editor: boolean): void {
  c.fillStyle = editor ? '#e6e2ee' : '#dcd8e3';
  c.fillRect(win.x, win.y, win.w, win.h);
  if (!editor) return;
  const s = Math.min(win.w, win.h) * 0.26;
  const cx = win.x + win.w / 2;
  const cy = win.y + win.h / 2;
  c.fillStyle = '#b4adc6';
  c.beginPath();
  c.moveTo(cx - s, cy + s * 0.6);
  c.lineTo(cx - s * 0.25, cy - s * 0.35);
  c.lineTo(cx + s * 0.2, cy + s * 0.15);
  c.lineTo(cx + s * 0.45, cy - s * 0.1);
  c.lineTo(cx + s, cy + s * 0.6);
  c.closePath();
  c.fill();
  c.beginPath();
  c.arc(cx + s * 0.45, cy - s * 0.55, s * 0.2, 0, Math.PI * 2);
  c.fill();
}

function drawPhoto(c: AnyCtx, el: PhotoElement, doc: CollageDoc, opts: RenderOptions): void {
  const S = shortSide(doc);
  const { w, h } = el;
  const frame = el.frame && el.frame.style !== 'none' ? el.frame : null;
  const win = photoWindow(frame, w, h);
  const mask = maskPath(el.mask, win.w, win.h, win.x, win.y);

  // Frame body or drop shadow.
  if (frame && frame.style !== 'tape') {
    c.save();
    applyShadow(c, el.shadow, opts.scale, S);
    c.fillStyle = frame.color;
    c.beginPath();
    if (frame.style === 'instant') c.roundRect(0, 0, w, h, Math.min(w, h) * 0.025);
    else c.rect(0, 0, w, h);
    c.fill();
    c.restore();
    if (frame.style === 'polaroid' || frame.style === 'instant') {
      // A whisper of paper shading.
      const g = c.createLinearGradient(0, 0, 0, h);
      g.addColorStop(0, 'rgba(255,255,255,0.0)');
      g.addColorStop(1, 'rgba(0,0,0,0.045)');
      c.fillStyle = g;
      c.fillRect(0, 0, w, h);
    }
    if (frame.style === 'mat') {
      c.strokeStyle = 'rgba(0,0,0,0.18)';
      c.lineWidth = Math.max(1, Math.min(w, h) * 0.004);
      c.strokeRect(win.x - c.lineWidth * 2, win.y - c.lineWidth * 2, win.w + c.lineWidth * 4, win.h + c.lineWidth * 4);
    }
  } else if (el.shadow) {
    c.save();
    applyShadow(c, el.shadow, opts.scale, S);
    c.fillStyle = '#808080';
    c.fill(mask);
    c.restore();
  }

  // The photo itself.
  c.save();
  c.clip(mask);
  const need = Math.max(win.w, win.h) * Math.max(1, el.crop.zoom) * opts.scale * 1.1;
  const img = el.asset ? opts.resources.image(el.asset, need) : null;
  if (img) {
    let source: CanvasImageSource = img.source;
    if (hasPixelAdjust(el.adjust)) {
      source = opts.resources.adjustCache.get(img.key, img.source, img.width, img.height, el.adjust) as CanvasImageSource;
    }
    c.imageSmoothingEnabled = true;
    c.imageSmoothingQuality = 'high';
    if (el.flipX || el.flipY) {
      c.translate(win.x + win.w / 2, win.y + win.h / 2);
      c.scale(el.flipX ? -1 : 1, el.flipY ? -1 : 1);
      c.translate(-(win.x + win.w / 2), -(win.y + win.h / 2));
    }
    drawCover(c, img, win, el.crop, source);
  } else {
    drawPlaceholder(c, win, opts.placeholders);
  }
  if (el.adjust && el.adjust.vignette > 0) {
    const g = c.createRadialGradient(
      win.x + win.w / 2,
      win.y + win.h / 2,
      Math.min(win.w, win.h) * 0.25,
      win.x + win.w / 2,
      win.y + win.h / 2,
      Math.hypot(win.w, win.h) / 2,
    );
    g.addColorStop(0, 'rgba(0,0,0,0)');
    g.addColorStop(1, `rgba(0,0,0,${0.7 * el.adjust.vignette})`);
    c.fillStyle = g;
    c.fillRect(win.x, win.y, win.w, win.h);
  }
  c.restore();

  if (el.border && el.border.width > 0) {
    c.save();
    c.clip(mask);
    c.strokeStyle = el.border.color;
    c.lineWidth = el.border.width * 2;
    c.stroke(mask);
    c.restore();
  }

  if (frame?.style === 'film') {
    const band = win.y;
    const hole = band * 0.42;
    const step = hole * 1.9;
    c.fillStyle = 'rgba(255,255,255,0.9)';
    for (let x = step / 2; x < w; x += step) {
      for (const y of [band / 2, h - band / 2]) {
        c.beginPath();
        c.roundRect(x - hole * 0.35, y - hole / 2, hole * 0.7, hole, hole * 0.15);
        c.fill();
      }
    }
  }

  if (frame?.style === 'tape') {
    const tl = Math.min(w, h) * 0.36;
    const tt = Math.min(w, h) * 0.1;
    c.save();
    c.fillStyle = withAlpha(frame.color, 0.78);
    for (const [x, y, a] of [
      [0, 0, -Math.PI / 4.2],
      [w, 0, Math.PI / 4.2],
    ] as const) {
      c.save();
      c.translate(x, y);
      c.rotate(a);
      c.fillRect(-tl / 2, -tt / 2, tl, tt);
      c.restore();
    }
    c.restore();
  }

  const cap = captionArea(frame, w, h);
  if (cap && frame?.caption) {
    const font = frame.captionFont ?? 'caveat';
    void ensureFont(font, 400);
    c.save();
    let size = cap.h * 0.46;
    c.font = cssFont(font, 400, size);
    const tw = c.measureText(frame.caption).width;
    if (tw > cap.w * 0.92) {
      size *= (cap.w * 0.92) / tw;
      c.font = cssFont(font, 400, size);
    }
    c.fillStyle = frame.captionColor ?? '#2b2733';
    c.textAlign = 'center';
    c.textBaseline = 'middle';
    c.fillText(frame.caption, cap.x + cap.w / 2, cap.y + cap.h * 0.48);
    c.restore();
  }
}

/* -------------------------------------------------------- shapes, stickers */

function drawShape(c: AnyCtx, el: ShapeElement, doc: CollageDoc, opts: RenderOptions): void {
  const S = shortSide(doc);
  const path = shapePath(el.shape, el.w, el.h, el.radius);
  c.save();
  applyShadow(c, el.shadow, opts.scale, S);
  if (el.shape === 'ring') {
    if (el.stroke) {
      c.strokeStyle = el.stroke.color;
      c.lineWidth = el.stroke.width;
      const inset = el.stroke.width / 2;
      c.stroke(shapePath('circle', el.w - 2 * inset, el.h - 2 * inset, 0, inset, inset));
    }
    c.restore();
    return;
  }
  if (el.fill) {
    c.fillStyle = paintFor(c, el.fill, 0, 0, el.w, el.h);
    c.fill(path);
  }
  c.restore();
  if (el.stroke && el.stroke.width > 0) {
    c.save();
    if (!el.fill) applyShadow(c, el.shadow, opts.scale, S);
    c.strokeStyle = el.stroke.color;
    c.lineWidth = el.stroke.width;
    c.lineJoin = 'round';
    c.stroke(path);
    c.restore();
  }
}

function drawSticker(c: AnyCtx, el: StickerElement, doc: CollageDoc, opts: RenderOptions): void {
  const img = opts.resources.sticker(el.sticker);
  if (!img) {
    if (opts.placeholders) {
      c.fillStyle = 'rgba(160,150,190,0.25)';
      c.fillRect(0, 0, el.w, el.h);
    }
    return;
  }
  c.save();
  applyShadow(c, el.shadow, opts.scale, shortSide(doc));
  c.imageSmoothingEnabled = true;
  c.imageSmoothingQuality = 'high';
  c.drawImage(img, 0, 0, el.w, el.h);
  c.restore();
}

/* --------------------------------------------------------------- elements */

export function elementCenter(el: CollageElement): { x: number; y: number } {
  return { x: el.x + el.w / 2, y: el.y + el.h / 2 };
}

/** Set up the element's local coordinate system: origin at its top-left, unrotated. */
export function enterElement(c: AnyCtx, el: CollageElement): void {
  const { x, y } = elementCenter(el);
  c.translate(x, y);
  if (el.rotation) c.rotate((el.rotation * Math.PI) / 180);
  if (el.kind !== 'photo' && (el.flipX || el.flipY)) c.scale(el.flipX ? -1 : 1, el.flipY ? -1 : 1);
  c.translate(-el.w / 2, -el.h / 2);
}

function intersects(a: Rect, el: CollageElement, pageShort: number): boolean {
  // Conservative: rotated elements fit inside their circumscribed circle,
  // plus room for frames/glows and however far the shadow reaches.
  const cx = el.x + el.w / 2;
  const cy = el.y + el.h / 2;
  const sh = el.shadow ? (el.shadow.blur * 1.5 + Math.abs(el.shadow.x) + Math.abs(el.shadow.y)) * pageShort : 0;
  const r = Math.hypot(el.w, el.h) / 2 + Math.max(el.w, el.h) * 0.3 + sh;
  return cx + r >= a.x && cx - r <= a.x + a.w && cy + r >= a.y && cy - r <= a.y + a.h;
}

export function drawElement(c: AnyCtx, el: CollageElement, doc: CollageDoc, opts: RenderOptions): void {
  c.save();
  enterElement(c, el);
  c.globalAlpha *= el.opacity;
  switch (el.kind) {
    case 'photo':
      drawPhoto(c, el, doc, opts);
      break;
    case 'text':
      drawText(c, el, { scale: opts.scale, pageShort: shortSide(doc) });
      break;
    case 'sticker':
      drawSticker(c, el, doc, opts);
      break;
    case 'shape':
      drawShape(c, el, doc, opts);
      break;
    case 'calendar':
      applyShadow(c, el.shadow, opts.scale, shortSide(doc));
      drawCalendar(c, el);
      break;
  }
  c.restore();
  clearShadow(c);
}

/**
 * Render the full document. The caller is responsible for the base transform
 * (viewport pan, export tile offset); this function multiplies in `opts.scale`.
 */
export function renderDocument(c: AnyCtx, doc: CollageDoc, opts: RenderOptions): void {
  const { width: W, height: H } = doc.page;
  c.save();
  c.scale(opts.scale, opts.scale);
  c.beginPath();
  c.rect(0, 0, W, H);
  c.clip();
  drawBackground(c, doc, opts);
  const S = shortSide(doc);
  for (const el of doc.elements) {
    if (el.hidden || opts.hidden?.has(el.id)) continue;
    if (opts.clip && !intersects(opts.clip, el, S)) continue;
    drawElement(c, el, doc, opts);
  }
  if (doc.frame) drawPageFrame(c, doc.frame, W, H);
  c.restore();
}
