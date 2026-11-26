/**
 * OpenRaster (https://www.openraster.org/): Paint's layered file format,
 * which GIMP, Krita and MyPaint open too. A zip with `mimetype` (first,
 * stored), `stack.xml`, one PNG per layer under `data/`, `mergedimage.png`
 * and `Thumbnails/thumbnail.png` (at most 256 px).
 *
 * Only the XML and zip parts live here (no DOM), so they are unit-tested;
 * PNG encoding/decoding is the caller's job.
 */
import { readZip, writeZip, type ZipEntry } from './zip';

export type BlendMode =
  | 'source-over'
  | 'multiply'
  | 'screen'
  | 'overlay'
  | 'darken'
  | 'lighten'
  | 'color-dodge'
  | 'color-burn'
  | 'hard-light'
  | 'soft-light'
  | 'difference'
  | 'exclusion'
  | 'hue'
  | 'saturation'
  | 'color'
  | 'luminosity'
  | 'lighter';

/** Canvas composite operation ↔ OpenRaster `composite-op`. */
const TO_ORA: Record<BlendMode, string> = {
  'source-over': 'svg:src-over',
  multiply: 'svg:multiply',
  screen: 'svg:screen',
  overlay: 'svg:overlay',
  darken: 'svg:darken',
  lighten: 'svg:lighten',
  'color-dodge': 'svg:color-dodge',
  'color-burn': 'svg:color-burn',
  'hard-light': 'svg:hard-light',
  'soft-light': 'svg:soft-light',
  difference: 'svg:difference',
  exclusion: 'svg:exclusion',
  hue: 'svg:hue',
  saturation: 'svg:saturation',
  color: 'svg:color',
  luminosity: 'svg:luminosity',
  lighter: 'svg:plus',
};
const FROM_ORA = new Map(Object.entries(TO_ORA).map(([k, v]) => [v, k as BlendMode]));

export function blendToOra(b: BlendMode): string {
  return TO_ORA[b] ?? 'svg:src-over';
}
export function blendFromOra(op: string | undefined): BlendMode {
  return (op && FROM_ORA.get(op)) || 'source-over';
}

export interface OraLayer {
  name: string;
  /** PNG bytes of the layer's pixels (placed at x, y). */
  png: Uint8Array;
  x: number;
  y: number;
  opacity: number;
  visible: boolean;
  locked: boolean;
  blend: BlendMode;
  selected?: boolean;
  /** Tessera: the live text of a text layer (JSON), so it stays editable. */
  text?: string;
}

export interface OraImage {
  width: number;
  height: number;
  /** Top layer first, as in stack.xml. */
  layers: OraLayer[];
}

export const TESSERA_NS = 'https://github.com/MrNechev/tessera/ns/paint';

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/\n/g, '&#10;');
}

function unesc(s: string): string {
  return s.replace(/&(#x?[0-9a-f]+|amp|lt|gt|quot|apos);/gi, (m, e: string) => {
    const k = e.toLowerCase();
    if (k === 'amp') return '&';
    if (k === 'lt') return '<';
    if (k === 'gt') return '>';
    if (k === 'quot') return '"';
    if (k === 'apos') return "'";
    const code = k.startsWith('#x') ? parseInt(k.slice(2), 16) : parseInt(k.slice(1), 10);
    return Number.isFinite(code) ? String.fromCodePoint(code) : m;
  });
}

/** The stack.xml for these layers (`src` paths are data/layerN.png, top first). */
export function stackXml(img: { width: number; height: number; layers: Omit<OraLayer, 'png'>[] }): string {
  const lines = img.layers.map((l, i) => {
    const attrs = [
      `name="${esc(l.name)}"`,
      `src="data/layer${img.layers.length - 1 - i}.png"`,
      `x="${Math.round(l.x)}"`,
      `y="${Math.round(l.y)}"`,
      `opacity="${Math.max(0, Math.min(1, l.opacity)).toFixed(3)}"`,
      `visibility="${l.visible ? 'visible' : 'hidden'}"`,
      `composite-op="${blendToOra(l.blend)}"`,
    ];
    if (l.locked) attrs.push('edit-locked="true"');
    if (l.selected) attrs.push('selected="true"');
    if (l.text) attrs.push(`tessera:text="${esc(l.text)}"`);
    return `    <layer ${attrs.join(' ')}/>`;
  });
  return (
    `<?xml version="1.0" encoding="UTF-8"?>\n` +
    `<image version="0.0.5" w="${img.width}" h="${img.height}" xres="72" yres="72" xmlns:tessera="${TESSERA_NS}">\n` +
    `  <stack>\n${lines.join('\n')}\n  </stack>\n</image>\n`
  );
}

function attrsOf(tag: string): Record<string, string> {
  const out: Record<string, string> = {};
  const re = /([\w:.-]+)\s*=\s*("([^"]*)"|'([^']*)')/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(tag))) out[m[1]] = unesc(m[3] ?? m[4] ?? '');
  return out;
}

export interface StackLayer extends Omit<OraLayer, 'png'> {
  src: string;
}

/** Parse a stack.xml: image size and every layer, top first (nested stacks are flattened). */
export function parseStackXml(xml: string): { width: number; height: number; layers: StackLayer[] } {
  const imgTag = /<image\b[^>]*>/.exec(xml);
  if (!imgTag) throw new Error('stack.xml has no <image>');
  const ia = attrsOf(imgTag[0]);
  const width = parseInt(ia.w, 10);
  const height = parseInt(ia.h, 10);
  if (!(width > 0 && height > 0)) throw new Error('stack.xml has no image size');
  // Hidden or translucent sub-stacks hide/dim their layers.
  const layers: StackLayer[] = [];
  const stack: { visible: boolean; opacity: number; x: number; y: number }[] = [{ visible: true, opacity: 1, x: 0, y: 0 }];
  const re = /<(\/?)(stack|layer)\b([^>]*?)(\/?)>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(xml))) {
    const [, close, kind, body, selfClose] = m;
    const top = stack[stack.length - 1];
    if (kind === 'stack') {
      if (close) {
        if (stack.length > 1) stack.pop();
      } else if (!selfClose) {
        const a = attrsOf(body);
        stack.push({
          visible: top.visible && a.visibility !== 'hidden',
          opacity: top.opacity * (a.opacity !== undefined ? clamp01(parseFloat(a.opacity)) : 1),
          x: top.x + (parseInt(a.x ?? '0', 10) || 0),
          y: top.y + (parseInt(a.y ?? '0', 10) || 0),
        });
      }
      continue;
    }
    if (close) continue;
    const a = attrsOf(body);
    if (!a.src) continue;
    layers.push({
      name: a.name ?? `Layer ${layers.length + 1}`,
      src: a.src,
      x: top.x + (parseInt(a.x ?? '0', 10) || 0),
      y: top.y + (parseInt(a.y ?? '0', 10) || 0),
      opacity: top.opacity * (a.opacity !== undefined ? clamp01(parseFloat(a.opacity)) : 1),
      visible: top.visible && a.visibility !== 'hidden',
      locked: a['edit-locked'] === 'true',
      blend: blendFromOra(a['composite-op']),
      selected: a.selected === 'true',
      ...(a['tessera:text'] ? { text: a['tessera:text'] } : {}),
    });
  }
  return { width, height, layers };
}

function clamp01(v: number): number {
  return Number.isFinite(v) ? Math.max(0, Math.min(1, v)) : 1;
}

const enc = new TextEncoder();

/** Build the .ora file. `merged` is the flattened image, `thumbnail` ≤ 256 px (both PNG). */
export function buildOra(img: OraImage, merged: Uint8Array, thumbnail: Uint8Array): Uint8Array<ArrayBuffer> {
  const entries: ZipEntry[] = [
    { name: 'mimetype', data: enc.encode('image/openraster') },
    { name: 'stack.xml', data: enc.encode(stackXml(img)) },
  ];
  img.layers.forEach((l, i) => entries.push({ name: `data/layer${img.layers.length - 1 - i}.png`, data: l.png }));
  entries.push({ name: 'mergedimage.png', data: merged });
  entries.push({ name: 'Thumbnails/thumbnail.png', data: thumbnail });
  return writeZip(entries);
}

/** Read an .ora file: size and layers (top first) with their PNG bytes. */
export function parseOra(bytes: Uint8Array): OraImage & { merged?: Uint8Array } {
  const zip = readZip(bytes);
  const mime = zip.get('mimetype');
  if (!mime || new TextDecoder().decode(mime()).trim() !== 'image/openraster') throw new Error('This is not an OpenRaster (.ora) file');
  const stack = zip.get('stack.xml');
  if (!stack) throw new Error('This .ora file has no stack.xml');
  const s = parseStackXml(new TextDecoder().decode(stack()));
  const layers: OraLayer[] = [];
  for (const l of s.layers) {
    const f = zip.get(l.src.replace(/^\.?\//, ''));
    if (!f) continue;
    const { src: _src, ...rest } = l;
    void _src;
    layers.push({ ...rest, png: f() });
  }
  const merged = zip.get('mergedimage.png');
  return { width: s.width, height: s.height, layers, ...(merged ? { merged: merged() } : {}) };
}
