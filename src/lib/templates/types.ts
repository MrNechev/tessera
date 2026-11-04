import type {
  Background,
  Fill,
  LayoutSettings,
  Mask,
  PageFrame,
  PhotoFrame,
  Shadow,
  ShapeKind,
  TextAlign,
} from '../model/types';

export type TemplateCategory =
  | 'simple'
  | 'love'
  | 'christmas'
  | 'newyear'
  | 'birthday'
  | 'family'
  | 'wedding'
  | 'baby'
  | 'travel'
  | 'seasonal'
  | 'friends'
  | 'school'
  | 'pets';

export const TEMPLATE_CATEGORIES: { id: TemplateCategory; name: string; emoji: string }[] = [
  { id: 'simple', name: 'Simple', emoji: '▦' },
  { id: 'love', name: 'Love & Valentine’s', emoji: '♥' },
  { id: 'christmas', name: 'Christmas & Winter', emoji: '❄' },
  { id: 'newyear', name: 'New Year', emoji: '✦' },
  { id: 'birthday', name: 'Birthday', emoji: '✿' },
  { id: 'family', name: 'Family & Parents', emoji: '⌂' },
  { id: 'wedding', name: 'Wedding', emoji: '◎' },
  { id: 'baby', name: 'Baby', emoji: '☾' },
  { id: 'travel', name: 'Travel & Summer', emoji: '✈' },
  { id: 'seasonal', name: 'Holidays & Seasons', emoji: '❀' },
  { id: 'friends', name: 'Friends', emoji: '☺' },
  { id: 'school', name: 'Graduation', emoji: '✎' },
  { id: 'pets', name: 'Pets', emoji: '❦' },
];

/** All coordinates are fractions of the page width (x, w) and height (y, h). */
export interface SlotDef {
  x: number;
  y: number;
  w: number;
  h: number;
  rotation?: number;
  mask?: Mask;
  frame?: PhotoFrame | null;
  /** Border width as a fraction of the page's shorter side. */
  border?: { width: number; color: string } | null;
  shadow?: Shadow | null;
}

export interface TextDef {
  kind: 'text';
  text: string;
  /** Centre of the text box. */
  cx: number;
  cy: number;
  /** Box width as a fraction of the page width. */
  w: number;
  font: string;
  /** Font size as a fraction of the page's shorter side. */
  size: number;
  color: string;
  weight?: number;
  italic?: boolean;
  align?: TextAlign;
  lineHeight?: number;
  letterSpacing?: number;
  uppercase?: boolean;
  curve?: number;
  rotation?: number;
  stroke?: { width: number; color: string } | null; // width: fraction of font size
  shadow?: Shadow | null;
  background?: { color: string; padding: number; radius: number } | null;
}

export interface StickerDef {
  kind: 'sticker';
  sticker: string;
  cx: number;
  cy: number;
  /** Width as a fraction of the page's shorter side. */
  size: number;
  rotation?: number;
  opacity?: number;
  flipX?: boolean;
  shadow?: Shadow | null;
}

export interface ShapeDef {
  kind: 'shape';
  shape: ShapeKind;
  x: number;
  y: number;
  w: number;
  h: number;
  fill: Fill | null;
  stroke?: { width: number; color: string } | null; // width: fraction of short side
  radius?: number;
  rotation?: number;
  opacity?: number;
  shadow?: Shadow | null;
  /** Draw below the photos. */
  below?: boolean;
}

export type ItemDef = TextDef | StickerDef | ShapeDef;

export interface TemplateDef {
  id: string;
  name: string;
  category: TemplateCategory;
  description: string;
  page: { width: number; height: number; dpi?: number; preset?: string };
  background: Background;
  frame?: PageFrame | null;
  /** Automatic layout for any number of photos … */
  layout?: Partial<LayoutSettings> & Pick<LayoutSettings, 'engine'>;
  /** Style applied to every auto-laid-out photo. */
  photoStyle?: Omit<SlotDef, 'x' | 'y' | 'w' | 'h'>;
  /** … or fixed photo frames. */
  slots?: SlotDef[];
  items?: ItemDef[];
  /** How many placeholder photos to show in previews of auto-layout templates. */
  previewPhotos?: number;
}
