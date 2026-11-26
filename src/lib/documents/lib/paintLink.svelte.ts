// Stories ↔ Paint: send a photo of a story (or a new, empty drawing) to Paint and bring the
// exported image back. The story editor is not mounted while Paint is open, so the result
// waits here until the story opens again (Editor.svelte calls applyPaintResult after load).
import { app } from '../../stores/app.svelte';
import { newPaint, onPaintExport, openPaint, type ExportResult } from '../../paint/session.svelte';
import { paint } from '../../paint/state.svelte';
import { defaultCardSize, snap } from './canvas';
import { newSection, type DocState } from './docstate.svelte';
import { api } from './api';
import { resolver } from './resolver.svelte';
import { router } from './router.svelte';
import { toasts as toast } from './toast.svelte';
import type { MediaRef } from './types';
import { editor } from '../../editor/editor.svelte';
import { replacePhotoAsset } from '../../cutout/collage';
import type { PhotoElement } from '../../model/types';

type Target =
  /** replace one photo of a section with the drawing */
  | {
      kind: 'replace';
      sectionId: string;
      old: { libraryId: string; path: string };
    }
  /** add the drawing as a new card at a canvas point (or a new section at the end) */
  | { kind: 'insert'; at?: { x: number; y: number } }
  /** replace a photo of the open collage (the collage editor stays mounted: applied at once) */
  | { kind: 'collage'; elId: string };

export type PaintJob = Target & {
  docId: string;
  title: string;
  hash: string;
  drawingId: string;
};

class PaintLink {
  /** the drawing that came from a story and where its export goes */
  job = $state<PaintJob | null>(null);
  /** an export waiting to go into its story */
  result = $state<(PaintJob & { ref: { libraryId: string; path: string } }) | null>(null);
}
export const paintLink = new PaintLink();

let listening = false;
/** settles once the latest export of a story/collage drawing is indexed and handed over */
let handover: Promise<void> = Promise.resolve();

/** The library watcher picks new files up within a few seconds; stories and collages can
 *  only show an export once it is indexed. */
async function indexed(ref: { libraryId: string; path: string }) {
  for (let i = 0; i < 40; i++) {
    const res = await api.resolve([ref]).catch(() => []);
    if (res[0]?.found && res[0].file) return;
    await new Promise((r) => setTimeout(r, 400));
  }
}

function listen() {
  if (listening) return;
  listening = true;
  onPaintExport((r: ExportResult & { drawingId: string }) => {
    const j = paintLink.job;
    if (!j || r.drawingId !== j.drawingId) return;
    handover = indexed(r.ref).then(async () => {
      if (j.kind === 'collage') {
        if (await replacePhotoAsset(j.elId, r.ref)) toast.show(`The photo in “${j.title}” now shows your drawing`, 'ok', 5000);
        return;
      }
      paintLink.result = { ...j, ref: r.ref };
    });
  });
}

/** Wait until the last export went to its story or collage (see backToStory). */
export function handedOver(): Promise<void> {
  return handover;
}

function here(docId: string, title: string) {
  const hash = location.hash.replace(/^#/, '') || `/doc/${docId}`;
  return { docId, title: title || 'the story', hash };
}

/** Paint on a photo of a section; the export replaces that photo. */
export async function paintPhoto(docId: string, title: string, sectionId: string, ref: { libraryId: string; path: string }, name?: string) {
  listen();
  const from = here(docId, title);
  const doc = await openPaint({
    libraryId: ref.libraryId,
    path: ref.path,
    name,
  });
  if (!doc) return;
  paintLink.result = null;
  paintLink.job = {
    ...from,
    kind: 'replace',
    sectionId,
    old: { libraryId: ref.libraryId, path: ref.path },
    drawingId: doc.id,
  };
}

/** A new, empty drawing; its export becomes a new card (at a canvas point) or section. */
export async function paintNew(docId: string, title: string, at?: { x: number; y: number }) {
  listen();
  const from = here(docId, title);
  const doc = await newPaint({
    width: 1600,
    height: 1200,
    background: 'transparent',
    name: `Drawing for ${title || 'a story'}`,
  });
  paintLink.result = null;
  paintLink.job = { ...from, kind: 'insert', at, drawingId: doc.id };
}

/** Paint on a photo of the open collage; each export replaces that photo. */
export async function paintCollagePhoto(el: PhotoElement) {
  const a = el.asset ? editor.doc.assets[el.asset] : null;
  if (!a) return;
  listen();
  const doc = await openPaint({ path: a.path, name: a.name });
  if (!doc) return;
  paintLink.result = null;
  paintLink.job = {
    kind: 'collage',
    elId: el.id,
    docId: '',
    title: editor.doc.name || 'the collage',
    hash: '',
    drawingId: doc.id,
  };
}

/** Is the open Paint drawing one that goes back into a story? */
export function storyJob(): PaintJob | null {
  const j = paintLink.job;
  return j && paint.doc?.id === j.drawingId ? j : null;
}

/** Leave Paint for the story the drawing came from. */
export function backToStory() {
  const j = paintLink.job;
  if (!j) return;
  if (j.kind === 'collage') {
    app.view = 'editor';
    return;
  }
  app.view = 'documents';
  router.go(j.hash);
}

/** Called by the story editor after loading: put a waiting export into this story. */
export function applyPaintResult(ds: DocState) {
  const r = paintLink.result;
  const doc = ds.doc;
  if (!r || !doc || r.kind === 'collage' || doc.id !== r.docId) return;
  paintLink.result = null;
  paintLink.job = null;
  const ref: MediaRef = { libraryId: r.ref.libraryId, path: r.ref.path };
  resolver.refresh(ref);
  if (r.kind === 'replace') {
    const sec = doc.sections.find((s) => s.id === r.sectionId);
    const i = sec ? sec.media.findIndex((m) => m.libraryId === r.old.libraryId && m.path === r.old.path) : -1;
    if (!sec || i < 0) {
      // the photo is gone: add the drawing at the end instead of losing it
      ds.addSection(null, newSection('media-top', [ref]));
      ds.touch();
      toast.show('The photo was no longer there, so the drawing became a new section', 'info', 5000);
      return;
    }
    const prev = sec.media[i];
    // the look was painted in already (Paint opens the photo as it is): keep the caption only
    sec.media[i] = { ...ref, caption: prev.caption };
    ds.touch();
    toast.show('Your drawing replaced the photo', 'ok', 6000, {
      label: 'Undo',
      run: () => {
        const s = ds.doc?.sections.find((x) => x.id === r.sectionId);
        const j = s ? s.media.findIndex((m) => m.libraryId === ref.libraryId && m.path === ref.path) : -1;
        if (s && j >= 0) {
          s.media[j] = prev;
          ds.touch();
        }
      },
    });
    return;
  }
  const s = newSection('media-top', [ref]);
  if (r.at) {
    const { w, h } = defaultCardSize(s);
    s.canvas = { x: snap(r.at.x - w / 2), y: snap(r.at.y - h / 2), w, h };
  }
  ds.addSection(null, s);
  ds.touch();
  toast.show(r.at ? 'Your drawing is on the canvas' : 'Your drawing was added as a new section', 'ok', 4000);
}
