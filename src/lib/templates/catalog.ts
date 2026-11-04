/**
 * Built-in templates. Coordinates are fractions of the page (see types.ts).
 * Template artwork (stickers, patterns) is CC0, so collages made with them are
 * entirely the user's.
 */
import type { Background, Fill, PageFrame, PhotoFrame, Shadow } from '../model/types';
import type { ShapeDef, SlotDef, StickerDef, TemplateDef, TextDef } from './types';

/* ------------------------------------------------------------- helpers */

const SQUARE = { width: 3000, height: 3000, preset: 'square' };
const A4P = { width: 2480, height: 3508, preset: 'a4' };
const A4L = { width: 3508, height: 2480, preset: 'a4' };
const A3P = { width: 3508, height: 4961, preset: 'a3' };
const CARD_L = { width: 2100, height: 1500, preset: 'photo-5x7' };
const WIDE = { width: 3600, height: 2400, preset: 'photo-4x6' };

const solid = (color: string): Fill => ({ kind: 'solid', color });
const linear = (angle: number, ...colors: string[]): Fill => ({
  kind: 'linear',
  angle,
  stops: colors.map((color, i) => ({ offset: i / Math.max(1, colors.length - 1), color })),
});
const radial = (...colors: string[]): Fill => ({
  kind: 'radial',
  stops: colors.map((color, i) => ({ offset: i / Math.max(1, colors.length - 1), color })),
});

function bg(fill: Fill, extra: Partial<Background> = {}): Background {
  return { fill, pattern: null, texture: null, image: null, vignette: 0, ...extra };
}

const softShadow: Shadow = { color: 'rgba(30,20,40,0.28)', blur: 0.012, x: 0, y: 0.004 };
const deepShadow: Shadow = { color: 'rgba(20,10,30,0.4)', blur: 0.02, x: 0, y: 0.008 };
const textGlow = (color = 'rgba(0,0,0,0.35)'): Shadow => ({ color, blur: 0.006, x: 0, y: 0.002 });

const polaroid = (caption?: string, captionFont = 'caveat'): PhotoFrame => ({
  style: 'polaroid',
  color: '#ffffff',
  caption,
  captionFont,
  captionColor: '#3b3446',
});

function slot(x: number, y: number, w: number, h: number, extra: Partial<SlotDef> = {}): SlotDef {
  return { x, y, w, h, ...extra };
}

function text(t: string, cx: number, cy: number, w: number, font: string, size: number, color: string, extra: Partial<TextDef> = {}): TextDef {
  return { kind: 'text', text: t, cx, cy, w, font, size, color, ...extra };
}

function sticker(id: string, cx: number, cy: number, size: number, extra: Partial<StickerDef> = {}): StickerDef {
  return { kind: 'sticker', sticker: id, cx, cy, size, ...extra };
}

function shape(s: ShapeDef['shape'], x: number, y: number, w: number, h: number, fill: Fill | null, extra: Partial<ShapeDef> = {}): ShapeDef {
  return { kind: 'shape', shape: s, x, y, w, h, fill, ...extra };
}

const frame = (style: PageFrame['style'], color: string, width: number, inset: number): PageFrame => ({ style, color, width, inset });

/** Even grid of slots inside a region. */
function gridSlots(cols: number, rows: number, x: number, y: number, w: number, h: number, gapX: number, gapY: number, extra: Partial<SlotDef> = {}): SlotDef[] {
  const out: SlotDef[] = [];
  const cw = (w - gapX * (cols - 1)) / cols;
  const ch = (h - gapY * (rows - 1)) / rows;
  for (let r = 0; r < rows; r++) for (let c = 0; c < cols; c++) out.push(slot(x + c * (cw + gapX), y + r * (ch + gapY), cw, ch, extra));
  return out;
}

/* ----------------------------------------------------------- templates */

export const TEMPLATES: TemplateDef[] = [
  /* ================================================================ SIMPLE */
  {
    id: 'simple-four',
    name: 'Four Square',
    category: 'simple',
    description: 'Four equal photos with a crisp white border.',
    page: SQUARE,
    background: bg(solid('#ffffff')),
    slots: gridSlots(2, 2, 0.03, 0.03, 0.94, 0.94, 0.02, 0.02),
  },
  {
    id: 'simple-hero-three',
    name: 'Hero + Three',
    category: 'simple',
    description: 'One big photo and three supporting shots.',
    page: A4L,
    background: bg(solid('#ffffff')),
    slots: [
      slot(0.025, 0.035, 0.6, 0.93),
      slot(0.64, 0.035, 0.335, 0.297),
      slot(0.64, 0.352, 0.335, 0.296),
      slot(0.64, 0.668, 0.335, 0.297),
    ],
  },
  {
    id: 'simple-magazine',
    name: 'Magazine',
    category: 'simple',
    description: 'Editorial layout with a headline.',
    page: A4P,
    background: bg(solid('#f7f4ef')),
    slots: [slot(0.06, 0.05, 0.88, 0.45), slot(0.06, 0.52, 0.43, 0.3), slot(0.51, 0.52, 0.43, 0.3)],
    items: [
      text('Our Summer', 0.5, 0.875, 0.88, 'playfair-display', 0.1, '#1d1b26', { italic: true }),
      text('A COLLECTION OF FAVOURITE MOMENTS', 0.5, 0.94, 0.88, 'josefin-sans', 0.022, '#6b6478', { letterSpacing: 0.3 }),
    ],
  },
  {
    id: 'simple-filmstrip',
    name: 'Film Strip',
    category: 'simple',
    description: 'Four frames on a classic film strip.',
    page: { width: 3600, height: 1500, preset: undefined },
    background: bg(solid('#141218')),
    frame: frame('film', '#141218', 0.03, 0),
    slots: gridSlots(4, 1, 0.025, 0.2, 0.95, 0.6, 0.015, 0, { mask: { shape: 'rect', radius: 0.02 } }),
  },
  {
    id: 'simple-mosaic-poster',
    name: 'Mosaic Poster',
    category: 'simple',
    description: 'Any number of photos, fitted edge to edge, with a title.',
    page: A3P,
    background: bg(solid('#ffffff')),
    layout: { engine: 'mosaic', spacing: 0.008, margin: 0.03, region: { x: 0, y: 0, w: 1, h: 0.885 } },
    previewPhotos: 12,
    items: [
      text('Memories', 0.5, 0.925, 0.9, 'playfair-display', 0.075, '#1d1b26'),
      text('2026', 0.5, 0.968, 0.9, 'josefin-sans', 0.022, '#8a8396', { letterSpacing: 0.5 }),
    ],
  },
  {
    id: 'simple-polaroid-wall',
    name: 'Polaroid Table',
    category: 'simple',
    description: 'Casually scattered polaroids on a warm wooden-paper table.',
    page: A4L,
    background: bg(linear(90, '#c99a6b', '#b98559'), { texture: { id: 'kraft', intensity: 0.75 } }),
    layout: { engine: 'scatter', margin: 0.06 },
    previewPhotos: 7,
  },

  /* ================================================================== LOVE */
  {
    id: 'love-heart-mosaic',
    name: 'Heart of Us',
    category: 'love',
    description: 'Your photos form a big heart — perfect for a loved one.',
    page: SQUARE,
    background: bg(linear(135, '#ffe3ec', '#ffc2d4'), { pattern: { id: 'hearts', color: '#ffffff', opacity: 0.45, scale: 0.07 } }),
    layout: { engine: 'shape', shape: 'heart', spacing: 0.006, margin: 0.02, repeat: true, region: { x: 0.05, y: 0.04, w: 0.9, h: 0.76 } },
    photoStyle: { mask: { shape: 'rect', radius: 0.12 } },
    previewPhotos: 12,
    items: [
      text('You & Me', 0.5, 0.875, 0.9, 'great-vibes', 0.11, '#c2185b', { shadow: textGlow('rgba(255,255,255,0.8)') }),
      text('ALWAYS AND FOREVER', 0.5, 0.945, 0.8, 'josefin-sans', 0.024, '#ad4b6e', { letterSpacing: 0.4, weight: 700 }),
    ],
  },
  {
    id: 'love-be-mine',
    name: 'Be Mine',
    category: 'love',
    description: 'Valentine’s polaroids with hearts and a love letter.',
    page: A4P,
    background: bg(linear(160, '#ff5f7e', '#d81b60'), { pattern: { id: 'hearts', color: '#ffffff', opacity: 0.12, scale: 0.09 } }),
    frame: frame('stitched', '#ffffff', 0.004, 0.025),
    slots: [
      slot(0.07, 0.21, 0.44, 0.355, { rotation: -6, frame: polaroid('us ♥'), shadow: deepShadow }),
      slot(0.5, 0.27, 0.44, 0.355, { rotation: 5, frame: polaroid('always'), shadow: deepShadow }),
      slot(0.27, 0.6, 0.42, 0.335, { rotation: 3, frame: polaroid('forever'), shadow: deepShadow }),
    ],
    items: [
      text('Be Mine', 0.5, 0.12, 0.9, 'pacifico', 0.15, '#ffffff', { shadow: { color: 'rgba(90,0,30,0.45)', blur: 0.01, x: 0, y: 0.006 } }),
      sticker('heart-arrow', 0.82, 0.21, 0.2, { rotation: 12 }),
      sticker('love-letter', 0.82, 0.85, 0.22, { rotation: -10, shadow: softShadow }),
      sticker('heart-red', 0.12, 0.66, 0.12, { rotation: -14 }),
      sticker('heart-pink', 0.9, 0.62, 0.09, { rotation: 16 }),
      sticker('hearts-trio', 0.16, 0.9, 0.16),
    ],
  },
  {
    id: 'love-you-and-me',
    name: 'You & Me',
    category: 'love',
    description: 'Two portraits joined by a heart. Sweet and simple.',
    page: { width: 3600, height: 2400, preset: 'photo-4x6' },
    background: bg(solid('#fbf3ea'), { texture: { id: 'paper', intensity: 0.9 } }),
    slots: [
      slot(0.08, 0.12, 0.34, 0.51, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.012, color: '#ffffff' }, shadow: softShadow }),
      slot(0.58, 0.12, 0.34, 0.51, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.012, color: '#ffffff' }, shadow: softShadow }),
    ],
    items: [
      sticker('heart-red', 0.5, 0.37, 0.16, { shadow: softShadow }),
      text('you & me', 0.5, 0.78, 0.8, 'great-vibes', 0.15, '#b23a48'),
      text('SINCE 2021', 0.5, 0.9, 0.6, 'josefin-sans', 0.03, '#7c6a6d', { letterSpacing: 0.5, weight: 700 }),
      sticker('sparkle', 0.26, 0.08, 0.06, { opacity: 0.8 }),
      sticker('sparkle', 0.75, 0.68, 0.045, { opacity: 0.8 }),
    ],
  },
  {
    id: 'love-letter',
    name: 'Love Letter',
    category: 'love',
    description: 'Taped photos and handwritten notes on kraft paper.',
    page: { width: 2400, height: 3000, preset: undefined },
    background: bg(solid('#e2c7a1'), { texture: { id: 'kraft', intensity: 0.8 } }),
    slots: [
      slot(0.08, 0.2, 0.5, 0.33, { rotation: -4, frame: { style: 'tape', color: '#f7a8c4' }, border: { width: 0.012, color: '#ffffff' }, shadow: softShadow }),
      slot(0.45, 0.42, 0.47, 0.3, { rotation: 5, frame: { style: 'tape', color: '#9ed9cf' }, border: { width: 0.012, color: '#ffffff' }, shadow: softShadow }),
      slot(0.1, 0.68, 0.38, 0.24, { rotation: -2, frame: { style: 'tape', color: '#ffd27a' }, border: { width: 0.012, color: '#ffffff' }, shadow: softShadow }),
    ],
    items: [
      text('Dear you,', 0.3, 0.09, 0.5, 'caveat', 0.09, '#5b2333', { align: 'left' }),
      text('I love you to the moon\nand back ♥', 0.72, 0.84, 0.5, 'caveat', 0.055, '#5b2333', { rotation: -3 }),
      sticker('rose', 0.83, 0.18, 0.2, { rotation: 18 }),
      sticker('heart-outline', 0.7, 0.32, 0.08, { rotation: -10 }),
      sticker('lips', 0.62, 0.95, 0.1, { rotation: 12, opacity: 0.9 }),
    ],
  },
  {
    id: 'love-anniversary',
    name: 'Anniversary',
    category: 'love',
    description: 'Elegant burgundy and gold for a special date.',
    page: SQUARE,
    background: bg(radial('#6b1530', '#3d0a1b')),
    frame: frame('gold', '#c9a227', 0.004, 0.025),
    slots: [
      slot(0.3, 0.13, 0.4, 0.5, { mask: { shape: 'arch', radius: 0 }, border: { width: 0.006, color: '#e2c275' } }),
      slot(0.09, 0.4, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.005, color: '#e2c275' } }),
      slot(0.71, 0.4, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.005, color: '#e2c275' } }),
    ],
    items: [
      text('Happy Anniversary', 0.5, 0.75, 0.9, 'great-vibes', 0.1, '#f1d48b'),
      text('TEN YEARS OF US', 0.5, 0.84, 0.8, 'cinzel', 0.032, '#e8cfa0', { letterSpacing: 0.3 }),
      sticker('diamond-ring', 0.5, 0.92, 0.07),
      sticker('sparkle', 0.2, 0.2, 0.05),
      sticker('sparkle', 0.82, 0.24, 0.04),
    ],
  },
  {
    id: 'love-our-story',
    name: 'Our Story',
    category: 'love',
    description: 'A little timeline of your favourite moments together.',
    page: A4L,
    background: bg(linear(180, '#fff1f3', '#ffe1e7')),
    slots: [
      slot(0.05, 0.3, 0.205, 0.46, { rotation: -3, frame: polaroid('first date'), shadow: softShadow }),
      slot(0.285, 0.34, 0.205, 0.46, { rotation: 2, frame: polaroid('first trip'), shadow: softShadow }),
      slot(0.52, 0.29, 0.205, 0.46, { rotation: -2, frame: polaroid('the yes!'), shadow: softShadow }),
      slot(0.755, 0.33, 0.205, 0.46, { rotation: 3, frame: polaroid('forever'), shadow: softShadow }),
    ],
    items: [
      shape('rect', 0.03, 0.535, 0.94, 0.004, solid('#e8a0b4'), { below: true }),
      text('Our Story', 0.5, 0.13, 0.8, 'playfair-display', 0.11, '#7a2140', { italic: true }),
      sticker('heart-pink', 0.5, 0.9, 0.06),
      sticker('heart-red', 0.43, 0.92, 0.035),
      sticker('heart-red', 0.57, 0.92, 0.035),
    ],
  },
  {
    id: 'love-sweetheart-collage',
    name: 'Sweetheart',
    category: 'love',
    description: 'A soft mosaic of every photo of the two of you.',
    page: A4P,
    background: bg(solid('#fff6f8')),
    frame: frame('hearts', '#ff7aa2', 0.012, 0.024),
    layout: { engine: 'mosaic', spacing: 0.01, margin: 0.07, region: { x: 0, y: 0.13, w: 1, h: 0.87 } },
    photoStyle: { mask: { shape: 'rect', radius: 0.04 } },
    previewPhotos: 8,
    items: [text('For my love', 0.5, 0.09, 0.9, 'dancing-script', 0.1, '#d6336c', { weight: 700 })],
  },

  /* ============================================================= CHRISTMAS */
  {
    id: 'xmas-merry',
    name: 'Merry Christmas',
    category: 'christmas',
    description: 'Classic red with snowflakes, candy-cane border and festive stickers.',
    page: SQUARE,
    background: bg(radial('#c62828', '#8e1414'), { pattern: { id: 'snowflakes', color: '#ffffff', opacity: 0.14, scale: 0.1 } }),
    frame: frame('candy', '#d7263d', 0.012, 0),
    slots: gridSlots(2, 2, 0.15, 0.27, 0.7, 0.58, 0.025, 0.025, { border: { width: 0.008, color: '#ffffff' }, shadow: softShadow }),
    items: [
      text('Merry Christmas', 0.5, 0.15, 0.9, 'mountains-of-christmas', 0.12, '#ffffff', { weight: 700, shadow: textGlow('rgba(60,0,0,0.5)') }),
      sticker('christmas-tree', 0.1, 0.8, 0.17),
      sticker('gift-box', 0.9, 0.86, 0.13, { rotation: 6 }),
      sticker('holly', 0.88, 0.12, 0.12, { rotation: 12 }),
      sticker('ornament-red', 0.1, 0.2, 0.09, { rotation: -8 }),
      sticker('candy-cane', 0.92, 0.5, 0.08, { rotation: 18 }),
      text('& a Happy New Year', 0.5, 0.92, 0.7, 'dancing-script', 0.05, '#ffe9a8', { weight: 700 }),
    ],
  },
  {
    id: 'xmas-winter-wonderland',
    name: 'Winter Wonderland',
    category: 'christmas',
    description: 'Icy blues, snowflakes and round photo frames.',
    page: A4P,
    background: bg(linear(90, '#e8f6ff', '#a9d1f2'), { pattern: { id: 'snowflakes', color: '#ffffff', opacity: 0.55, scale: 0.12 } }),
    frame: frame('snow', '#ffffff', 0.014, 0.028),
    slots: [
      slot(0.2, 0.2, 0.6, 0.4247, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.01, color: '#ffffff' }, shadow: softShadow }),
      slot(0.07, 0.6, 0.27, 0.191, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.008, color: '#ffffff' }, shadow: softShadow }),
      slot(0.365, 0.66, 0.27, 0.191, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.008, color: '#ffffff' }, shadow: softShadow }),
      slot(0.66, 0.6, 0.27, 0.191, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.008, color: '#ffffff' }, shadow: softShadow }),
    ],
    items: [
      text('Winter Wonderland', 0.5, 0.11, 0.92, 'great-vibes', 0.11, '#23466e'),
      sticker('snowman', 0.84, 0.9, 0.16),
      sticker('snowflake-1', 0.13, 0.9, 0.1, { rotation: 10 }),
      sticker('snowflake-2', 0.88, 0.2, 0.08),
    ],
  },
  {
    id: 'xmas-family-card',
    name: 'Family Christmas Card',
    category: 'christmas',
    description: 'A 7×5 card: big family photo, evergreen panel, gold script.',
    page: CARD_L,
    background: bg(linear(90, '#1b4d32', '#123824')),
    slots: [slot(0, 0, 0.6, 1)],
    items: [
      text('Merry\nChristmas', 0.8, 0.38, 0.38, 'great-vibes', 0.2, '#f1d48b', { lineHeight: 1.0 }),
      text('FROM OUR FAMILY TO YOURS', 0.8, 0.66, 0.36, 'josefin-sans', 0.035, '#e8efe9', { letterSpacing: 0.25, weight: 700 }),
      text('with love, the Family', 0.8, 0.8, 0.36, 'dancing-script', 0.06, '#ffffff'),
      sticker('holly', 0.8, 0.12, 0.16),
      sticker('star-gold', 0.95, 0.9, 0.08, { rotation: 10 }),
    ],
  },
  {
    id: 'xmas-cozy-mosaic',
    name: 'Cozy Holidays',
    category: 'christmas',
    description: 'All your holiday photos in a cosy gingham mosaic.',
    page: A4L,
    background: bg(solid('#f4ede1'), { pattern: { id: 'gingham', color: '#c0392b', opacity: 0.16, scale: 0.05 } }),
    layout: { engine: 'mosaic', spacing: 0.01, margin: 0.05, region: { x: 0.3, y: 0, w: 0.7, h: 1 } },
    photoStyle: { border: { width: 0.006, color: '#ffffff' }, shadow: softShadow },
    previewPhotos: 7,
    items: [
      text('Happy\nHolidays', 0.165, 0.38, 0.3, 'pacifico', 0.1, '#b0222b', { lineHeight: 1.1 }),
      text('2026', 0.165, 0.58, 0.28, 'josefin-sans', 0.04, '#7a5a43', { letterSpacing: 0.6, weight: 700 }),
      sticker('gingerbread-man', 0.1, 0.82, 0.12, { rotation: -10 }),
      sticker('stocking', 0.24, 0.84, 0.1, { rotation: 8 }),
      sticker('bell', 0.07, 0.12, 0.08),
      sticker('mistletoe', 0.25, 0.1, 0.1),
    ],
  },

  /* ============================================================== NEW YEAR */
  {
    id: 'ny-sparkle',
    name: 'Happy New Year',
    category: 'newyear',
    description: 'Midnight navy with gold confetti and fireworks.',
    page: SQUARE,
    background: bg(radial('#1d2b5c', '#0a1230'), { pattern: { id: 'confetti', color: '#e7c35a', opacity: 0.55, scale: 0.08 } }),
    frame: frame('gold', '#c9a227', 0.004, 0.025),
    layout: { engine: 'mosaic', spacing: 0.008, margin: 0.02, region: { x: 0.08, y: 0.27, w: 0.84, h: 0.62 } },
    photoStyle: { border: { width: 0.004, color: '#e7c35a' } },
    previewPhotos: 6,
    items: [
      text('Happy New Year', 0.5, 0.13, 0.9, 'great-vibes', 0.11, '#f3d27a'),
      text('2027', 0.5, 0.225, 0.6, 'abril-fatface', 0.06, '#ffffff', { letterSpacing: 0.3 }),
      sticker('firework-1', 0.12, 0.11, 0.14),
      sticker('firework-2', 0.88, 0.12, 0.12),
      sticker('champagne-glasses', 0.86, 0.92, 0.12, { rotation: 6 }),
      sticker('clock-midnight', 0.13, 0.93, 0.09),
    ],
  },

  /* ============================================================== BIRTHDAY */
  {
    id: 'bday-bash',
    name: 'Birthday Bash',
    category: 'birthday',
    description: 'Confetti, bunting and balloons around your party photos.',
    page: A4L,
    background: bg(solid('#fff6dc'), { pattern: { id: 'confetti', color: 'multi', opacity: 0.55, scale: 0.09 } }),
    layout: { engine: 'mosaic', spacing: 0.012, margin: 0.02, region: { x: 0.04, y: 0.27, w: 0.72, h: 0.7 } },
    photoStyle: { mask: { shape: 'rect', radius: 0.05 }, border: { width: 0.008, color: '#ffffff' }, shadow: softShadow },
    previewPhotos: 6,
    items: [
      sticker('bunting', 0.5, 0.06, 0.95),
      text('Happy Birthday!', 0.42, 0.18, 0.8, 'shrikhand', 0.1, '#ff4f81', { stroke: { width: 0.08, color: '#ffffff' }, shadow: softShadow }),
      sticker('balloon-bunch', 0.88, 0.45, 0.3, { rotation: 6 }),
      sticker('birthday-cake', 0.88, 0.84, 0.2),
      sticker('party-hat', 0.76, 0.2, 0.1, { rotation: 18 }),
    ],
  },
  {
    id: 'bday-little-one',
    name: 'Little One Turns 3',
    category: 'birthday',
    description: 'Playful circles and a big number for a kid’s birthday.',
    page: SQUARE,
    background: bg(linear(90, '#d9f1ff', '#b3e0ff'), { pattern: { id: 'polka', color: '#ffffff', opacity: 0.6, scale: 0.06 } }),
    layout: { engine: 'ring', spacing: 0.012, margin: 0.02, region: { x: 0.12, y: 0.18, w: 0.76, h: 0.76 } },
    photoStyle: { border: { width: 0.008, color: '#ffffff' }, shadow: softShadow },
    previewPhotos: 7,
    items: [
      text('Happy 3rd Birthday', 0.5, 0.09, 0.9, 'fredoka', 0.085, '#ff6f61', { weight: 600, stroke: { width: 0.09, color: '#ffffff' } }),
      sticker('balloon-red', 0.08, 0.3, 0.12, { rotation: -10 }),
      sticker('balloon-blue', 0.92, 0.32, 0.12, { rotation: 8 }),
      sticker('cupcake', 0.9, 0.9, 0.12),
      sticker('present-stack', 0.1, 0.9, 0.13),
    ],
  },

  /* ================================================================ FAMILY */
  {
    id: 'family-home',
    name: 'Family',
    category: 'family',
    description: 'Warm, timeless grid for family photos.',
    page: A4P,
    background: bg(solid('#f6efe6'), { texture: { id: 'paper', intensity: 0.8 } }),
    slots: gridSlots(2, 3, 0.08, 0.25, 0.84, 0.69, 0.025, 0.018, { mask: { shape: 'rect', radius: 0.05 } }),
    items: [
      text('Family', 0.5, 0.1, 0.8, 'playfair-display', 0.14, '#3e2f28'),
      text('where life begins and love never ends', 0.5, 0.185, 0.84, 'lora', 0.032, '#7b6152', { italic: true }),
    ],
  },
  {
    id: 'family-mothers-day',
    name: 'Happy Mother’s Day',
    category: 'family',
    description: 'Pastel arches and flowers for Mom.',
    page: A4P,
    background: bg(linear(90, '#ffeef0', '#ffd9cf')),
    slots: [
      slot(0.08, 0.22, 0.41, 0.42, { mask: { shape: 'arch', radius: 0 }, shadow: softShadow }),
      slot(0.51, 0.22, 0.41, 0.42, { mask: { shape: 'arch', radius: 0 }, shadow: softShadow }),
      slot(0.08, 0.66, 0.84, 0.25, { mask: { shape: 'rect', radius: 0.08 }, shadow: softShadow }),
    ],
    items: [
      text('Happy Mother’s Day', 0.5, 0.1, 0.92, 'dancing-script', 0.1, '#c2456b', { weight: 700 }),
      text('THANK YOU FOR EVERYTHING, MOM', 0.5, 0.165, 0.84, 'josefin-sans', 0.024, '#9c6b7a', { letterSpacing: 0.3, weight: 700 }),
      sticker('flower-bouquet', 0.88, 0.9, 0.18, { rotation: 10 }),
      sticker('tulip', 0.1, 0.94, 0.1, { rotation: -12 }),
      sticker('butterfly', 0.9, 0.06, 0.08, { rotation: 15 }),
    ],
  },
  {
    id: 'family-best-dad',
    name: 'Best Dad Ever',
    category: 'family',
    description: 'Bold navy & tan design for Father’s Day.',
    page: A4L,
    background: bg(solid('#1f2a44')),
    slots: [
      slot(0.04, 0.06, 0.48, 0.88, { border: { width: 0.008, color: '#d9b77e' } }),
      slot(0.55, 0.5, 0.2, 0.44, { border: { width: 0.006, color: '#d9b77e' } }),
      slot(0.77, 0.5, 0.19, 0.44, { border: { width: 0.006, color: '#d9b77e' } }),
    ],
    items: [
      text('BEST DAD', 0.755, 0.17, 0.42, 'bebas-neue', 0.14, '#d9b77e', { letterSpacing: 0.08 }),
      text('EVER', 0.755, 0.31, 0.42, 'bebas-neue', 0.1, '#ffffff', { letterSpacing: 0.6 }),
      sticker('trophy', 0.94, 0.08, 0.09),
    ],
  },

  /* =============================================================== WEDDING */
  {
    id: 'wedding-just-married',
    name: 'Just Married',
    category: 'wedding',
    description: 'Ivory, gold and graceful script.',
    page: A4P,
    background: bg(solid('#fbf8f2'), { texture: { id: 'paper', intensity: 0.7 } }),
    frame: frame('gold', '#c9a227', 0.003, 0.03),
    slots: [
      slot(0.22, 0.1, 0.56, 0.47, { mask: { shape: 'arch', radius: 0 } }),
      slot(0.1, 0.62, 0.36, 0.24, { mask: { shape: 'rect', radius: 0.03 } }),
      slot(0.54, 0.62, 0.36, 0.24, { mask: { shape: 'rect', radius: 0.03 } }),
    ],
    items: [
      text('Just Married', 0.5, 0.9, 0.9, 'great-vibes', 0.1, '#a8813a'),
      text('06 · 14 · 2026', 0.5, 0.955, 0.6, 'cinzel', 0.025, '#7c6a4f', { letterSpacing: 0.3 }),
      sticker('dove', 0.85, 0.12, 0.1, { rotation: -8 }),
      sticker('wedding-rings', 0.5, 0.585, 0.08),
    ],
  },

  /* ================================================================== BABY */
  {
    id: 'baby-welcome',
    name: 'Welcome Baby',
    category: 'baby',
    description: 'Soft pastels, stars and a cloud-shaped portrait.',
    page: SQUARE,
    background: bg(linear(90, '#eef7f3', '#dff0ff'), { pattern: { id: 'stars', color: '#ffffff', opacity: 0.8, scale: 0.07 } }),
    slots: [
      slot(0.22, 0.2, 0.56, 0.42, { mask: { shape: 'cloud', radius: 0 }, shadow: softShadow }),
      slot(0.08, 0.66, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.006, color: '#ffffff' } }),
      slot(0.29, 0.7, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.006, color: '#ffffff' } }),
      slot(0.51, 0.7, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.006, color: '#ffffff' } }),
      slot(0.72, 0.66, 0.2, 0.2, { mask: { shape: 'circle', radius: 0 }, border: { width: 0.006, color: '#ffffff' } }),
    ],
    items: [
      text('Welcome to the world', 0.5, 0.09, 0.9, 'sacramento', 0.1, '#5a7fa6'),
      text('Baby Name', 0.5, 0.945, 0.8, 'comfortaa', 0.045, '#6f8f86', { weight: 700 }),
      sticker('moon-stars', 0.86, 0.22, 0.12),
      sticker('stork-bundle', 0.12, 0.24, 0.12, { rotation: -8 }),
      sticker('footprints', 0.88, 0.52, 0.08, { rotation: 12 }),
    ],
  },

  /* ================================================================ TRAVEL */
  {
    id: 'travel-postcard',
    name: 'Greetings From',
    category: 'travel',
    description: 'A retro postcard for your trip.',
    page: WIDE,
    background: bg(linear(90, '#7fd3ff', '#ffe9b3')),
    slots: [
      slot(0.04, 0.42, 0.22, 0.52, { border: { width: 0.008, color: '#ffffff' }, rotation: -3, shadow: softShadow }),
      slot(0.28, 0.45, 0.22, 0.5, { border: { width: 0.008, color: '#ffffff' }, rotation: 2, shadow: softShadow }),
      slot(0.52, 0.42, 0.22, 0.52, { border: { width: 0.008, color: '#ffffff' }, rotation: -2, shadow: softShadow }),
      slot(0.76, 0.45, 0.2, 0.5, { border: { width: 0.008, color: '#ffffff' }, rotation: 3, shadow: softShadow }),
    ],
    items: [
      text('GREETINGS FROM', 0.5, 0.11, 0.8, 'bebas-neue', 0.1, '#1e3a5f', { letterSpacing: 0.15 }),
      text('Paradise', 0.5, 0.27, 0.9, 'pacifico', 0.17, '#ff6b4a', { stroke: { width: 0.07, color: '#ffffff' }, shadow: softShadow }),
      sticker('airplane', 0.1, 0.18, 0.13, { rotation: -12 }),
      sticker('postage-stamp', 0.9, 0.17, 0.14, { rotation: 8 }),
      sticker('palm-tree', 0.97, 0.85, 0.2, { rotation: 0 }),
    ],
  },
  {
    id: 'travel-adventure',
    name: 'Adventure Awaits',
    category: 'travel',
    description: 'Scrapbook-style travel journal with polaroids.',
    page: A4P,
    background: bg(solid('#dcc19b'), { texture: { id: 'kraft', intensity: 0.85 } }),
    layout: { engine: 'scatter', margin: 0.06, region: { x: 0, y: 0.16, w: 1, h: 0.84 } },
    previewPhotos: 6,
    items: [
      text('Adventure awaits', 0.5, 0.085, 0.92, 'permanent-marker', 0.09, '#2f3b2f', { rotation: -2 }),
      sticker('compass', 0.88, 0.08, 0.12, { rotation: 10 }),
      sticker('camera', 0.12, 0.08, 0.12, { rotation: -10 }),
      sticker('washi-tape-mint', 0.5, 0.15, 0.36, { rotation: 2, opacity: 0.9 }),
    ],
  },
  {
    id: 'travel-summer-vibes',
    name: 'Summer Vibes',
    category: 'travel',
    description: 'Sunny gradient, waves and beach stickers.',
    page: SQUARE,
    background: bg(linear(90, '#ffd36e', '#ff7a59'), { pattern: { id: 'waves', color: '#ffffff', opacity: 0.18, scale: 0.08 } }),
    layout: { engine: 'mosaic', spacing: 0.012, margin: 0.02, region: { x: 0.06, y: 0.2, w: 0.88, h: 0.68 } },
    photoStyle: { mask: { shape: 'rect', radius: 0.06 }, border: { width: 0.006, color: '#ffffff' } },
    previewPhotos: 5,
    items: [
      text('Summer Vibes', 0.5, 0.1, 0.9, 'pacifico', 0.11, '#ffffff', { shadow: { color: 'rgba(150,50,0,0.4)', blur: 0.008, x: 0, y: 0.004 } }),
      sticker('sun', 0.9, 0.09, 0.13),
      sticker('palm-tree', 0.08, 0.9, 0.16),
      sticker('ice-cream', 0.92, 0.92, 0.1, { rotation: 10 }),
      sticker('seashell', 0.5, 0.95, 0.07),
    ],
  },

  /* ============================================================== SEASONAL */
  {
    id: 'season-halloween',
    name: 'Spooky Season',
    category: 'seasonal',
    description: 'Pumpkins, bats and a haunted glow.',
    page: SQUARE,
    background: bg(radial('#3a1d4f', '#120a1a'), { vignette: 0.4 }),
    slots: gridSlots(2, 2, 0.12, 0.24, 0.76, 0.62, 0.03, 0.03, { border: { width: 0.008, color: '#ff8c1a' }, mask: { shape: 'rect', radius: 0.04 } }),
    items: [
      text('Happy Halloween', 0.5, 0.12, 0.9, 'creepster', 0.11, '#ff8c1a', { shadow: { color: 'rgba(255,140,26,0.6)', blur: 0.012, x: 0, y: 0 } }),
      sticker('spider-web', 0.09, 0.09, 0.18),
      sticker('bat', 0.88, 0.08, 0.13, { rotation: 10 }),
      sticker('pumpkin', 0.12, 0.92, 0.15),
      sticker('ghost', 0.9, 0.9, 0.12, { rotation: -6 }),
      sticker('crescent-moon-spooky', 0.5, 0.94, 0.08),
    ],
  },
  {
    id: 'season-easter',
    name: 'Happy Easter',
    category: 'seasonal',
    description: 'Pastel spring with bunnies and eggs.',
    page: A4P,
    background: bg(linear(90, '#fffbe6', '#e7f7d4'), { pattern: { id: 'polka', color: '#ffd6e5', opacity: 0.7, scale: 0.06 } }),
    slots: [
      slot(0.08, 0.2, 0.84, 0.4, { mask: { shape: 'rect', radius: 0.06 }, border: { width: 0.008, color: '#ffffff' } }),
      slot(0.08, 0.63, 0.41, 0.28, { mask: { shape: 'arch', radius: 0 }, border: { width: 0.008, color: '#ffffff' } }),
      slot(0.51, 0.63, 0.41, 0.28, { mask: { shape: 'arch', radius: 0 }, border: { width: 0.008, color: '#ffffff' } }),
    ],
    items: [
      text('Happy Easter', 0.5, 0.1, 0.9, 'dancing-script', 0.12, '#7c4dff', { weight: 700 }),
      sticker('bunny', 0.88, 0.92, 0.16),
      sticker('easter-egg-1', 0.1, 0.94, 0.08, { rotation: -14 }),
      sticker('easter-egg-2', 0.2, 0.95, 0.07, { rotation: 10 }),
      sticker('chick', 0.86, 0.12, 0.1),
      sticker('tulip', 0.13, 0.12, 0.09, { rotation: -10 }),
    ],
  },
  {
    id: 'season-thankful',
    name: 'Grateful & Thankful',
    category: 'seasonal',
    description: 'Autumn leaves and warm tones.',
    page: A4L,
    background: bg(linear(135, '#f7e2c4', '#efc58f'), { texture: { id: 'paper', intensity: 0.8 } }),
    layout: { engine: 'mosaic', spacing: 0.01, margin: 0.03, region: { x: 0.33, y: 0.05, w: 0.64, h: 0.9 } },
    photoStyle: { mask: { shape: 'rect', radius: 0.04 }, border: { width: 0.006, color: '#fffaf2' } },
    previewPhotos: 6,
    items: [
      text('Grateful\n& Thankful', 0.17, 0.42, 0.3, 'amatic-sc', 0.13, '#8a3b12', { weight: 700, lineHeight: 1 }),
      sticker('maple-leaf', 0.08, 0.12, 0.12, { rotation: -20 }),
      sticker('acorn', 0.26, 0.15, 0.07, { rotation: 15 }),
      sticker('sunflower', 0.1, 0.84, 0.14),
      sticker('leaf-green', 0.26, 0.86, 0.08, { rotation: 40 }),
    ],
  },

  /* =============================================================== FRIENDS */
  {
    id: 'friends-besties',
    name: 'Besties',
    category: 'friends',
    description: 'Hexagon mosaic with sparkles for your crew.',
    page: SQUARE,
    background: bg(linear(135, '#e9d8ff', '#ffd6f0'), { pattern: { id: 'sparkles', color: '#ffffff', opacity: 0.7, scale: 0.08 } }),
    layout: { engine: 'shape', shape: 'hexagon', spacing: 0.008, margin: 0.02, repeat: true, region: { x: 0.06, y: 0.16, w: 0.88, h: 0.8 } },
    photoStyle: { mask: { shape: 'circle', radius: 0 } },
    previewPhotos: 10,
    items: [
      text('besties', 0.5, 0.09, 0.8, 'righteous', 0.11, '#7b2ff7', { stroke: { width: 0.08, color: '#ffffff' } }),
      sticker('crown', 0.79, 0.06, 0.09, { rotation: 18 }),
      sticker('heart-eyes', 0.1, 0.9, 0.1),
      sticker('sparkles-cluster', 0.9, 0.92, 0.1),
    ],
  },

  /* ================================================================ SCHOOL */
  {
    id: 'school-class-of',
    name: 'Class of 2026',
    category: 'school',
    description: 'Black and gold graduation announcement.',
    page: A4L,
    background: bg(radial('#24242c', '#0d0d10'), { pattern: { id: 'confetti', color: '#d4af37', opacity: 0.35, scale: 0.1 } }),
    slots: [
      slot(0.04, 0.07, 0.42, 0.86, { border: { width: 0.006, color: '#d4af37' } }),
      slot(0.5, 0.52, 0.145, 0.41, { border: { width: 0.004, color: '#d4af37' } }),
      slot(0.6575, 0.52, 0.145, 0.41, { border: { width: 0.004, color: '#d4af37' } }),
      slot(0.815, 0.52, 0.145, 0.41, { border: { width: 0.004, color: '#d4af37' } }),
    ],
    items: [
      text('CLASS OF', 0.73, 0.12, 0.46, 'josefin-sans', 0.05, '#ffffff', { letterSpacing: 0.6, weight: 700 }),
      text('2026', 0.73, 0.27, 0.46, 'abril-fatface', 0.17, '#e3c062'),
      text('Congratulations, Grad!', 0.73, 0.42, 0.46, 'great-vibes', 0.075, '#ffffff'),
      sticker('grad-cap', 0.93, 0.1, 0.11, { rotation: 14 }),
    ],
  },

  /* ================================================================== PETS */
  {
    id: 'pets-furry-friend',
    name: 'Furry Best Friend',
    category: 'pets',
    description: 'Rounded photos and paw prints for your pet.',
    page: SQUARE,
    background: bg(solid('#fff3e3'), { pattern: { id: 'dots', color: '#f2c99b', opacity: 0.8, scale: 0.05 } }),
    slots: gridSlots(2, 2, 0.08, 0.22, 0.84, 0.66, 0.03, 0.03, { mask: { shape: 'squircle', radius: 0 }, shadow: softShadow }),
    items: [
      text('My Furry Best Friend', 0.5, 0.11, 0.92, 'fredoka', 0.08, '#8a4b14', { weight: 600 }),
      sticker('paw-print', 0.1, 0.94, 0.08, { rotation: -20 }),
      sticker('bone', 0.88, 0.94, 0.12, { rotation: 15 }),
      sticker('dog-face', 0.9, 0.06, 0.08),
      sticker('cat-face', 0.1, 0.06, 0.08),
    ],
  },

  /* ======================================================== YEAR IN REVIEW */
  {
    id: 'simple-year-in-review',
    name: 'Year in Review',
    category: 'simple',
    description: 'Twelve months, twelve favourite photos.',
    page: A3P,
    background: bg(solid('#ffffff')),
    slots: gridSlots(3, 4, 0.06, 0.13, 0.88, 0.8, 0.025, 0.035, { mask: { shape: 'rect', radius: 0.02 } }),
    items: [
      text('2026', 0.5, 0.065, 0.6, 'bebas-neue', 0.1, '#1d1b26', { letterSpacing: 0.4 }),
      ...['JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN', 'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC'].map((m, i) => {
        const cw = (0.88 - 0.025 * 2) / 3;
        const ch = (0.8 - 0.035 * 3) / 4;
        const c = i % 3;
        const r = Math.floor(i / 3);
        return text(m, 0.06 + c * (cw + 0.025) + cw / 2, 0.13 + r * (ch + 0.035) + ch + 0.012, cw, 'josefin-sans', 0.018, '#6b6478', {
          letterSpacing: 0.4,
          weight: 700,
        });
      }),
    ],
  },
];

export function templateById(id: string): TemplateDef | undefined {
  return TEMPLATES.find((t) => t.id === id);
}
