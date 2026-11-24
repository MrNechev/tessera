/**
 * Video out: MP4 slideshows made by the engine (`video_start` & co. in
 * crates/tessera-core/src/tools/video). This side builds the ordered item list
 * from a library selection, a story or a collage, and draws what only the
 * browser can draw — title cards (Tessera's fonts) and collage pages (the
 * collage renderer) — as pictures the engine puts into the video.
 */
import { call } from '../api/http';
import { ensureFont, cssFont } from '../assets/fonts';
import type { Doc, FileItem, MediaRef } from '../documents/lib/types';
import type { CollageDoc } from '../model/types';
import { pageDoc } from '../model/multipage';
import { renderPageBlob } from '../editor/exporter';

export type VideoSize = 'landscape' | 'vertical' | 'square';
export type VideoTransition = 'crossfade' | 'black' | 'none';
export type VideoFraming = 'auto' | 'fill' | 'fit';

/** What a video is made from. */
export type VideoSource = { kind: 'files'; files: FileItem[] } | { kind: 'story'; doc: Doc } | { kind: 'collage' };

export interface FileRef {
  libraryId?: string;
  path: string;
}

export type VideoItem =
  | {
      kind: 'image';
      libraryId?: string;
      path: string;
      crop?: { x: number; y: number; w: number; h: number };
      flipX?: boolean;
      flipY?: boolean;
      seconds?: number;
      kenBurns?: boolean;
      framing?: VideoFraming;
    }
  | { kind: 'video'; libraryId?: string; path: string; start?: number; end?: number; framing?: VideoFraming }
  | { kind: 'card'; data: string; seconds?: number; kenBurns?: boolean; framing?: VideoFraming };

export interface VideoOptions {
  size: VideoSize;
  seconds: number;
  transition: VideoTransition;
  kenBurns: boolean;
  framing: VideoFraming;
  music?: FileRef | null;
  name: string;
}

export interface VideoStatus {
  job: number;
  state: 'queued' | 'running' | 'done' | 'failed' | 'cancelled';
  progress: number;
  stage: string;
  path?: string;
  rel?: string;
  error?: string;
  encoder?: string;
  seconds: number;
}

export interface VideoSupport {
  available: boolean;
  encoder?: string;
  gpu: boolean;
  message?: string;
}

export const videoApi = {
  support: () => call<VideoSupport>('video_support'),
  start: (items: VideoItem[], options: VideoOptions) => call<VideoStatus>('video_start', { items, options }),
  status: (job: number) => call<VideoStatus>('video_status', { job }),
  cancel: (job: number) => call<void>('video_cancel', { job }),
};

export const FRAME: Record<VideoSize, { w: number; h: number }> = {
  landscape: { w: 1920, h: 1080 },
  vertical: { w: 1080, h: 1920 },
  square: { w: 1080, h: 1080 },
};

const VIDEO_EXT = new Set(['mp4', 'm4v', 'mov', 'webm', 'mkv', 'avi', '3gp', 'mts', 'm2ts', 'mpg', 'mpeg', 'wmv', 'gif']);
const AUDIO_EXT = new Set(['mp3', 'm4a', 'aac', 'flac', 'ogg', 'oga', 'opus', 'wav', 'wma']);

function ext(path: string): string {
  const dot = path.lastIndexOf('.');
  return dot > path.lastIndexOf('/') ? path.slice(dot + 1).toLowerCase() : '';
}

async function blobToBase64(b: Blob): Promise<string> {
  const buf = new Uint8Array(await b.arrayBuffer());
  let s = '';
  // In slices: String.fromCharCode(...all) overflows the stack for large pictures.
  for (let i = 0; i < buf.length; i += 0x8000) s += String.fromCharCode(...buf.subarray(i, i + 0x8000));
  return btoa(s);
}

/** Break `text` into lines no wider than `max` at the context's current font. */
function wrap(c: CanvasRenderingContext2D, text: string, max: number): string[] {
  const lines: string[] = [];
  let line = '';
  for (const word of text.split(/\s+/).filter(Boolean)) {
    const next = line ? `${line} ${word}` : word;
    if (line && c.measureText(next).width > max) {
      lines.push(line);
      line = word;
    } else line = next;
  }
  if (line) lines.push(line);
  return lines;
}

/**
 * A title card at the video's size: a dark, softly lit background with the
 * title in Playfair Display and an optional small line above it in Inter.
 */
export async function titleCard(size: VideoSize, title: string, kicker?: string): Promise<VideoItem> {
  await Promise.all([ensureFont('playfair-display', 700), ensureFont('inter', 500)]);
  const { w, h } = FRAME[size];
  const canvas = document.createElement('canvas');
  canvas.width = w;
  canvas.height = h;
  const c = canvas.getContext('2d')!;
  const bg = c.createRadialGradient(w / 2, h * 0.45, 0, w / 2, h / 2, Math.hypot(w, h) / 1.6);
  bg.addColorStop(0, '#2a2533');
  bg.addColorStop(1, '#0d0b12');
  c.fillStyle = bg;
  c.fillRect(0, 0, w, h);

  const short = Math.min(w, h);
  const maxW = w * 0.8;
  // Shrink long titles until they fit in three lines.
  let size_ = Math.round(short * 0.1);
  let lines: string[] = [];
  for (; size_ > short * 0.04; size_ = Math.round(size_ * 0.9)) {
    c.font = cssFont('playfair-display', 700, size_);
    lines = wrap(c, title, maxW);
    if (lines.length <= 3 && lines.every((l) => c.measureText(l).width <= maxW)) break;
  }
  const lineH = size_ * 1.18;
  const kickerSize = Math.round(short * 0.028);
  const blockH = lines.length * lineH + (kicker ? kickerSize * 2.2 : 0);
  let y = (h - blockH) / 2;
  c.textAlign = 'center';
  c.textBaseline = 'top';
  if (kicker) {
    c.font = cssFont('inter', 500, kickerSize);
    c.fillStyle = 'rgba(255,255,255,0.62)';
    if ('letterSpacing' in c) (c as CanvasRenderingContext2D & { letterSpacing: string }).letterSpacing = `${Math.round(kickerSize * 0.18)}px`;
    c.fillText(kicker.toUpperCase(), w / 2, y);
    if ('letterSpacing' in c) (c as CanvasRenderingContext2D & { letterSpacing: string }).letterSpacing = '0px';
    y += kickerSize * 2.2;
  }
  c.font = cssFont('playfair-display', 700, size_);
  c.fillStyle = '#f6f1ea';
  for (const l of lines) {
    c.fillText(l, w / 2, y);
    y += lineH;
  }
  // A thin rule under the title.
  c.fillStyle = 'rgba(246,241,234,0.45)';
  c.fillRect(w / 2 - short * 0.04, y + size_ * 0.25, short * 0.08, Math.max(2, short * 0.002));
  const blob = await new Promise<Blob>((res, rej) => canvas.toBlob((b) => (b ? res(b) : rej(new Error('Could not draw the title card'))), 'image/png'));
  return { kind: 'card', data: await blobToBase64(blob), framing: 'fill' };
}

/** Library selection → items, in selection order (audio files are left out). */
export function itemsFromFiles(files: FileItem[]): VideoItem[] {
  const out: VideoItem[] = [];
  for (const f of files) {
    if (f.kind === 'image') out.push({ kind: 'image', libraryId: f.libraryId, path: f.path });
    else if (f.kind === 'video' || f.kind === 'gif') out.push({ kind: 'video', libraryId: f.libraryId, path: f.path });
  }
  return out;
}

function itemFromRef(m: MediaRef): VideoItem | null {
  const e = ext(m.path);
  if (AUDIO_EXT.has(e)) return null;
  if (VIDEO_EXT.has(e)) return { kind: 'video', libraryId: m.libraryId, path: m.path, start: m.startAt, end: m.endAt };
  return { kind: 'image', libraryId: m.libraryId, path: m.path, crop: m.crop, flipX: m.flipX, flipY: m.flipY };
}

/** How many photos and clips a story would put into a video. */
export function storyMediaCount(doc: Doc): number {
  return doc.sections.reduce((n, s) => n + s.media.filter((m) => itemFromRef(m)).length, 0);
}

/**
 * Story → items: an opening card with the story's title, then for each section
 * its title as a card (when it has one) and its photos and clips in order, with
 * their crops, flips and clip in/out points.
 */
export async function itemsFromStory(doc: Doc, size: VideoSize, titleCards: boolean): Promise<VideoItem[]> {
  const out: VideoItem[] = [];
  if (titleCards && doc.title.trim()) out.push(await titleCard(size, doc.title.trim()));
  for (const s of doc.sections) {
    const media = s.media.map(itemFromRef).filter((m): m is VideoItem => m !== null);
    const title = s.title?.trim();
    // A section without photos still gets its card when it's a chapter heading.
    if (titleCards && title && (media.length || s.chapter)) out.push(await titleCard(size, title, s.chapter ? 'Chapter' : undefined));
    out.push(...media);
  }
  return out;
}

/**
 * Collage → items: one slide per page, rendered by the collage renderer. With
 * Ken Burns the pages are drawn twice as large, so the zoom stays sharp.
 */
export async function itemsFromCollage(
  doc: CollageDoc,
  pages: number[],
  size: VideoSize,
  kenBurns: boolean,
  onProgress?: (done: number, total: number) => void,
): Promise<VideoItem[]> {
  const { w, h } = FRAME[size];
  const k = kenBurns ? 2 : 1;
  const out: VideoItem[] = [];
  for (const [n, i] of pages.entries()) {
    onProgress?.(n, pages.length);
    const blob = await renderPageBlob(pageDoc(doc, i), w * k, h * k, 'image/jpeg');
    out.push({ kind: 'card', data: await blobToBase64(blob), kenBurns });
  }
  onProgress?.(pages.length, pages.length);
  return out;
}
