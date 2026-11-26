/**
 * Undo/redo for Paint. Pixel edits store only the tiles they changed (the
 * pixels before and after), structural edits (new layer, resize…) store
 * their own undo/redo functions. A memory budget drops the oldest steps.
 *
 * No DOM here: reading and writing pixels go through callbacks, so the
 * bookkeeping is unit-tested with plain arrays.
 */
import type { Rect } from './mask';

export const TILE = 128;
/** Default memory budget for undo steps. */
export const DEFAULT_BUDGET = 384 * 1024 * 1024;

export interface Step {
  label: string;
  /** Approximate memory held by this step, in bytes. */
  bytes: number;
  undo(): void;
  redo(): void;
}

/** Indices (ty * cols + tx) of the tiles a rectangle touches, inside a w × h image. */
export function tilesFor(r: Rect, w: number, h: number, tile = TILE): number[] {
  const cols = Math.ceil(w / tile);
  const x0 = Math.max(0, Math.floor(r.x / tile));
  const y0 = Math.max(0, Math.floor(r.y / tile));
  const x1 = Math.min(cols - 1, Math.floor((r.x + r.w - 1) / tile));
  const y1 = Math.min(Math.ceil(h / tile) - 1, Math.floor((r.y + r.h - 1) / tile));
  const out: number[] = [];
  if (r.w <= 0 || r.h <= 0) return out;
  for (let ty = y0; ty <= y1; ty++) for (let tx = x0; tx <= x1; tx++) out.push(ty * cols + tx);
  return out;
}

/** The pixel rectangle of tile `i`. */
export function tileRect(i: number, w: number, h: number, tile = TILE): Rect {
  const cols = Math.ceil(w / tile);
  const x = (i % cols) * tile;
  const y = Math.floor(i / cols) * tile;
  return { x, y, w: Math.min(tile, w - x), h: Math.min(tile, h - y) };
}

export interface Patch {
  rect: Rect;
  before: Uint8ClampedArray;
  after: Uint8ClampedArray;
}

type Read = (r: Rect) => Uint8ClampedArray;

/**
 * Records one pixel edit of one layer: call `touch` with every rectangle
 * *before* drawing into it (the first touch of a tile saves its pixels),
 * then `finish` once the edit is done. Unchanged tiles are dropped.
 */
export class TileRecorder {
  private before = new Map<number, Uint8ClampedArray>();
  constructor(
    readonly w: number,
    readonly h: number,
    private read: Read,
    readonly tile = TILE,
  ) {}

  touch(r: Rect): void {
    for (const i of tilesFor(r, this.w, this.h, this.tile)) {
      if (!this.before.has(i)) this.before.set(i, this.read(tileRect(i, this.w, this.h, this.tile)));
    }
  }

  get touched(): number {
    return this.before.size;
  }

  finish(): Patch[] {
    const out: Patch[] = [];
    for (const [i, before] of this.before) {
      const rect = tileRect(i, this.w, this.h, this.tile);
      const after = this.read(rect);
      if (!same(before, after)) out.push({ rect, before, after });
    }
    this.before.clear();
    return out;
  }
}

function same(a: Uint8ClampedArray, b: Uint8ClampedArray): boolean {
  if (a.length !== b.length) return false;
  // Compare 32 bits at a time when aligned.
  if (a.byteOffset % 4 === 0 && b.byteOffset % 4 === 0 && a.length % 4 === 0) {
    const x = new Uint32Array(a.buffer, a.byteOffset, a.length / 4);
    const y = new Uint32Array(b.buffer, b.byteOffset, b.length / 4);
    for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return false;
    return true;
  }
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

export function patchBytes(p: Patch[]): number {
  return p.reduce((s, x) => s + x.before.byteLength + x.after.byteLength + 64, 0);
}

export class History {
  private done: Step[] = [];
  private undone: Step[] = [];
  private total = 0;
  /** Bumped on every change (for reactive UIs). */
  version = 0;

  constructor(public budget = DEFAULT_BUDGET) {}

  get canUndo(): boolean {
    return this.done.length > 0;
  }
  get canRedo(): boolean {
    return this.undone.length > 0;
  }
  get bytes(): number {
    return this.total;
  }
  get undoLabel(): string | null {
    return this.done.at(-1)?.label ?? null;
  }
  get redoLabel(): string | null {
    return this.undone.at(-1)?.label ?? null;
  }
  get size(): number {
    return this.done.length;
  }

  /** Record a step that has already been applied. */
  push(step: Step): void {
    for (const s of this.undone) this.total -= s.bytes;
    this.undone = [];
    this.done.push(step);
    this.total += step.bytes;
    this.trim();
    this.version++;
  }

  undo(): Step | null {
    const s = this.done.pop();
    if (!s) return null;
    s.undo();
    this.undone.push(s);
    this.version++;
    return s;
  }

  redo(): Step | null {
    const s = this.undone.pop();
    if (!s) return null;
    s.redo();
    this.done.push(s);
    this.version++;
    return s;
  }

  clear(): void {
    this.done = [];
    this.undone = [];
    this.total = 0;
    this.version++;
  }

  /** Forget the oldest steps until within budget (the newest step is always kept). */
  private trim(): void {
    while (this.total > this.budget && this.done.length > 1) {
      const s = this.done.shift()!;
      this.total -= s.bytes;
    }
  }
}
