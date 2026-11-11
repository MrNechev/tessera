/** Generators for multi-page projects: photo books and photo calendars. */
import type { ImportedImage } from '../api/types';
import { relayout } from '../collage/layout';
import { mulberry32, randomSeed } from '../collage/rng';
import { createDoc, createPhoto, createText, defaultBackground, defaultLayout, uid } from '../model/factory';
import { fromPages } from '../model/multipage';
import { presetById, presetSize } from '../model/pages';
import type { Background, CalendarElement, CollageDoc, LayoutEngine, PageContent } from '../model/types';
import { syncTextHeight } from '../render/text';
import { templateById } from '../templates/catalog';
import { instantiateTemplate } from '../templates/instantiate';

export type BookStyle = 'clean' | 'warm' | 'dark';

export interface BookOptions {
  presetId: string;
  orientation: 'portrait' | 'landscape';
  /** Photos per page, 0 = automatic variety. */
  perPage: number;
  title: string;
  subtitle: string;
  style: BookStyle;
  pageNumbers: boolean;
}

const STYLES: Record<BookStyle, { bg: () => Background; text: string; muted: string; font: string }> = {
  clean: { bg: () => defaultBackground('#ffffff'), text: '#1d1b26', muted: '#8a8396', font: 'playfair-display' },
  warm: {
    bg: () => ({ ...defaultBackground('#f6efe6'), texture: { id: 'paper', intensity: 0.7 } }),
    text: '#3e2f28',
    muted: '#8a7262',
    font: 'lora',
  },
  dark: { bg: () => defaultBackground('#1d1b26'), text: '#f3eef9', muted: '#a49cb5', font: 'montserrat' },
};

function asset(img: ImportedImage) {
  return { path: img.path, name: img.name, width: img.width, height: img.height };
}

/** How many photos go on each page: a varied rhythm, or a fixed count. */
export function chunkSizes(n: number, perPage: number, seed = 1): number[] {
  if (n <= 0) return [];
  if (perPage > 0) {
    const out: number[] = [];
    for (let left = n; left > 0; left -= perPage) out.push(Math.min(perPage, left));
    return out;
  }
  const rhythm = [4, 3, 5, 2, 4, 6, 3, 1];
  const rng = mulberry32(seed);
  const out: number[] = [];
  let left = n;
  let k = rng.int(0, rhythm.length - 1);
  while (left > 0) {
    let take = Math.min(rhythm[k % rhythm.length], left);
    // Avoid a lonely last page unless there's only one photo left in total.
    if (left - take === 1 && take > 1) take -= 1;
    out.push(take);
    left -= take;
    k++;
  }
  return out;
}

function engineFor(count: number, k: number): LayoutEngine {
  if (count === 1) return 'grid';
  if (count === 2) return k % 2 ? 'grid' : 'rows';
  if (count === 3) return k % 2 ? 'featured' : 'rows';
  return (['mosaic', 'featured', 'rows', 'mosaic', 'grid'] as LayoutEngine[])[k % 5];
}

function pageContent(doc: CollageDoc): PageContent {
  return { id: uid('pg'), background: doc.background, frame: doc.frame, layout: doc.layout, elements: doc.elements };
}

export function buildPhotoBook(images: ImportedImage[], opts: BookOptions): CollageDoc {
  const preset = presetById(opts.presetId) ?? presetById('scrapbook-12')!;
  const [width, height] = presetSize(preset, 300, preset.mm && preset.mm[0] === preset.mm[1] ? 'auto' : opts.orientation);
  const style = STYLES[opts.style];
  const base = createDoc({ name: opts.title || 'Photo book', width, height, dpi: 300, preset: preset.id });
  for (const img of images) base.assets[img.id] = asset(img);
  const S = Math.min(width, height);
  const pages: PageContent[] = [];

  // Cover: the first photo, large, with the title underneath.
  {
    const cover = images[0];
    const doc: CollageDoc = { ...base, background: style.bg(), layout: defaultLayout('none'), elements: [] };
    const m = S * 0.07;
    const photoH = height * 0.66;
    if (cover) doc.elements.push(createPhoto({ x: m, y: m, w: width - 2 * m, h: photoH - m, asset: cover.id, mask: { shape: 'rect', radius: 0.015 } }));
    const title = createText({ text: opts.title || 'Our Story', x: m, y: 0, w: width - 2 * m, font: style.font, size: S * 0.085, fill: style.text, weight: 700 });
    syncTextHeight(title);
    title.y = photoH + (height - photoH) * 0.36 - title.h / 2;
    doc.elements.push(title);
    if (opts.subtitle) {
      const sub = createText({ text: opts.subtitle.toUpperCase(), x: m, y: 0, w: width - 2 * m, font: 'josefin-sans', size: S * 0.026, fill: style.muted, weight: 700, letterSpacing: 0.3 });
      syncTextHeight(sub);
      sub.y = title.y + title.h + S * 0.02;
      doc.elements.push(sub);
    }
    pages.push(pageContent(doc));
  }

  // Inner pages: varied layouts.
  const rest = images.slice(1);
  const sizes = chunkSizes(rest.length, opts.perPage, randomSeed());
  let at = 0;
  sizes.forEach((count, k) => {
    const group = rest.slice(at, at + count);
    at += count;
    const engine = engineFor(count, k);
    let doc: CollageDoc = {
      ...base,
      background: style.bg(),
      layout: { ...defaultLayout(engine), spacing: 0.014, margin: 0.065, region: opts.pageNumbers ? { x: 0, y: 0, w: 1, h: 0.95 } : null },
      elements: group.map((img) => createPhoto({ x: 0, y: 0, w: 1, h: 1, asset: img.id, slot: true, mask: { shape: 'rect', radius: 0.01 } })),
    };
    doc = relayout(doc);
    if (opts.pageNumbers) {
      const num = createText({ text: String(k + 2), x: width / 2 - S * 0.05, y: 0, w: S * 0.1, font: 'josefin-sans', size: S * 0.02, fill: style.muted, weight: 700 });
      syncTextHeight(num);
      num.y = height - S * 0.045 - num.h / 2;
      doc = { ...doc, elements: [...doc.elements, num] };
    }
    pages.push(pageContent(doc));
  });

  return fromPages(base, pages, 0);
}

export interface CalendarOptions {
  year: number;
  firstDay: 0 | 1;
  title: string;
}

/** Cover + twelve month pages, one photo each (photos repeat if fewer than 12). */
export function buildPhotoCalendar(images: ImportedImage[], opts: CalendarOptions): CollageDoc {
  const tpl = templateById('cal-photo-month')!;
  const pages: PageContent[] = [];
  let base: CollageDoc | null = null;
  const photo = (i: number) => (images.length ? [{ id: images[i % images.length].id, ref: asset(images[i % images.length]) }] : []);

  for (let m = 1; m <= 12; m++) {
    const doc = instantiateTemplate(tpl, { photos: photo(m), stickerAspect: () => 1 });
    const elements = doc.elements.map((e) =>
      e.kind === 'calendar' ? ({ ...e, month: m, year: opts.year, firstDay: opts.firstDay } as CalendarElement) : e,
    );
    if (!base) base = doc;
    pages.push({ ...pageContent(doc), elements });
  }
  base = base!;
  const { width: W, height: H } = base.page;
  const S = Math.min(W, H);
  const cover: CollageDoc = { ...base, background: defaultBackground('#ffffff'), layout: defaultLayout('none'), elements: [] };
  if (images[0]) cover.elements.push(createPhoto({ x: S * 0.06, y: S * 0.06, w: W - S * 0.12, h: H * 0.64, asset: images[0].id, mask: { shape: 'rect', radius: 0.012 } }));
  const year = createText({ text: String(opts.year), x: 0, y: 0, w: W * 0.8, font: 'bebas-neue', size: S * 0.16, fill: '#1d1b26', letterSpacing: 0.25 });
  syncTextHeight(year);
  year.x = (W - year.w) / 2;
  year.y = H * 0.84 - year.h / 2;
  const title = createText({ text: opts.title || 'Our Year', x: 0, y: 0, w: W * 0.8, font: 'great-vibes', size: S * 0.07, fill: '#c2185b' });
  syncTextHeight(title);
  title.x = (W - title.w) / 2;
  title.y = year.y - title.h * 0.8;
  cover.elements.push(year, title);

  const doc = createDoc({ name: opts.title || `Calendar ${opts.year}`, width: W, height: H, dpi: base.page.dpi, preset: base.page.preset });
  for (const img of images) doc.assets[img.id] = asset(img);
  return fromPages(doc, [pageContent(cover), ...pages], 0);
}
