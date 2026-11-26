// Canvas → SVG: shapes, connectors, notes, text, drawings, icons, frames, stickers and
// section cards (title + first picture, embedded) as plain vector SVG that other apps open.
import { arrowPath, contentRect, ink, isBox, paper, pathPoints, smoothPath, targets, zOf } from '../canvas';
import { thumbUrl } from '../api';
import { resolver } from '../resolver.svelte';
import { stickers } from '../../../stores/stickers.svelte';
import { fontInfo } from '../../../assets/fonts';
import type { ArrowItem, Doc, NoteItem, Rect, Section, ShapeItem } from '../types';
import { dashArray, dashOf, headsOf } from './connectors';
import { iconPath } from './icons';
import { DEFAULT_RADIUS, outlineExtra, outlineKind, outlinePath, textBox } from './shapes';
import { fillOf, HEAD_SHAPES, markerId, markerScale, shapeTextColor } from './style';

const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const n = (v: number) => String(Math.round(v * 100) / 100);

async function dataUrl(url: string): Promise<string | null> {
  try {
    const r = await fetch(url);
    if (!r.ok) return null;
    const b = await r.blob();
    return await new Promise((res) => {
      const fr = new FileReader();
      fr.onload = () => res(String(fr.result));
      fr.onerror = () => res(null);
      fr.readAsDataURL(b);
    });
  } catch {
    return null;
  }
}

let measure: CanvasRenderingContext2D | null = null;
/** break text into lines that fit `width` in the given CSS font */
function wrap(text: string, font: string, width: number, upper = false): string[] {
  measure ??= document.createElement('canvas').getContext('2d');
  const ctx = measure!;
  ctx.font = font;
  const out: string[] = [];
  for (const para of (upper ? text.toUpperCase() : text).split('\n')) {
    let line = '';
    for (const word of para.split(/(\s+)/)) {
      const next = line + word;
      if (line && ctx.measureText(next.trimEnd()).width > width) {
        out.push(line.trimEnd());
        line = word.trimStart();
      } else line = next;
    }
    out.push(line.trimEnd());
  }
  return out;
}

interface Theme {
  [v: string]: string;
}

/** the theme's colours and fonts as seen by `el` (the canvas: story tokens live on its ancestors) */
function readTheme(el: Element): Theme {
  const cs = getComputedStyle(el);
  const t: Theme = {};
  const fallback: Record<string, string> = { '--bg': '#ffffff', '--surface': '#ffffff', '--ui': 'sans-serif', '--serif': 'serif', '--line-strong': '#cccccc', '--text-2': '#555555' };
  for (const v of ['--text', '--text-2', '--bg', '--surface', '--line-strong', '--accent', '--ui', '--serif']) t[v] = cs.getPropertyValue(v).trim() || fallback[v] || '#000000';
  return t;
}

/** text block: lines centred (or aligned) in a box, vertically centred or from the top */
function textSvg(text: string, box: Rect, o: { size: number; color: string; family: string; weight: number; italic?: boolean; upper?: boolean; align?: string; top?: boolean; halo?: string }) {
  const font = `${o.italic ? 'italic ' : ''}${o.weight} ${o.size}px ${o.family}`;
  const lines = wrap(text, font, Math.max(10, box.w), o.upper);
  const lh = o.size * 1.28;
  const total = lines.length * lh;
  const y0 = o.top ? box.y + o.size : box.y + box.h / 2 - total / 2 + o.size * 0.92;
  const anchor = o.align === 'left' ? 'start' : o.align === 'right' ? 'end' : 'middle';
  const x = o.align === 'left' ? box.x : o.align === 'right' ? box.x + box.w : box.x + box.w / 2;
  const halo = o.halo ? ` stroke="${o.halo}" stroke-width="${n(o.size * 0.28)}" paint-order="stroke" stroke-linejoin="round"` : '';
  const spans = lines.map((l, i) => `<tspan x="${n(x)}" y="${n(y0 + i * lh)}">${esc(l) || ' '}</tspan>`).join('');
  return `<text font-family="${esc(o.family)}" font-size="${n(o.size)}" font-weight="${o.weight}"${o.italic ? ' font-style="italic"' : ''} fill="${o.color}" text-anchor="${anchor}"${halo}>${spans}</text>`;
}

/**
 * The region `rect` of the canvas (default: everything) as an SVG document.
 * `only` limits it to these ids (selection export).
 */
export async function canvasSvg(doc: Doc, rect: Rect | null, opts: { only?: Set<string>; transparent?: boolean; themeEl?: Element } = {}): Promise<string> {
  const th = readTheme(opts.themeEl ?? document.body);
  const col = (c: string) => c.replace(/var\((--[\w-]+)\)/g, (_, v) => th[v] ?? '#000');
  const ui = th['--ui'] || 'sans-serif';
  const r0 = rect ?? contentRect(doc) ?? { x: 0, y: 0, w: 800, h: 600 };
  const pad = Math.max(16, Math.min(r0.w, r0.h) * 0.03);
  const R = { x: r0.x - pad, y: r0.y - pad, w: r0.w + 2 * pad, h: r0.h + 2 * pad };
  const tg = targets(doc);
  const want = (id: string) => !opts.only || opts.only.has(id);
  const body: { z: number; svg: string }[] = [];
  const familyOf = (look?: { font: string }) => (look ? `'${fontInfo(look.font).family}', ${ui}` : ui);

  // frames, shapes, notes, icons, stickers, drawings and cards, in stacking order
  const items = (doc.canvas?.items ?? []).filter((it) => want(it.id));
  if (items.some((i) => i.type === 'sticker') && !stickers.loaded) await stickers.load();
  for (const it of items) {
    if (!isBox(it)) continue;
    const op = it.type !== 'frame' && it.opacity !== undefined && it.opacity < 1 ? ` opacity="${it.opacity}"` : '';
    let s = '';
    if (it.type === 'frame') {
      s = `<rect x="${n(it.x)}" y="${n(it.y)}" width="${n(it.w)}" height="${n(it.h)}" rx="4" fill="${col('var(--surface)')}" fill-opacity="0.35" stroke="${col('var(--line-strong)')}" stroke-width="2"/>`;
      if (it.title) s += `<text x="${n(it.x)}" y="${n(it.y - 6)}" font-family="${esc(ui)}" font-size="13" font-weight="600" fill="${col('var(--text-2)')}">${esc(it.title)}</text>`;
    } else if (it.type === 'rect' || it.type === 'ellipse') {
      s = shapeSvg(it, col, familyOf(it.look));
    } else if (it.type === 'note') {
      s = noteSvg(it, col, familyOf(it.look));
    } else if (it.type === 'icon') {
      const k = Math.min(it.w, it.h) / 24;
      const ox = it.x + (it.w - 24 * k) / 2,
        oy = it.y + (it.h - 24 * k) / 2;
      s = `<path transform="translate(${n(ox)} ${n(oy)}) scale(${n(k)})" d="${iconPath(it.icon)}" fill="none" stroke="${col(ink(it.color))}" stroke-width="${it.strokeWidth ?? 2}" stroke-linecap="round" stroke-linejoin="round"/>`;
    } else if (it.type === 'path') {
      s = `<path d="${smoothPath(pathPoints(it))}" fill="none" stroke="${col(ink(it.color))}" stroke-width="${n(it.width)}" stroke-linecap="${it.highlighter ? 'square' : 'round'}" stroke-linejoin="round"${it.highlighter ? ' stroke-opacity="0.38"' : ''}/>`;
    } else if (it.type === 'sticker') {
      const href = stickers.loaded ? await dataUrl(stickers.url(it.sticker)) : null;
      if (href) {
        const t: string[] = [];
        if (it.rotation) t.push(`rotate(${it.rotation} ${n(it.x + it.w / 2)} ${n(it.y + it.h / 2)})`);
        if (it.flipX || it.flipY) {
          const cx = it.x + it.w / 2,
            cy = it.y + it.h / 2;
          t.push(`translate(${n(cx)} ${n(cy)}) scale(${it.flipX ? -1 : 1} ${it.flipY ? -1 : 1}) translate(${n(-cx)} ${n(-cy)})`);
        }
        s = `<image x="${n(it.x)}" y="${n(it.y)}" width="${n(it.w)}" height="${n(it.h)}" href="${href}"${t.length ? ` transform="${t.join(' ')}"` : ''}/>`;
      }
    }
    if (s) body.push({ z: zOf(it, it.type), svg: op ? `<g${op}>${s}</g>` : s });
  }
  for (const sec of doc.sections) {
    if (!sec.canvas || sec.canvas.hidden || !want(sec.id)) continue;
    body.push({ z: zOf(sec.canvas, 'card'), svg: await cardSvg(sec, col) });
  }
  body.sort((a, b) => a.z - b.z);

  // connectors on top
  const arrows = items.filter((i): i is ArrowItem => i.type === 'arrow');
  const defs = new Map<string, string>();
  const arrowSvg: string[] = [];
  for (const a of arrows) {
    const p = arrowPath(a, tg);
    const h = headsOf(a);
    const w = a.width ?? 3;
    const c = col(ink(a.color));
    for (const head of [h.start, h.end]) {
      if (head === 'none') continue;
      const id = markerId(head, a.color, w);
      if (defs.has(id)) continue;
      const hs = HEAD_SHAPES[head];
      const k = markerScale(w);
      defs.set(
        id,
        `<marker id="${id}" viewBox="-1 -1 12 12" refX="${hs.refX}" refY="5" markerWidth="${n(k * 1.2)}" markerHeight="${n(k * 1.2)}" orient="auto-start-reverse" markerUnits="strokeWidth"><path d="${hs.d}" fill="${hs.fill ? c : 'none'}" stroke="${hs.fill ? 'none' : c}" stroke-width="${hs.fill ? 0 : 1.6}" stroke-linecap="round" stroke-linejoin="round"/></marker>`,
      );
    }
    const dash = dashArray(dashOf(a), w);
    let s = `<path d="${p.d}" fill="none" stroke="${c}" stroke-width="${w}" stroke-linecap="round" stroke-linejoin="round"${dash ? ` stroke-dasharray="${dash}"` : ''}${h.end !== 'none' ? ` marker-end="url(#${markerId(h.end, a.color, w)})"` : ''}${h.start !== 'none' ? ` marker-start="url(#${markerId(h.start, a.color, w)})"` : ''}/>`;
    if (a.text) {
      const onLine = !!a.route || a.labelAt !== undefined || !!a.from.side || !!a.to.side;
      s += `<text x="${n(p.label.x)}" y="${n(onLine ? p.label.y + 6 : p.label.y - 10)}" text-anchor="middle" font-family="${esc(ui)}" font-size="18" font-weight="600" fill="${c}" stroke="${col('var(--bg)')}" stroke-width="5" paint-order="stroke" stroke-linejoin="round">${esc(a.text)}</text>`;
    }
    const op = a.opacity !== undefined && a.opacity < 1 ? ` opacity="${a.opacity}"` : '';
    arrowSvg.push(op ? `<g${op}>${s}</g>` : s);
  }

  const bg = opts.transparent ? '' : `<rect x="${n(R.x)}" y="${n(R.y)}" width="${n(R.w)}" height="${n(R.h)}" fill="${col('var(--bg)')}"/>`;
  return (
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="${n(R.x)} ${n(R.y)} ${n(R.w)} ${n(R.h)}" width="${Math.round(R.w)}" height="${Math.round(R.h)}">` +
    `<defs>${[...defs.values()].join('')}</defs>${bg}${body.map((b) => b.svg).join('')}${arrowSvg.join('')}</svg>`
  );
}

function shapeSvg(it: ShapeItem, col: (c: string) => string, family: string): string {
  const k = outlineKind(it);
  const sw = it.strokeWidth ?? 3;
  let fill = fillOf(it);
  let fo = '';
  if (fill.startsWith('color-mix')) {
    fill = ink(it.color);
    fo = ' fill-opacity="0.22"';
  }
  const dash = dashArray(it.dash ?? 'solid', sw);
  const extra = outlineExtra(k, it.w, it.h, sw);
  let s = `<g transform="translate(${n(it.x)} ${n(it.y)})"><path d="${outlinePath(k, it.w, it.h, sw, it.radius ?? DEFAULT_RADIUS)}" fill="${col(fill)}"${fo} stroke="${sw > 0 ? col(ink(it.color)) : 'none'}" stroke-width="${sw}"${dash ? ` stroke-dasharray="${dash}"` : ''}/>`;
  if (extra) s += `<path d="${extra}" fill="none" stroke="${col(ink(it.color))}" stroke-width="${sw}"/>`;
  s += '</g>';
  if (it.text) {
    const tb = textBox(k, it.w, it.h);
    const size = it.size || 24;
    s += textSvg(it.text, { x: it.x + tb.x, y: it.y + tb.y, w: tb.w, h: tb.h }, {
      size,
      color: col(it.look?.fill ?? shapeTextColor(it)),
      family,
      weight: it.look?.weight ?? 400,
      italic: it.look?.italic,
      upper: it.look?.uppercase,
      align: it.align ?? 'center',
    });
  }
  return s;
}

function noteSvg(it: NoteItem, col: (c: string) => string, family: string): string {
  const free = it.color === 'none';
  let s = '';
  if (!free) s += `<rect x="${n(it.x)}" y="${n(it.y)}" width="${n(it.w)}" height="${n(it.h)}" rx="4" fill="${col(paper(it.color))}"/>`;
  else if (it.look?.background) {
    const b = it.look.background;
    s += `<rect x="${n(it.x)}" y="${n(it.y)}" width="${n(it.w)}" height="${n(it.h)}" rx="${n(b.radius * it.size)}" fill="${b.color}"/>`;
  }
  const padX = free ? it.size * 0.15 : it.size * 0.65,
    padY = free ? it.size * 0.1 : it.size * 0.55;
  if (it.text)
    s += textSvg(it.text, { x: it.x + padX, y: it.y + padY, w: it.w - 2 * padX, h: it.h - 2 * padY }, {
      size: it.size,
      color: col(it.look?.fill ?? (free ? 'var(--text)' : '#1d1b22')),
      family,
      weight: it.look?.weight ?? 400,
      italic: it.look?.italic,
      upper: it.look?.uppercase,
      align: it.align ?? 'left',
      top: true,
      halo: it.look?.stroke ? it.look.stroke.color : undefined,
    });
  return s;
}

async function cardSvg(sec: Section, col: (c: string) => string): Promise<string> {
  const c = sec.canvas!;
  let s = `<rect x="${n(c.x)}" y="${n(c.y)}" width="${n(c.w)}" height="${n(c.h)}" rx="10" fill="${col('var(--surface)')}" stroke="${col('var(--line-strong)')}"/>`;
  let top = c.y + 12;
  if (sec.title) {
    s += `<text x="${n(c.x + 14)}" y="${n(c.y + 34)}" font-family="${esc(col('var(--serif)'))}, serif" font-size="22" font-weight="650" fill="${col('var(--text)')}">${esc(sec.title)}</text>`;
    top = c.y + 46;
  }
  const m = sec.media[0];
  const r = m ? resolver.get(m) : undefined;
  if (r?.found) {
    const href = await dataUrl(thumbUrl(r.derived, 1280));
    if (href) s += `<image x="${n(c.x + 12)}" y="${n(top)}" width="${n(c.w - 24)}" height="${n(Math.max(10, c.y + c.h - top - 12))}" href="${href}" preserveAspectRatio="xMidYMid slice"/>`;
  }
  return `<g>${s}</g>`;
}

/** trigger a download of text as a file */
export function downloadText(text: string, name: string, type: string) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
}

