import {
  DOC_VERSION,
  type Background,
  type CollageDoc,
  type PhotoElement,
  type ShapeElement,
  type StickerElement,
  type TextElement,
  type LayoutSettings,
  type PhotoAdjust,
  type CollageElement,
  type CalendarElement,
} from './types';

export function uid(prefix = 'e'): string {
  const rnd =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID().replace(/-/g, '').slice(0, 12)
      : Math.random().toString(36).slice(2, 14);
  return `${prefix}_${rnd}`;
}

export function nowIso(): string {
  return new Date().toISOString();
}

export function defaultBackground(color = '#ffffff'): Background {
  return {
    fill: { kind: 'solid', color },
    pattern: null,
    texture: null,
    image: null,
    vignette: 0,
  };
}

export function defaultLayout(engine: LayoutSettings['engine'] = 'none'): LayoutSettings {
  return {
    engine,
    seed: Math.floor(Math.random() * 2 ** 31),
    spacing: 0.012,
    margin: 0.012,
    region: null,
    shape: 'heart',
    repeat: true,
    columns: 0,
  };
}

export function createDoc(opts: {
  name?: string;
  width: number;
  height: number;
  dpi?: number;
  preset?: string;
  background?: Background;
  layout?: LayoutSettings;
}): CollageDoc {
  const t = nowIso();
  return {
    version: DOC_VERSION,
    id: uid('doc'),
    name: opts.name ?? 'Untitled collage',
    created: t,
    modified: t,
    page: { width: opts.width, height: opts.height, dpi: opts.dpi ?? 300, preset: opts.preset },
    background: opts.background ?? defaultBackground(),
    frame: null,
    layout: opts.layout ?? defaultLayout(),
    elements: [],
    assets: {},
    template: null,
  };
}

export function neutralAdjust(): PhotoAdjust {
  return {
    brightness: 0,
    contrast: 0,
    saturation: 0,
    warmth: 0,
    fade: 0,
    vignette: 0,
    grayscale: 0,
    sepia: 0,
  };
}

export function createPhoto(p: Partial<PhotoElement> & Pick<PhotoElement, 'x' | 'y' | 'w' | 'h'>): PhotoElement {
  return {
    id: uid('ph'),
    kind: 'photo',
    rotation: 0,
    opacity: 1,
    asset: null,
    crop: { x: 0.5, y: 0.5, zoom: 1 },
    mask: { shape: 'rect', radius: 0 },
    border: null,
    frame: null,
    adjust: null,
    filter: null,
    shadow: null,
    ...p,
  };
}

export function createText(p: Partial<TextElement> & Pick<TextElement, 'x' | 'y' | 'w'>): TextElement {
  return {
    id: uid('tx'),
    kind: 'text',
    h: 100,
    rotation: 0,
    opacity: 1,
    text: 'Your text',
    font: 'playfair-display',
    size: 120,
    weight: 400,
    italic: false,
    fill: '#1d1b26',
    align: 'center',
    lineHeight: 1.2,
    letterSpacing: 0,
    stroke: null,
    background: null,
    curve: 0,
    shadow: null,
    ...p,
  };
}

export function createSticker(
  p: Partial<StickerElement> & Pick<StickerElement, 'x' | 'y' | 'w' | 'h' | 'sticker'>,
): StickerElement {
  return {
    id: uid('st'),
    kind: 'sticker',
    rotation: 0,
    opacity: 1,
    shadow: null,
    ...p,
  };
}

export function createShape(p: Partial<ShapeElement> & Pick<ShapeElement, 'x' | 'y' | 'w' | 'h'>): ShapeElement {
  return {
    id: uid('sh'),
    kind: 'shape',
    rotation: 0,
    opacity: 1,
    shape: 'rect',
    fill: { kind: 'solid', color: '#ff5d8f' },
    stroke: null,
    radius: 0,
    shadow: null,
    ...p,
  };
}

export function createCalendar(
  p: Partial<CalendarElement> & Pick<CalendarElement, 'x' | 'y' | 'w' | 'h'>,
): CalendarElement {
  const now = new Date();
  return {
    id: uid('cal'),
    kind: 'calendar',
    rotation: 0,
    opacity: 1,
    year: now.getFullYear(),
    month: now.getMonth() + 1,
    firstDay: 1,
    style: 'clean',
    titleFont: 'playfair-display',
    font: 'inter',
    color: '#1d1b26',
    accent: '#ff5d8f',
    weekendColor: '#8a8299',
    showTitle: true,
    showYear: true,
    marked: [],
    background: null,
    shadow: null,
    ...p,
  };
}

/** Deep clone with fresh ids (used by duplicate / paste). */
export function cloneElement<T extends CollageElement>(el: T, offset = 0): T {
  const copy = structuredClone(el) as T;
  copy.id = uid(el.id.split('_')[0] || 'e');
  copy.x += offset;
  copy.y += offset;
  if (copy.kind === 'photo') (copy as PhotoElement).slot = false;
  return copy;
}

export function shortSide(doc: CollageDoc): number {
  return Math.min(doc.page.width, doc.page.height);
}
