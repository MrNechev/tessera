/** Text layout and drawing (shared by the editor preview and the exporter). */
import { cssFont, ensureFont, isFontLoaded } from '../assets/fonts';
import type { TextElement } from '../model/types';
import { makeCanvas, ctx2d, type AnyCtx } from './patterns';

let measureCtx: AnyCtx | null = null;
function mctx(): AnyCtx {
  if (!measureCtx) measureCtx = ctx2d(makeCanvas(8, 8));
  return measureCtx;
}

export interface TextLayout {
  lines: string[];
  widths: number[];
  lineHeight: number;
  /** Width of the widest line. */
  contentWidth: number;
  /** Height of the text block (without background padding). */
  contentHeight: number;
  /** Padding applied by the background pill. */
  pad: number;
  /** Total element height. */
  height: number;
  /** For curved text: arc radius (0 if straight). */
  radius: number;
}

function displayText(el: TextElement): string {
  return el.uppercase ? el.text.toUpperCase() : el.text;
}

export function measureWidth(c: AnyCtx, text: string, spacing: number): number {
  return c.measureText(text).width + spacing * Math.max(0, [...text].length - 1);
}

/** Lay out a text element. Line breaking depends only on the model, never on render scale. */
export function layoutText(el: TextElement): TextLayout {
  const c = mctx();
  c.font = cssFont(el.font, el.weight, el.size, el.italic);
  const spacing = el.letterSpacing * el.size;
  const pad = el.background ? el.background.padding * el.size : 0;
  const lineHeight = el.size * el.lineHeight;
  const text = displayText(el);

  if (el.curve !== 0) {
    const line = text.replace(/\s*\n\s*/g, ' ');
    const width = measureWidth(c, line, spacing);
    const theta = Math.abs(el.curve) * Math.PI * 1.1;
    const radius = width / Math.max(theta, 1e-3);
    const sag = radius * (1 - Math.cos(Math.min(theta, Math.PI * 1.9) / 2));
    const contentHeight = lineHeight + sag + el.size * 0.15;
    return { lines: [line], widths: [width], lineHeight, contentWidth: width, contentHeight, pad, height: contentHeight + 2 * pad, radius };
  }

  const maxW = Math.max(el.size * 0.6, el.w - 2 * pad);
  const lines: string[] = [];
  for (const para of text.split('\n')) {
    const words = para.split(/(\s+)/).filter((t) => t.length > 0);
    let cur = '';
    for (const word of words) {
      const candidate = cur + word;
      if (cur.trim().length > 0 && measureWidth(c, candidate.trimEnd(), spacing) > maxW && word.trim().length > 0) {
        lines.push(cur.trimEnd());
        cur = word.trimStart();
      } else {
        cur = candidate;
      }
    }
    lines.push(cur.trimEnd());
  }
  const widths = lines.map((l) => measureWidth(c, l, spacing));
  const contentWidth = Math.max(0, ...widths);
  const contentHeight = lines.length * lineHeight;
  return { lines, widths, lineHeight, contentWidth, contentHeight, pad, height: contentHeight + 2 * pad, radius: 0 };
}

/** Draw a line with manual letter spacing while preserving kerning (prefix measurement). */
function drawSpaced(c: AnyCtx, line: string, x: number, y: number, spacing: number, mode: 'fill' | 'stroke') {
  if (spacing === 0) {
    if (mode === 'fill') c.fillText(line, x, y);
    else c.strokeText(line, x, y);
    return;
  }
  const chars = [...line];
  let prefix = '';
  for (let i = 0; i < chars.length; i++) {
    const cx = x + c.measureText(prefix).width + spacing * i;
    if (mode === 'fill') c.fillText(chars[i], cx, y);
    else c.strokeText(chars[i], cx, y);
    prefix += chars[i];
  }
}

export interface TextDrawOptions {
  /** Device pixels per page unit (for shadow sizes, which canvas doesn't transform). */
  scale: number;
  pageShort: number;
}

/** Draw `el` in its local box (0,0)-(w,h). Caller sets up the transform. */
export function drawText(c: AnyCtx, el: TextElement, opts: TextDrawOptions): void {
  void ensureFont(el.font, el.weight);
  const L = layoutText(el);
  const w = el.w;
  const h = L.height;

  if (el.background) {
    c.save();
    c.fillStyle = el.background.color;
    const r = Math.min(el.background.radius * el.size, h / 2, w / 2);
    c.beginPath();
    c.roundRect(0, 0, w, h, r);
    c.fill();
    c.restore();
  }

  c.save();
  c.font = cssFont(el.font, el.weight, el.size, el.italic);
  c.textBaseline = 'middle';
  c.textAlign = 'left';
  c.lineJoin = 'round';
  c.miterLimit = 2;
  const spacing = el.letterSpacing * el.size;
  const setShadow = (on: boolean) => {
    if (on && el.shadow) {
      c.shadowColor = el.shadow.color;
      c.shadowBlur = el.shadow.blur * opts.pageShort * opts.scale;
      c.shadowOffsetX = el.shadow.x * opts.pageShort * opts.scale;
      c.shadowOffsetY = el.shadow.y * opts.pageShort * opts.scale;
    } else {
      c.shadowColor = 'transparent';
      c.shadowBlur = 0;
      c.shadowOffsetX = 0;
      c.shadowOffsetY = 0;
    }
  };

  if (L.radius > 0) {
    drawCurved(c, el, L, w, h, spacing, setShadow);
    c.restore();
    return;
  }

  const passes: ('stroke' | 'fill')[] = el.stroke && el.stroke.width > 0 ? ['stroke', 'fill'] : ['fill'];
  for (const pass of passes) {
    setShadow(pass === passes[0]);
    if (pass === 'stroke') {
      c.strokeStyle = el.stroke!.color;
      c.lineWidth = el.stroke!.width * 2;
    } else c.fillStyle = el.fill;
    L.lines.forEach((line, i) => {
      const lw = L.widths[i];
      const x = el.align === 'left' ? L.pad : el.align === 'right' ? w - L.pad - lw : (w - lw) / 2;
      const y = L.pad + (i + 0.5) * L.lineHeight;
      drawSpaced(c, line, x, y, spacing, pass);
    });
  }
  c.restore();
}

function drawCurved(
  c: AnyCtx,
  el: TextElement,
  L: TextLayout,
  w: number,
  h: number,
  spacing: number,
  setShadow: (on: boolean) => void,
) {
  const line = L.lines[0];
  const R = L.radius;
  const arch = el.curve > 0;
  const total = L.widths[0];
  const chars = [...line];
  // Centre angle positions of every glyph along the arc.
  const angles: number[] = [];
  let prefix = '';
  for (let i = 0; i < chars.length; i++) {
    const startX = c.measureText(prefix).width + spacing * i;
    const cw = c.measureText(chars[i]).width;
    angles.push((startX + cw / 2 - total / 2) / R);
    prefix += chars[i];
  }
  const cx = w / 2;
  const cy = arch ? L.pad + L.lineHeight / 2 + R : h - L.pad - L.lineHeight / 2 - R;
  c.textAlign = 'center';
  const passes: ('stroke' | 'fill')[] = el.stroke && el.stroke.width > 0 ? ['stroke', 'fill'] : ['fill'];
  for (const pass of passes) {
    setShadow(pass === passes[0]);
    if (pass === 'stroke') {
      c.strokeStyle = el.stroke!.color;
      c.lineWidth = el.stroke!.width * 2;
    } else c.fillStyle = el.fill;
    chars.forEach((ch, i) => {
      const a = angles[i];
      c.save();
      if (arch) {
        c.translate(cx + R * Math.sin(a), cy - R * Math.cos(a));
        c.rotate(a);
      } else {
        c.translate(cx + R * Math.sin(a), cy + R * Math.cos(a));
        c.rotate(-a);
      }
      if (pass === 'fill') c.fillText(ch, 0, 0);
      else c.strokeText(ch, 0, 0);
      c.restore();
    });
  }
}

/** Height a text element should have for its current content. */
export function textHeight(el: TextElement): number {
  return layoutText(el).height;
}

/** Natural single-line width (used when creating text so the box hugs the words). */
export function naturalTextWidth(el: TextElement): number {
  const c = mctx();
  c.font = cssFont(el.font, el.weight, el.size, el.italic);
  const spacing = el.letterSpacing * el.size;
  const pad = el.background ? el.background.padding * el.size : 0;
  const lines = (el.uppercase ? el.text.toUpperCase() : el.text).split('\n');
  return Math.max(...lines.map((l) => measureWidth(c, l, spacing))) + 2 * pad + el.size * 0.1;
}

export function textReady(el: TextElement): boolean {
  return isFontLoaded(el.font, el.weight);
}

/** Update a text element's height to fit its content, keeping its visual centre. */
export function syncTextHeight(el: { h: number; y: number } & TextElement): void {
  const h = textHeight(el);
  if (Math.abs(h - el.h) > 0.01) {
    el.y += (el.h - h) / 2;
    el.h = h;
  }
}
