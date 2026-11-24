/**
 * Cut-outs in the collage editor: a cut-out is a photo element without
 * frame, border or background whose asset is the transparent PNG — it keeps
 * its alpha through proxies (PNG), the renderer and the export.
 */
import { editor } from '../editor/editor.svelte';
import { importLibraryRefs } from '../editor/session.svelte';
import { createPhoto } from '../model/factory';
import type { PhotoElement } from '../model/types';
import { library } from '../stores/library.svelte';
import { openCutout } from './open.svelte';

/** Fit an aspect ratio into a square of `size`. */
function fit(aspect: number, size: number): { w: number; h: number } {
  return aspect >= 1 ? { w: size, h: size / aspect } : { w: size * aspect, h: size };
}

/**
 * Add a library file (usually Outputs/Cut-outs/…) to the page like a
 * sticker. `at`/`size` default to the page centre and a third of the page.
 */
export async function addCutout(ref: { libraryId: string; path: string }, at?: { x: number; y: number }, size?: number): Promise<void> {
  const [id] = await importLibraryRefs([ref]);
  if (!id) return;
  const a = library.assetRef(id);
  const { w, h } = fit(a ? a.width / a.height : 1, size ?? editor.pageShort * 0.34);
  // Cascade repeated inserts a little so they don't stack exactly.
  const n = at ? 0 : editor.doc.elements.filter((e) => e.kind === 'photo' && e.cutout).length % 6;
  const off = n * editor.pageShort * 0.02;
  const c = at ?? { x: editor.doc.page.width / 2 + off, y: editor.doc.page.height / 2 + off };
  editor.place(createPhoto({ x: 0, y: 0, w, h, asset: id, cutout: true }), c);
}

/** Put a cut-out where `el` was, about as large, and select it. */
export function addCutoutOver(el: PhotoElement, ref: { libraryId: string; path: string }): Promise<void> {
  return addCutout(ref, { x: el.x + el.w / 2, y: el.y + el.h / 2 }, Math.max(el.w, el.h));
}

/**
 * Turn a free photo into its cut-out in place: same centre and rotation,
 * fitted into the old box, without frame or border.
 */
export async function replaceWithCutout(elId: string, ref: { libraryId: string; path: string }): Promise<void> {
  const [id] = await importLibraryRefs([ref]);
  const a = id ? library.assetRef(id) : null;
  if (!id || !a) return;
  editor.update((d) => {
    editor.ensureAsset(d, id);
    const el = d.elements.find((e) => e.id === elId);
    if (!el || el.kind !== 'photo') return;
    const cx = el.x + el.w / 2;
    const cy = el.y + el.h / 2;
    const aspect = a.width / a.height;
    const { w, h } = aspect >= el.w / el.h ? { w: el.w, h: el.w / aspect } : { w: el.h * aspect, h: el.h };
    Object.assign(el, { asset: id, cutout: true, frame: null, border: null, mask: { shape: 'rect', radius: 0 }, crop: { x: 0.5, y: 0.5, zoom: 1 }, x: cx - w / 2, y: cy - h / 2, w, h });
  });
  editor.select([elId]);
}

/** "Cut out…" on a photo of the collage (Inspector). */
export function cutOutPhoto(el: PhotoElement): void {
  const a = el.asset ? editor.doc.assets[el.asset] : null;
  if (!a) return;
  openCutout({
    source: { path: a.path, name: a.name },
    // Laid-out photos belong to the auto-layout; the cut-out goes on top instead.
    actions: el.slot
      ? [{ id: 'add', label: 'Add on top' }]
      : [
          { id: 'add', label: 'Add as a sticker' },
          { id: 'replace', label: 'Replace the photo' },
        ],
    onsaved: (r, action) => void (action === 'replace' ? replaceWithCutout(el.id, r.ref) : addCutoutOver(el, r.ref)),
  });
}

/** "Cut out" on one of the collage's imported photos (Photos panel). */
export function cutOutImported(img: { path: string; name: string }): void {
  openCutout({
    source: { path: img.path, name: img.name },
    actions: [{ id: 'add', label: 'Add to collage' }],
    onsaved: (r) => void addCutout(r.ref),
  });
}
