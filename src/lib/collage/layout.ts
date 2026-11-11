import { createPhoto } from '../model/factory';
import type { CollageDoc, LayoutEngine, PhotoElement, Rect } from '../model/types';
import { lettersEngine, ringEngine, scatterEngine, shapeEngine } from './engines/artistic';
import { spiralEngine } from './engines/spiral';
import { columnsEngine } from './engines/columns';
import { featuredEngine, gridEngine, rowsEngine } from './engines/grids';
import { mosaicEngine } from './engines/mosaic';
import type { Engine, LayoutItem } from './engines/types';
import { mulberry32, randomSeed } from './rng';

export interface EngineInfo {
  id: Exclude<LayoutEngine, 'none'>;
  name: string;
  description: string;
}

export const ENGINE_INFO: EngineInfo[] = [
  { id: 'mosaic', name: 'Mosaic', description: 'Fills the page edge to edge while cropping photos as little as possible.' },
  { id: 'columns', name: 'Classic', description: 'The PhotoCollage column layout, with wide photos spanning two columns.' },
  { id: 'rows', name: 'Rows', description: 'Justified rows, like a photo gallery.' },
  { id: 'grid', name: 'Grid', description: 'Equal cells in a neat grid.' },
  { id: 'featured', name: 'Featured', description: 'One large hero photo, the rest beside it.' },
  { id: 'shape', name: 'Shape', description: 'Photos arranged into a heart, circle, star and more.' },
  { id: 'scatter', name: 'Scattered', description: 'Polaroids casually tossed on the table.' },
  { id: 'ring', name: 'Rings', description: 'A circular centre photo surrounded by rings of photos.' },
  { id: 'letters', name: 'Letters', description: 'Photos fill the letters of a word, a name or a number.' },
  { id: 'spiral', name: 'Spiral', description: 'A golden spiral: each photo a little smaller than the last.' },
];

const ENGINES: Record<Exclude<LayoutEngine, 'none'>, Engine> = {
  mosaic: mosaicEngine,
  columns: columnsEngine,
  rows: rowsEngine,
  grid: gridEngine,
  featured: featuredEngine,
  shape: shapeEngine,
  scatter: scatterEngine,
  ring: ringEngine,
  letters: lettersEngine,
  spiral: spiralEngine,
};

export function pageShort(doc: CollageDoc): number {
  return Math.min(doc.page.width, doc.page.height);
}

/** The area available to the auto-layout: optional region minus the margin. */
export function layoutArea(doc: CollageDoc): Rect {
  const { width: W, height: H } = doc.page;
  const r = doc.layout.region;
  const base: Rect = r ? { x: r.x * W, y: r.y * H, w: r.w * W, h: r.h * H } : { x: 0, y: 0, w: W, h: H };
  const m = doc.layout.margin * pageShort(doc);
  return { x: base.x + m, y: base.y + m, w: Math.max(1, base.w - 2 * m), h: Math.max(1, base.h - 2 * m) };
}

export function managedPhotos(doc: CollageDoc): PhotoElement[] {
  return doc.elements.filter((e): e is PhotoElement => e.kind === 'photo' && !!e.slot);
}

/** Asset ids participating in the layout, in their current order, without repeats. */
export function layoutAssetOrder(doc: CollageDoc): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of managedPhotos(doc)) {
    if (p.asset && !seen.has(p.asset) && doc.assets[p.asset]) {
      seen.add(p.asset);
      out.push(p.asset);
    }
  }
  return out;
}

function aspectOf(doc: CollageDoc, asset: string): number {
  const a = doc.assets[asset];
  return a && a.width > 0 && a.height > 0 ? a.width / a.height : 1.5;
}

export interface RelayoutOptions {
  /** New random arrangement (PhotoCollage's "Regenerate"). */
  shuffle?: boolean;
  /** Explicit seed (layout history navigation). */
  seed?: number;
  /** Explicit asset order (layout history navigation / adding photos). */
  order?: string[];
  /** Engine change: force the engine's own photo style. */
  restyle?: boolean;
}

/**
 * Recompute the auto-layout of `doc`. Returns a new document (input untouched).
 * Managed photo elements keep their ids and individual styling where possible.
 */
export function relayout(doc: CollageDoc, opts: RelayoutOptions = {}): CollageDoc {
  const engineId = doc.layout.engine;
  if (engineId === 'none') return doc;
  const seed = opts.seed ?? (opts.shuffle ? randomSeed() : doc.layout.seed);
  const rng = mulberry32(seed);
  let order = opts.order ?? layoutAssetOrder(doc);
  if (opts.shuffle && !opts.order) order = rng.shuffle([...order]);
  const items: LayoutItem[] = order.map((key) => ({ key, aspect: aspectOf(doc, key) }));
  const short = pageShort(doc);
  const result = ENGINES[engineId]({
    items,
    area: layoutArea(doc),
    gap: doc.layout.spacing * short,
    rng,
    settings: doc.layout,
    pageShort: short,
  });

  const old = managedPhotos(doc);
  const pool = new Map<string, PhotoElement[]>();
  for (const p of old) {
    if (!p.asset) continue;
    const list = pool.get(p.asset) ?? [];
    list.push(p);
    pool.set(p.asset, list);
  }
  const template = old[0];
  const fresh: PhotoElement[] = result.placements.map((pl) => {
    const asset = items[pl.item].key;
    const reuse = pool.get(asset)?.shift();
    const base: PhotoElement = reuse
      ? { ...reuse }
      : createPhoto({
          x: 0,
          y: 0,
          w: 1,
          h: 1,
          asset,
          mask: template ? { ...template.mask } : { shape: 'rect', radius: 0 },
          border: template?.border ? { ...template.border } : null,
          shadow: template?.shadow ? { ...template.shadow } : null,
          frame: template?.frame ? { style: template.frame.style, color: template.frame.color } : null,
        });
    const el: PhotoElement = {
      ...base,
      asset,
      slot: true,
      x: pl.rect.x,
      y: pl.rect.y,
      w: pl.rect.w,
      h: pl.rect.h,
      rotation: pl.rotation ?? 0,
    };
    if (result.style && (opts.restyle || !reuse)) {
      if (result.style.mask) el.mask = { ...result.style.mask };
      if (result.style.frame !== undefined) el.frame = result.style.frame ? { ...result.style.frame } : null;
      if (result.style.shadow !== undefined) el.shadow = result.style.shadow ? { ...result.style.shadow } : null;
    } else if (opts.restyle) {
      // Leaving a styling engine (e.g. polaroids → mosaic): drop forced frames.
      if (el.frame?.style === 'polaroid' && engineId !== 'scatter') el.frame = null;
      if (el.mask.shape === 'circle' && engineId !== 'ring') el.mask = { shape: 'rect', radius: 0 };
    }
    return el;
  });

  // Splice managed photos back where the first one was, keeping other elements' order.
  const firstIdx = doc.elements.findIndex((e) => e.kind === 'photo' && (e as PhotoElement).slot);
  const others = doc.elements.filter((e) => !(e.kind === 'photo' && (e as PhotoElement).slot));
  let insertAt = 0;
  if (firstIdx >= 0) {
    insertAt = doc.elements.slice(0, firstIdx).filter((e) => !(e.kind === 'photo' && (e as PhotoElement).slot)).length;
  } else {
    // New layout: above background-ish shapes but below everything else.
    insertAt = 0;
  }
  const elements = [...others.slice(0, insertAt), ...fresh, ...others.slice(insertAt)];
  return { ...doc, layout: { ...doc.layout, seed }, elements };
}

/** Add assets to the auto-layout (appended to the current order). */
export function addToLayout(doc: CollageDoc, assetIds: string[]): CollageDoc {
  const order = layoutAssetOrder(doc);
  const set = new Set(order);
  for (const a of assetIds) if (!set.has(a)) order.push(a);
  return relayout(doc, { order });
}

/** Remove one asset from the layout. */
export function removeFromLayout(doc: CollageDoc, assetId: string): CollageDoc {
  const order = layoutAssetOrder(doc).filter((a) => a !== assetId);
  const stripped: CollageDoc = {
    ...doc,
    elements: doc.elements.filter((e) => !(e.kind === 'photo' && e.slot && e.asset === assetId)),
  };
  if (order.length === 0) return stripped;
  return relayout(doc, { order });
}
