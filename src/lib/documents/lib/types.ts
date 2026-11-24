import type { PhotoAdjust } from '../../model/types';

export type Kind = 'image' | 'gif' | 'video' | 'audio';

export type Layout = 'media-top' | 'media-left' | 'media-right' | 'media-full' | 'text-only' | 'gallery' | 'compare' | 'collage';

export interface MediaRef {
  libraryId: string;
  path: string;
  caption?: string;
  startAt?: number;
  endAt?: number;
  muted?: boolean;
  loop?: boolean;
  autoplay?: boolean;
  fit?: 'contain' | 'cover';
  /** crop of the source, as fractions of its width/height (originals are never touched) */
  crop?: { x: number; y: number; w: number; h: number };
  /** photo look (Tessera's photo toolkit): colour adjustments, the filter preset they came from */
  adjust?: PhotoAdjust;
  filter?: string;
  flipX?: boolean;
  flipY?: boolean;
}

/**
 * A live collage in a section. The project stays the single source: editing it in
 * the collage editor changes every story that shows it.
 */
export interface CollageRef {
  /** id of a project in the app's project library (`load_project_by_id`) */
  projectId: string;
  /** page of a multi-page project (0 = first) */
  page?: number;
  /** width / height when it was linked: sizes the box before the project has loaded */
  aspect?: number;
  /** the project's name when it was linked, for the "missing collage" placeholder */
  name?: string;
}

export interface Section {
  id: string;
  title?: string;
  layout: Layout;
  media: MediaRef[];
  text?: any; // TipTap JSON
  slide: boolean;
  notes?: string;
  collapsed?: boolean;
  gallery?: { mode?: 'grid' | 'carousel'; columns?: number };
  /** comparison layout: slider over two images/videos, or two videos side by side in sync */
  compare?: { mode?: 'slider' | 'sync'; pos?: number };
  /** collage layout: a page of a collage project, rendered live from the project */
  collage?: CollageRef;
  transition?: string;
  /** position of the section's card on the canvas */
  canvas?: SectionCanvas;
  /** a chapter heading: groups the sections after it until the next chapter */
  chapter?: boolean;
  /** seconds on screen in timed slideshows (overrides the document setting) */
  advance?: number;
  /** last rehearsal: seconds spent on this slide */
  rehearsal?: number;
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** shared by every canvas box: grouping, locking and reveal steps */
export interface BoxFlags {
  /** items with the same group id move and select together */
  group?: string;
  /** can't be moved, resized or deleted until unlocked */
  locked?: boolean;
  /** canvas presentation: appears on the n-th click inside its frame (0/undefined = always) */
  reveal?: number;
}

export interface SectionCanvas extends Rect, BoxFlags {
  /** removed from the canvas (the section stays in the document) */
  hidden?: boolean;
}

/** Arrow end: a free point, or attached to a card/note/shape/frame (id); x/y is then the fallback. */
export interface ArrowEnd {
  x: number;
  y: number;
  id?: string;
}

export interface NoteItem extends Rect, BoxFlags {
  id: string;
  type: 'note';
  /** free text grows sideways while typing until resized by hand */
  autoW?: boolean;
  text: string;
  /** palette key, or 'none' for free text without a background */
  color: string;
  size: number;
  align?: 'left' | 'center' | 'right';
}

export interface ShapeItem extends Rect, BoxFlags {
  id: string;
  type: 'rect' | 'ellipse';
  color: string;
  fill?: boolean;
  text?: string;
  size?: number;
}

export interface FrameItem extends Rect, BoxFlags {
  id: string;
  type: 'frame';
  title: string;
  notes?: string;
}

export interface ArrowItem {
  id: string;
  type: 'arrow';
  from: ArrowEnd;
  to: ArrowEnd;
  color: string;
  head?: 'end' | 'both' | 'none';
  dashed?: boolean;
  text?: string;
  /** bend: offset of the curve's middle from the straight line (world px, signed) */
  curve?: number;
  group?: string;
  locked?: boolean;
  reveal?: number;
}

/** freehand pen / highlighter stroke; points are fractions of the box (so it scales) */
export interface PathItem extends Rect, BoxFlags {
  id: string;
  type: 'path';
  points: [number, number][];
  color: string;
  width: number;
  highlighter?: boolean;
}

export type BoxItem = NoteItem | ShapeItem | FrameItem | PathItem;
export type CanvasItem = BoxItem | ArrowItem;

export interface DocCanvas {
  items: CanvasItem[];
}

export interface DocSettings {
  transition?: 'fade' | 'slide' | 'zoom' | 'none';
  theme?: string;
  /** background music for presentations (an audio file from a library) */
  music?: MediaRef;
  musicVolume?: number;
  /** advance slides automatically after this many seconds (0/undefined = off) */
  autoAdvance?: number;
  /** advance when the slide's video ends */
  advanceOnVideoEnd?: boolean;
  /** start over after the last slide (unattended slideshow) */
  loop?: boolean;
}

export interface Doc {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  version: number;
  settings?: DocSettings;
  sections: Section[];
  canvas?: DocCanvas;
  tags?: string[];
  cover?: MediaRef;
}

export interface DocMeta {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  version: number;
  sections: number;
  media: number;
  coverLibraryId?: string;
  coverPath?: string;
  tags: string[];
}

export interface FileItem {
  id: number;
  libraryId: string;
  path: string;
  dir: string;
  name: string;
  ext: string;
  kind: Kind;
  size: number;
  mtime: number;
  key: string;
  probed: boolean;
  width: number;
  height: number;
  duration: number;
  vcodec?: string;
  acodec?: string;
  format?: string;
  bitrate?: number;
  rotation?: number;
  probeError?: string;
  playable: boolean;
  /** capture time (unix ms), when the file has it */
  taken?: number;
  lat?: number;
  lon?: number;
  hasGps?: boolean;
  camera?: string;
  hdr?: boolean;
  /** live/motion photo */
  motion?: string;
  /** "City, Country" for files with GPS */
  place?: string;
}

export interface ProxyState {
  variant: string;
  status: 'ready' | 'queued' | 'running' | 'failed' | 'none';
  progress?: number;
  error?: string;
}

export interface Source {
  url?: string;
  variant?: string;
  proxy?: ProxyState;
}

export interface Resolved {
  libraryId: string;
  path: string;
  found: boolean;
  file?: FileItem;
  original?: string;
  derived?: string;
  gif?: string;
  doc?: Source;
  present?: Source;
  motion?: string;
  sprite?: string;
}

export interface PrefItem {
  libraryId: string;
  path: string;
  dir?: boolean;
  at?: number;
}

export interface Prefs {
  favorites: PrefItem[];
  recent: PrefItem[];
  hidden: PrefItem[];
}

export interface SearchHit {
  docId: string;
  docTitle: string;
  sectionId?: string;
  section: number;
  field: string;
  snippet: string;
}

export interface Usage {
  docId: string;
  docTitle: string;
  sectionId?: string;
  section: number;
  title?: string;
  where: 'section' | 'cover' | 'music';
}

export interface Library {
  id: string;
  name: string;
  counts: { total: number; byKind: Record<string, number>; unprobed: number; bytes: number };
}

export interface DirNode {
  name: string;
  path: string;
  count: number;
  total: number;
  children?: DirNode[];
}

export interface HistoryEntry {
  version: number;
  savedAt: string;
  title: string;
  sections: number;
  media: number;
  bytes: number;
}
