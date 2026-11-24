// Diary and story-starter actions: the API side of diary.ts (which stays pure and tested).
import { editor } from '../../editor/editor.svelte';
import { importLibraryRefs, newBlank } from '../../editor/session.svelte';
import { app } from '../../stores/app.svelte';
import { api } from './api';
import { addToDiary, dayBounds, DIARY_TAG, draftStory, findDiary, findDay, type DraftOptions } from './diary';
import { router } from './router.svelte';
import type { DocMeta, FileItem } from './types';

/** Opens a story (at a section) from anywhere in the app. */
export function openStory(id: string, sectionId?: string) {
  app.view = 'documents';
  router.go(`/doc/${id}${sectionId ? `?s=${sectionId}` : ''}`);
}

/**
 * Opens the diary at a day, creating the diary story and the day's chapter when needed, and
 * adds `files` to the day. One diary with a chapter per day rather than a story per day: the
 * story list stays readable after a year of entries, the outline lists the days, and the
 * whole diary presents and exports as one chronicle.
 */
export async function openDiaryDay(iso: string, files: FileItem[]) {
  const meta = findDiary(await api.docs());
  const doc = meta ? await api.getDoc(meta.id) : await api.importDoc({ title: 'Diary', tags: [DIARY_TAG], sections: [] });
  const before = doc.sections.length + ':' + doc.sections.reduce((n, s) => n + s.media.length, 0);
  const sid = addToDiary(doc, iso, files);
  if (doc.sections.length + ':' + doc.sections.reduce((n, s) => n + s.media.length, 0) !== before) await api.saveDoc(doc.id, doc.version, doc);
  openStory(doc.id, sid);
}

/** Opens a story at the section dated `iso` (its chapter if there is one). */
export async function openStoryAt(meta: DocMeta, iso: string) {
  try {
    const doc = await api.getDoc(meta.id);
    const i = findDay(doc, iso);
    openStory(meta.id, (i >= 0 ? doc.sections[i] : doc.sections.find((s) => s.date === iso))?.id);
  } catch {
    openStory(meta.id);
  }
}

export interface RangeQuery {
  from: string;
  to: string;
  library?: string;
  dir?: string;
  videos?: boolean;
}

const rangeParams = (q: RangeQuery) => ({
  ...dayBounds(q.from, q.to),
  library: q.library || undefined,
  dir: q.library ? q.dir || undefined : undefined,
  recursive: true,
  kind: q.videos === false ? 'image' : 'image,video',
});

/** How many files a range has (for the starter's preview). */
export async function countRange(q: RangeQuery, signal?: AbortSignal): Promise<number> {
  return (await api.files({ ...rangeParams(q), limit: 1 }, signal)).total;
}

/** Every file of a range in time order, with perceptual hashes; at most `cap`. */
export async function filesInRange(q: RangeQuery, cap = 6000): Promise<{ items: FileItem[]; total: number }> {
  const items: FileItem[] = [];
  let total = 0;
  for (let offset = 0; offset < cap; offset += 2000) {
    const r = await api.files({ ...rangeParams(q), sort: 'taken', phash: true, offset, limit: Math.min(2000, cap - offset) });
    total = r.total;
    items.push(...r.items);
    if (items.length >= r.total || !r.items.length) break;
  }
  return { items, total };
}

/** Drafts a story from a range and opens it. Returns false when the range has no media. */
export async function draftAndOpen(q: RangeQuery, opts: DraftOptions): Promise<boolean> {
  const { items } = await filesInRange(q);
  if (!items.length) return false;
  const d = await api.importDoc(draftStory(items, opts));
  openStory(d.id);
  return true;
}

/** A collage editor filled with these library photos (videos are skipped). */
export async function collageOf(files: FileItem[], title: string) {
  const ids = await importLibraryRefs(files.map((f) => ({ libraryId: f.libraryId, path: f.path, kind: f.kind })));
  if (!ids.length) return;
  newBlank();
  editor.rename(title);
  editor.addToLayout(ids);
  app.view = 'editor';
}
