/**
 * Tessera collage document model.
 *
 * All geometry is expressed in *page units*, which are the pixels of the final
 * exported image at 100% scale. Rotation is in degrees, clockwise, around the
 * element's centre. Elements are stored bottom-to-top (index 0 is drawn first).
 */

export const DOC_VERSION = 1;

/** CSS colour string: `#rrggbb`, `#rrggbbaa` or `rgba(...)`. */
export type Color = string;

export interface GradientStop {
  offset: number; // 0..1
  color: Color;
}

export type Fill =
  | { kind: 'solid'; color: Color }
  | { kind: 'linear'; angle: number; stops: GradientStop[] }
  | { kind: 'radial'; stops: GradientStop[]; cx?: number; cy?: number };

export interface Shadow {
  color: Color;
  /** Blur radius as a fraction of the page's shorter side. */
  blur: number;
  /** Offset as a fraction of the page's shorter side. */
  x: number;
  y: number;
}

export type MaskShape =
  | 'rect'
  | 'circle'
  | 'heart'
  | 'star'
  | 'hexagon'
  | 'octagon'
  | 'diamond'
  | 'triangle'
  | 'arch'
  | 'pill'
  | 'blob'
  | 'scallop'
  | 'cloud'
  | 'squircle';

export interface Mask {
  shape: MaskShape;
  /** Corner radius for `rect`, as a fraction of the element's shorter side (0..0.5). */
  radius: number;
}

export interface Stroke {
  /** Width in page units. */
  width: number;
  color: Color;
}

export type PhotoFrameStyle = 'none' | 'polaroid' | 'film' | 'mat' | 'tape' | 'instant';

export interface PhotoFrame {
  style: PhotoFrameStyle;
  color: Color;
  /** Optional caption, drawn in the polaroid / instant bottom margin. */
  caption?: string;
  captionFont?: string;
  captionColor?: Color;
}

export interface PhotoAdjust {
  brightness: number; // -1..1
  contrast: number; // -1..1
  saturation: number; // -1..1
  warmth: number; // -1..1
  fade: number; // 0..1
  vignette: number; // 0..1
  grayscale: number; // 0..1
  sepia: number; // 0..1
}

export interface Crop {
  /** Focal position inside the overflow, 0..1 (0.5 = centred), like PhotoCollage's offsets. */
  x: number;
  y: number;
  /** >= 1. 1 means the photo exactly covers its frame. */
  zoom: number;
}

export type ElementKind = 'photo' | 'text' | 'sticker' | 'shape' | 'calendar';

export interface ElementBase {
  id: string;
  kind: ElementKind;
  name?: string;
  x: number;
  y: number;
  w: number;
  h: number;
  rotation: number;
  opacity: number;
  flipX?: boolean;
  flipY?: boolean;
  locked?: boolean;
  hidden?: boolean;
  shadow?: Shadow | null;
}

export interface PhotoElement extends ElementBase {
  kind: 'photo';
  /** Asset id, or null for an empty placeholder frame (templates). */
  asset: string | null;
  crop: Crop;
  mask: Mask;
  border: Stroke | null;
  frame: PhotoFrame | null;
  adjust: PhotoAdjust | null;
  filter?: string | null;
  /** Set when the element is managed by the document's auto-layout. */
  slot?: boolean;
}

export type TextAlign = 'left' | 'center' | 'right';

export interface TextElement extends ElementBase {
  kind: 'text';
  text: string;
  font: string; // font id from the font registry
  size: number; // page units
  weight: number;
  italic: boolean;
  fill: Color;
  align: TextAlign;
  lineHeight: number; // multiple of size
  letterSpacing: number; // em
  uppercase?: boolean;
  stroke: Stroke | null;
  /** Rounded "pill" behind the text. */
  background: { color: Color; padding: number; radius: number } | null;
  /** Arc amount -1..1 (0 = straight). */
  curve: number;
}

export interface StickerElement extends ElementBase {
  kind: 'sticker';
  sticker: string;
}

export type ShapeKind = MaskShape | 'line' | 'ring';

export interface ShapeElement extends ElementBase {
  kind: 'shape';
  shape: ShapeKind;
  fill: Fill | null;
  stroke: Stroke | null;
  radius: number;
}

export type CalendarStyle = 'clean' | 'lines' | 'boxes' | 'dots';

export interface CalendarElement extends ElementBase {
  kind: 'calendar';
  year: number;
  /** 1–12. */
  month: number;
  /** 0 = Sunday, 1 = Monday. */
  firstDay: 0 | 1;
  style: CalendarStyle;
  /** Font ids for the month title and the day numbers. */
  titleFont: string;
  font: string;
  color: Color;
  accent: Color;
  weekendColor: Color;
  showTitle: boolean;
  showYear: boolean;
  /** Days of this month to mark with the accent colour (birthdays, holidays…). */
  marked: number[];
  /** Optional background pill behind the whole calendar. */
  background: Color | null;
}

export type CollageElement = PhotoElement | TextElement | StickerElement | ShapeElement | CalendarElement;

export type PatternId =
  | 'dots'
  | 'polka'
  | 'stripes'
  | 'pinstripe'
  | 'grid'
  | 'checker'
  | 'gingham'
  | 'hearts'
  | 'stars'
  | 'snowflakes'
  | 'confetti'
  | 'chevron'
  | 'waves'
  | 'plus'
  | 'triangles'
  | 'sparkles';

export interface BackgroundPattern {
  id: PatternId;
  color: Color;
  opacity: number;
  /** Tile size as a fraction of the page's shorter side. */
  scale: number;
}

export type TextureId = 'paper' | 'linen' | 'kraft' | 'grain' | 'watercolor';

export interface Background {
  fill: Fill;
  pattern: BackgroundPattern | null;
  texture: { id: TextureId; intensity: number } | null;
  image: { asset: string; blur: number; dim: number; opacity: number } | null;
  vignette: number;
}

export type PageFrameStyle =
  | 'none'
  | 'line'
  | 'double'
  | 'stitched'
  | 'dots'
  | 'scallop'
  | 'candy'
  | 'hearts'
  | 'stars'
  | 'film'
  | 'corners'
  | 'gold'
  | 'snow';

export interface PageFrame {
  style: PageFrameStyle;
  color: Color;
  /** Line width / motif size as a fraction of the page's shorter side. */
  width: number;
  /** Distance from the page edge as a fraction of the page's shorter side. */
  inset: number;
}

export interface Page {
  width: number;
  height: number;
  dpi: number;
  preset?: string;
}

export type LayoutEngine =
  | 'none'
  | 'mosaic'
  | 'columns'
  | 'rows'
  | 'grid'
  | 'featured'
  | 'shape'
  | 'scatter'
  | 'ring'
  | 'letters'
  | 'spiral';

export type LayoutShape = 'heart' | 'circle' | 'star' | 'hexagon' | 'diamond';

export interface LayoutSettings {
  engine: LayoutEngine;
  seed: number;
  /** Gap between photos as a fraction of the page's shorter side. */
  spacing: number;
  /** Outer margin as a fraction of the page's shorter side. */
  margin: number;
  /** Optional region of the page used by the layout (fractions of page w/h). */
  region?: { x: number; y: number; w: number; h: number } | null;
  shape?: LayoutShape;
  /** Repeat photos to fill every cell of a shape layout. */
  repeat?: boolean;
  /** Preferred column count for `columns` / `grid` (0 = automatic). */
  columns?: number;
  /** Text whose glyphs the photos fill (`letters` engine). */
  text?: string;
  /** Font id and weight used to shape the `letters` layout. */
  font?: string;
  fontWeight?: number;
}

export interface AssetRef {
  path: string;
  name: string;
  width: number;
  height: number;
}

export interface CollageDoc {
  version: number;
  id: string;
  name: string;
  created: string;
  modified: string;
  page: Page;
  background: Background;
  frame: PageFrame | null;
  layout: LayoutSettings;
  elements: CollageElement[];
  assets: Record<string, AssetRef>;
  /** Template the document was started from, if any. */
  template?: string | null;
  /**
   * Multi-page projects (photo books, calendars). The active page's content
   * lives in the top-level fields above; `pages` holds every page in order,
   * with `null` at `activePage`. Absent for single-page collages.
   */
  pages?: (PageContent | null)[];
  activePage?: number;
  /** Id of the active page (its content is in the top-level fields). */
  activePageId?: string;
}

/** Everything that differs between the pages of a multi-page project. */
export interface PageContent {
  id: string;
  background: Background;
  frame: PageFrame | null;
  layout: LayoutSettings;
  elements: CollageElement[];
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}
