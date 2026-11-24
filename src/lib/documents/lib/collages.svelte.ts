/**
 * Live collages in stories. A collage section only references a project of the
 * app's project library (`{projectId, page}`); the project stays the single
 * source, so editing it in the collage editor changes every story that shows
 * it. Everything here works the same on the desktop and in the web app: the
 * project comes from the engine by id, its photos through `library` (the same
 * proxies and thumbnails the collage editor uses).
 */
import { api, fileUrl } from '../../api/backend';
import type { ProjectSummary } from '../../api/types';
import { ensureFont } from '../../assets/fonts';
import { editor } from '../../editor/editor.svelte';
import { flushAutosave, importLibraryRefs, newBlank, openEditor, parseProject } from '../../editor/session.svelte';
import { pageCount, pageDoc } from '../../model/multipage';
import type { CollageDoc } from '../../model/types';
import { AdjustCache } from '../../render/adjust';
import { renderDocument, type RenderResources } from '../../render/renderer';
import { app } from '../../stores/app.svelte';
import { library } from '../../stores/library.svelte';
import { stickers } from '../../stores/stickers.svelte';
import { toasts } from '../../stores/toasts.svelte';
import { resolver } from './resolver.svelte';
import { router } from './router.svelte';
import type { CollageRef, MediaRef, Section } from './types';

/** Where "Back to the story" in the collage editor leads. */
export interface StoryReturn {
  docId: string;
  sectionId: string;
  projectId: string;
  title: string;
}

class Collages {
  /** Bumped per project whenever it is saved: live views reload it. */
  rev = $state<Record<string, number>>({});
  /** Set while a collage opened from a story is in the editor. */
  returnTo = $state<StoryReturn | null>(null);
}

export const collages = new Collages();

const docs = new Map<string, Promise<CollageDoc>>();
const images = new Map<string, Promise<CollageImage | null>>();
/** Photo files that could not be imported this session (no toast storm on every render). */
const lostPaths = new Set<string>();

export function isCollageSection(s: Section): boolean {
  return s.layout === 'collage';
}

/** Called by the collage editor's autosave: views of this project redraw. */
export function collageSaved(projectId: string): void {
  docs.delete(projectId);
  for (const k of [...images.keys()]) if (k.startsWith(projectId + '|')) images.delete(k);
  collages.rev[projectId] = (collages.rev[projectId] ?? 0) + 1;
}

/**
 * The project, from the collage editor when it is open there (always the latest,
 * even before the autosave ran), otherwise from the project library. Rejects when
 * the project was deleted.
 */
export function loadCollage(projectId: string): Promise<CollageDoc> {
  if (editor.open && editor.doc.id === projectId) return Promise.resolve(editor.doc);
  let p = docs.get(projectId);
  if (!p) {
    p = api.loadProjectById(projectId).then(parseProject);
    // A failure (deleted project, server unreachable) is retried by the next view.
    p.catch(() => docs.get(projectId) === p && docs.delete(projectId));
    docs.set(projectId, p);
  }
  return p;
}

/** Asset ids drawn on a single page. */
function usedAssets(doc: CollageDoc): string[] {
  const ids = doc.elements.flatMap((e) => (e.kind === 'photo' && e.asset ? [e.asset] : []));
  if (doc.background.image) ids.push(doc.background.image.asset);
  return [...new Set(ids)];
}

/**
 * One page as a single-page document whose photos are known to `library` (imported
 * by path, like opening the project does). Only the page's own photos are imported,
 * not a whole photo book's.
 */
export async function collagePage(ref: CollageRef): Promise<CollageDoc> {
  const full = await loadCollage(ref.projectId);
  const doc = pageDoc(full, Math.min(ref.page ?? 0, pageCount(full) - 1));
  const assets = { ...doc.assets };
  const wanted: typeof assets = {};
  for (const id of usedAssets(doc)) if (assets[id] && !library.has(id) && !lostPaths.has(assets[id].path)) wanted[id] = assets[id];
  if (Object.keys(wanted).length) {
    const lost = await library.ensureAssets(wanted);
    for (const id of lost) lostPaths.add(wanted[id].path);
    Object.assign(assets, wanted);
  }
  return { ...doc, assets };
}

/** Fonts and stickers the page draws with; resolves when they are loaded. */
export function pageExtras(doc: CollageDoc): Promise<unknown> {
  const fonts: Promise<void>[] = [];
  const ids: string[] = [];
  for (const el of doc.elements) {
    if (el.kind === 'text') fonts.push(ensureFont(el.font, el.weight));
    if (el.kind === 'photo' && el.frame?.caption) fonts.push(ensureFont(el.frame.captionFont ?? 'caveat', 400));
    if (el.kind === 'sticker') ids.push(el.sticker);
    if (el.kind === 'calendar') fonts.push(ensureFont(el.titleFont, 700), ensureFont(el.font, 400), ensureFont(el.font, 600));
  }
  return Promise.all([...fonts, stickers.preload(ids)]);
}

export function collageResources(adjustCache: AdjustCache): RenderResources {
  return {
    image: (id, px) => library.entry(id, px),
    sticker: (id) => stickers.image(id),
    adjustCache,
  };
}

export interface CollageImage {
  url: string;
  width: number;
  height: number;
}

/**
 * The page as a JPEG data URL whose long side is at most `maxSide` px, for print,
 * PDF and PowerPoint. Photos come from the 2048 px proxies, which is plenty for a
 * collage that fills part of a page. Null when the project is gone.
 */
export function collageImage(ref: CollageRef, maxSide = 2400): Promise<CollageImage | null> {
  const key = `${ref.projectId}|${ref.page ?? 0}|${maxSide}`;
  let p = images.get(key);
  if (!p) {
    p = (async () => {
      try {
        const doc = await collagePage(ref);
        const scale = maxSide / Math.max(doc.page.width, doc.page.height);
        await Promise.all([library.preload(usedAssets(doc), Infinity), pageExtras(doc)]);
        const canvas = document.createElement('canvas');
        canvas.width = Math.max(1, Math.round(doc.page.width * scale));
        canvas.height = Math.max(1, Math.round(doc.page.height * scale));
        renderDocument(canvas.getContext('2d')!, doc, { scale, resources: collageResources(new AdjustCache(128)), placeholders: false });
        return { url: canvas.toDataURL('image/jpeg', 0.9), width: canvas.width, height: canvas.height };
      } catch {
        return null;
      }
    })();
    images.set(key, p);
  }
  return p;
}

/** Open the section's collage in the collage editor; the editor then offers the way back. */
export async function editCollage(ref: CollageRef, back: Omit<StoryReturn, 'projectId'> | null): Promise<void> {
  try {
    await flushAutosave();
    if (!(editor.open && editor.doc.id === ref.projectId)) {
      const doc = parseProject(await api.loadProjectById(ref.projectId));
      const missing = await library.ensureAssets(doc.assets);
      openEditor(doc, null, false);
      if (missing.length)
        toasts.push({
          kind: 'warning',
          title: `${missing.length} photo${missing.length > 1 ? 's are' : ' is'} missing`,
          message: 'They were moved or deleted. Drop new photos onto the empty frames.',
        });
    }
    if (ref.page) editor.goToPage(Math.min(ref.page, editor.pageCount - 1));
    collages.returnTo = back ? { ...back, projectId: ref.projectId } : null;
    app.view = 'editor';
  } catch (e) {
    toasts.push({ kind: 'error', title: 'Could not open the collage', message: String(e instanceof Error ? e.message : e) });
  }
}

/** "Back to the story": save the collage first so the story shows the changes. */
export async function backToStory(): Promise<void> {
  const r = collages.returnTo;
  if (!r) return;
  collages.returnTo = null;
  await flushAutosave();
  router.go(`/doc/${r.docId}?s=${r.sectionId}`);
  app.view = 'documents';
}

/** A reference to the project open in the collage editor (after creating one for a story). */
export function refToOpenProject(page = 0): CollageRef {
  const d = editor.doc;
  return { projectId: d.id, page: page || undefined, aspect: d.page.width / d.page.height, name: d.name };
}

/** What the collage picker returns: an existing project's page, or a new collage. */
export type CollageChoice = { kind: 'project'; ref: CollageRef } | { kind: 'new'; photos: MediaRef[]; name: string };

/**
 * Carry out the picker's choice: `link` gets the reference to put into the section.
 * A new collage (from the section's photos, or blank) is created and opened in the
 * collage editor, and `link` runs in the same tick, before the story view unmounts
 * and saves, so the link is never lost. Returns whether a new collage was opened.
 */
export async function applyChoice(choice: CollageChoice, link: (ref: CollageRef) => void): Promise<boolean> {
  if (choice.kind === 'project') {
    link(choice.ref);
    return false;
  }
  let ids: string[] = [];
  if (choice.photos.length) {
    ids = await importLibraryRefs(choice.photos.map((m) => ({ libraryId: m.libraryId, path: m.path, kind: resolver.cache.get(m.libraryId + '\u0000' + m.path)?.file?.kind })));
    if (!ids.length) return false;
  }
  newBlank();
  editor.rename(choice.name || 'Collage');
  if (ids.length) editor.addToLayout(ids);
  link(refToOpenProject());
  return true;
}

/** Thumbnail URL of a project in the library (cache-busted by its modification time). */
export function projectThumb(p: ProjectSummary): string {
  return p.thumb ? fileUrl(p.thumb, p.modified) : '';
}

export function listProjects(): Promise<ProjectSummary[]> {
  return api.listProjects();
}
