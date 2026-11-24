// Tessera's text styles, shared by the collage editor (Text panel), the story canvas
// (text items) and the linear story view (section titles).
//
// A style is a `TextLook`: everything except the size, with outline, shadow and pill
// measured in *em* (fractions of the font size). That makes one definition usable at any
// size: the collage turns it into page units (`presetElement`), the HTML views turn it
// into CSS (`lookCss`), where `em` scales by itself.
import { fontInfo, resolveWeight } from '../assets/fonts';
import type { TextElement } from '../model/types';

export interface TextLook {
  /** font id from the font registry (src/lib/assets/fontList.ts) */
  font: string;
  weight?: number;
  italic?: boolean;
  fill?: string;
  /** outline width (visible outside the glyphs) in em */
  stroke?: { width: number; color: string } | null;
  /** offset and blur in em */
  shadow?: { x: number; y: number; blur: number; color: string } | null;
  uppercase?: boolean;
  /** extra space between letters, em */
  letterSpacing?: number;
  /** rounded pill behind the text; padding and radius in em */
  background?: { color: string; padding: number; radius: number } | null;
}

export interface TextPreset {
  id: string;
  label: string;
  /** what the style menu shows */
  sample: string;
  /** text of a new collage element (defaults to the sample) */
  text?: string;
  /** collage: size as a fraction of the page's short side */
  size: number;
  look: TextLook;
  /** collage only: arc amount -1..1 */
  curve?: number;
  /** the fill is just dark ink: views with light and dark themes use the theme's text colour */
  themeInk?: boolean;
}

export const TEXT_PRESETS: TextPreset[] = [
  { id: 'heading', label: 'Heading', sample: 'Add a heading', size: 0.09, look: { font: 'playfair-display', weight: 700 } },
  { id: 'subheading', label: 'Subheading', sample: 'Add a subheading', size: 0.05, look: { font: 'montserrat', weight: 600 } },
  { id: 'body', label: 'Body', sample: 'Add a little bit of body text', size: 0.03, look: { font: 'inter', weight: 400 } },
  { id: 'script', label: 'Script', sample: 'With love', size: 0.1, look: { font: 'great-vibes', fill: '#c2185b' } },
  { id: 'handwritten', label: 'Handwritten', sample: 'best day ever!', size: 0.07, themeInk: true, look: { font: 'caveat', weight: 700, fill: '#2b2340' } },
  { id: 'retro', label: 'Retro', sample: 'Summer', size: 0.1, look: { font: 'pacifico', fill: '#ff6b4a', stroke: { width: 0, color: '#ffffff' } } },
  // Classic meme lettering: white Anton with a black outline (8.5 % of the size outside the glyphs).
  { id: 'meme', label: 'Meme', sample: 'TOP TEXT', size: 0.1, look: { font: 'anton', uppercase: true, fill: '#ffffff', stroke: { width: 0.085, color: '#000000' } } },
  { id: 'poster', label: 'Poster', sample: 'GOOD TIMES', size: 0.1, themeInk: true, look: { font: 'bebas-neue', letterSpacing: 0.08, fill: '#1d1b26' } },
  { id: 'bubbly', label: 'Bubbly', sample: 'Party time', size: 0.08, look: { font: 'fredoka', weight: 600, fill: '#7b2ff7' } },
  { id: 'marker', label: 'Marker', sample: 'Adventure!', size: 0.07, themeInk: true, look: { font: 'permanent-marker', fill: '#2f3b2f' } },
  { id: 'elegant', label: 'Elegant caps', sample: 'FOREVER', size: 0.06, look: { font: 'cinzel', weight: 700, letterSpacing: 0.25, fill: '#a8813a' } },
  {
    id: 'label',
    label: 'Label',
    sample: 'SALE · NEW · WOW',
    text: 'BEST FRIENDS',
    size: 0.035,
    look: { font: 'montserrat', weight: 800, fill: '#ffffff', background: { color: '#ff5d8f', padding: 0.45, radius: 1 } },
  },
  { id: 'arched', label: 'Arched', sample: 'Happy Birthday', size: 0.07, curve: 0.35, look: { font: 'shrikhand', fill: '#ff4f81' } },
  { id: 'christmas', label: 'Christmas', sample: 'Merry Christmas', size: 0.08, look: { font: 'mountains-of-christmas', weight: 700, fill: '#c62828' } },
  { id: 'spooky', label: 'Spooky', sample: 'Boo!', size: 0.1, look: { font: 'creepster', fill: '#ff8c1a' } },
];

const byId = new Map(TEXT_PRESETS.map((p) => [p.id, p]));

export function textPreset(id: string | undefined): TextPreset | undefined {
  return id ? byId.get(id) : undefined;
}

/** The collage text element for a preset at `size` page units (`pageShort` converts shadows). */
export function presetElement(p: TextPreset, size: number, pageShort: number): Partial<TextElement> {
  const l = p.look;
  const el: Partial<TextElement> = { font: l.font, text: p.text ?? p.sample, size };
  if (l.weight) el.weight = l.weight;
  if (l.italic) el.italic = true;
  if (l.fill) el.fill = l.fill;
  if (l.uppercase) el.uppercase = true;
  if (l.letterSpacing) el.letterSpacing = l.letterSpacing;
  if (l.stroke) el.stroke = { width: l.stroke.width * size, color: l.stroke.color };
  if (l.background) el.background = { ...l.background };
  if (l.shadow && pageShort > 0)
    el.shadow = { color: l.shadow.color, blur: (l.shadow.blur * size) / pageShort, x: (l.shadow.x * size) / pageShort, y: (l.shadow.y * size) / pageShort };
  if (p.curve) el.curve = p.curve;
  return el;
}

/** The look of a preset for a view with light and dark themes (dark ink → theme colour). */
export function themedLook(p: TextPreset): TextLook {
  const look = structuredClone(p.look);
  if (p.themeInk) delete look.fill;
  return look;
}

const num = (v: number) => Math.round(v * 10000) / 10000;

/**
 * Inline CSS for a look. The outline uses -webkit-text-stroke (supported by WebKitGTK,
 * Chromium and Firefox) with `paint-order: stroke fill`, so the stroke is drawn under
 * the fill and only its outer half shows — hence twice the width, like the collage
 * renderer's `lineWidth = width * 2`. The pill is left to the caller (it depends on the
 * box); `pill: true` adds it as a background with em padding and radius.
 */
export function lookCss(look: TextLook | undefined | null, opts: { pill?: boolean } = {}): string {
  if (!look) return '';
  const f = fontInfo(look.font);
  const css: string[] = [`font-family:'${f.family}',var(--ui, sans-serif)`, `font-weight:${resolveWeight(look.font, look.weight ?? 400)}`];
  if (look.italic) css.push('font-style:italic');
  if (look.fill) css.push(`color:${look.fill}`);
  if (look.uppercase) css.push('text-transform:uppercase');
  if (look.letterSpacing) css.push(`letter-spacing:${num(look.letterSpacing)}em`);
  if (look.stroke && look.stroke.width > 0) {
    css.push(`-webkit-text-stroke:${num(look.stroke.width * 2)}em ${look.stroke.color}`, 'paint-order:stroke fill');
  }
  if (look.shadow) {
    const s = look.shadow;
    css.push(`text-shadow:${num(s.x)}em ${num(s.y)}em ${num(s.blur)}em ${s.color}`);
  }
  if (opts.pill && look.background) {
    const b = look.background;
    css.push(`background:${b.color}`, `padding:${num(b.padding * 0.6)}em ${num(b.padding)}em`, `border-radius:${num(b.radius)}em`);
  }
  return css.join(';');
}

// ---------- section titles (linear story view, slides, print) ----------

export interface TitleStyle {
  id: string;
  label: string;
  /** size relative to the classic title (script faces look small at the same size) */
  scale: number;
  look?: TextLook;
}

/** Classic serif first (no `look`: the views' own title font). */
export const TITLE_STYLES: TitleStyle[] = [
  { id: 'classic', label: 'Classic serif', scale: 1 },
  { id: 'script', label: 'Script', scale: 1.45, look: { font: 'great-vibes' } },
  { id: 'poster', label: 'Poster', scale: 1.25, look: { font: 'bebas-neue', letterSpacing: 0.06 } },
  { id: 'handwritten', label: 'Handwritten', scale: 1.3, look: { font: 'caveat', weight: 700 } },
  { id: 'marker', label: 'Marker', scale: 1.05, look: { font: 'permanent-marker' } },
  { id: 'meme', label: 'Meme', scale: 1.15, look: themedLook(byId.get('meme')!) },
  { id: 'bubbly', label: 'Bubbly', scale: 1.05, look: { font: 'fredoka', weight: 600 } },
  { id: 'elegant', label: 'Elegant caps', scale: 0.95, look: { font: 'cinzel', weight: 700, letterSpacing: 0.12 } },
];

const titleById = new Map(TITLE_STYLES.map((t) => [t.id, t]));

export function titleStyle(id: string | undefined): TitleStyle {
  return (id && titleById.get(id)) || TITLE_STYLES[0];
}

/**
 * Inline CSS for a section title in a given style ('' for the classic look). The size
 * factor is passed as `--tscale`: each view multiplies its own title size by it.
 */
export function titleCss(id: string | undefined): string {
  const t = titleStyle(id);
  if (!t.look) return '';
  return `${lookCss(t.look)};--tscale:${t.scale}`;
}
