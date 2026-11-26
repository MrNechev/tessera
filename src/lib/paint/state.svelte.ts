/**
 * Paint's UI state: the open document, the current tool and its options,
 * colours, the viewport. Tool options and colours are remembered per browser
 * (localStorage `tessera.paint`).
 */
import type { TextElement } from '../model/types';
import type { PaintDoc } from './doc.svelte';
import type { View } from './lib/geom';
import type { Combine, Rect } from './lib/mask';

export type ToolId =
  | 'brush'
  | 'pencil'
  | 'eraser'
  | 'fill'
  | 'picker'
  | 'shape'
  | 'text'
  | 'select'
  | 'lasso'
  | 'wand'
  | 'move'
  | 'hand'
  | 'crop'
  | 'blur'
  | 'smudge'
  | 'clone';

export type ShapeKind = 'line' | 'arrow' | 'rect' | 'ellipse';
export type ShapeStyle = 'outline' | 'fill' | 'both';

export interface ToolOptions {
  brushSize: number;
  pencilSize: number;
  eraserSize: number;
  retouchSize: number;
  /** 0..1 */
  opacity: number;
  /** 0..1: how much strokes are smoothed. */
  smoothing: number;
  /** Fill/wand: 0..100 %. */
  tolerance: number;
  contiguous: boolean;
  /** Fill/wand/picker: look at all visible layers, not just the active one. */
  sampleAll: boolean;
  shape: ShapeKind;
  shapeStyle: ShapeStyle;
  shapeWidth: number;
  selectShape: 'rect' | 'ellipse';
  selectMode: Combine;
  textPreset: string;
  textSize: number;
  /** 0..1 for blur and smudge. */
  strength: number;
  grid: boolean;
}

const DEFAULTS: ToolOptions = {
  brushSize: 14,
  pencilSize: 1,
  eraserSize: 28,
  retouchSize: 40,
  opacity: 1,
  smoothing: 0.5,
  tolerance: 20,
  contiguous: true,
  sampleAll: false,
  shape: 'rect',
  shapeStyle: 'outline',
  shapeWidth: 6,
  selectShape: 'rect',
  selectMode: 'replace',
  textPreset: 'heading',
  textSize: 72,
  strength: 0.5,
  grid: true,
};

/** A question with buttons (large image, unsaved changes…), answered by PaintView. */
export interface Ask {
  title: string;
  message: string;
  choices: { id: string; label: string; primary?: boolean }[];
  resolve: (id: string | null) => void;
}

const KEY = 'tessera.paint';

function load(): { opts: Partial<ToolOptions>; primary?: string; secondary?: string; recent?: string[]; tool?: ToolId } {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '{}') ?? {};
  } catch {
    return { opts: {} };
  }
}

class PaintState {
  doc = $state.raw<PaintDoc | null>(null);
  tool = $state<ToolId>('brush');
  opts = $state<ToolOptions>({ ...DEFAULTS });
  primary = $state('#1d1b26');
  secondary = $state('#ffffff');
  recent = $state<string[]>([]);
  view = $state<View>({ zoom: 1, x: 0, y: 0 });
  /** Autosave status. */
  saving = $state<'idle' | 'saving' | 'saved' | 'error'>('idle');
  busy = $state<string | null>(null);
  /** The text layer being edited, and its text when editing began (one undo step). */
  textEdit = $state<{ layerId: string; before: TextElement | null } | null>(null);
  crop = $state<Rect | null>(null);
  ask = $state<Ask | null>(null);
  /** Dialogs of the Paint view. */
  dialog = $state<null | 'new' | 'export' | 'resize' | 'canvas' | 'look' | 'help' | 'open'>(null);
  /** Set when "fit to window" should run once the view knows its size. */
  fitPending = $state(true);

  constructor() {
    if (typeof localStorage === 'undefined') return;
    const s = load();
    this.opts = { ...DEFAULTS, ...(s.opts ?? {}) };
    if (s.primary) this.primary = s.primary;
    if (s.secondary) this.secondary = s.secondary;
    if (Array.isArray(s.recent)) this.recent = s.recent.slice(0, 16);
    if (s.tool) this.tool = s.tool;
  }

  persist(): void {
    try {
      localStorage.setItem(KEY, JSON.stringify({ opts: $state.snapshot(this.opts), primary: this.primary, secondary: this.secondary, recent: this.recent, tool: this.tool }));
    } catch {
      /* private mode */
    }
  }

  setPrimary(c: string): void {
    this.primary = c;
    this.remember(c);
  }
  setSecondary(c: string): void {
    this.secondary = c;
    this.remember(c);
  }
  swap(): void {
    [this.primary, this.secondary] = [this.secondary, this.primary];
    this.persist();
  }
  resetColors(): void {
    this.primary = '#000000';
    this.secondary = '#ffffff';
    this.persist();
  }
  remember(c: string): void {
    const hex = c.slice(0, 7).toLowerCase();
    this.recent = [hex, ...this.recent.filter((x) => x !== hex)].slice(0, 16);
    this.persist();
  }

  /** Ask a question with buttons; resolves with the chosen id (null: closed). */
  choose(title: string, message: string, choices: Ask['choices']): Promise<string | null> {
    return new Promise((resolve) => {
      this.ask?.resolve(null);
      this.ask = {
        title,
        message,
        choices,
        resolve: (id) => {
          this.ask = null;
          resolve(id);
        },
      };
    });
  }

  /** Size of the active tool (brush, pencil, eraser, retouch). */
  get size(): number {
    const o = this.opts;
    return this.tool === 'pencil' ? o.pencilSize : this.tool === 'eraser' ? o.eraserSize : ['blur', 'smudge', 'clone'].includes(this.tool) ? o.retouchSize : o.brushSize;
  }
  set size(v: number) {
    const s = Math.max(1, Math.min(1000, Math.round(v)));
    if (this.tool === 'pencil') this.opts.pencilSize = s;
    else if (this.tool === 'eraser') this.opts.eraserSize = s;
    else if (['blur', 'smudge', 'clone'].includes(this.tool)) this.opts.retouchSize = s;
    else this.opts.brushSize = s;
    this.persist();
  }
}

export const paint = new PaintState();

/** "#rrggbb" or "#rrggbbaa" → [r, g, b, a] (0..255). */
export function rgbaOf(c: string): [number, number, number, number] {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(c.trim());
  if (!m) return [0, 0, 0, 255];
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255, m[2] ? parseInt(m[2], 16) : 255];
}

export function hexOf(r: number, g: number, b: number, a = 255): string {
  const h = (v: number) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, '0');
  return `#${h(r)}${h(g)}${h(b)}${a < 255 ? h(a) : ''}`;
}
