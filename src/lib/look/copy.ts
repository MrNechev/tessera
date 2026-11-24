/**
 * "Edit a copy": save a library photo with its edits as a new file in
 * Outputs/Edited. The photo becomes a one-element collage page of exactly the
 * edited size, so the collage exporter does the work: full resolution (RAW,
 * HEIC… decoded by the engine), the shared look pipeline, tiled rendering.
 */
import { api } from '../api/backend';
import { call } from '../api/http';
import type { Area, ImportedImage } from '../api/types';
import { exportDocument } from '../editor/exporter';
import { createDoc, createPhoto } from '../model/factory';
import type { CollageDoc } from '../model/types';
import { library } from '../stores/library.svelte';
import { straightenScale, type Crop, type Look } from './looks.svelte';

export type PhotoEdit = Look & { crop?: Crop };

/** A single-page collage that renders `img` exactly as edited. */
export function editedCopyDoc(img: ImportedImage, e: PhotoEdit): CollageDoc {
  const c = e.crop && e.crop.w > 0 && e.crop.h > 0 ? e.crop : { x: 0, y: 0, w: 1, h: 1 };
  const cw = Math.max(1, Math.round(img.width * c.w));
  const ch = Math.max(1, Math.round(img.height * c.h));
  const quarter = e.rotate === 90 || e.rotate === 270;
  const pw = quarter ? ch : cw;
  const ph = quarter ? cw : ch;
  const doc = createDoc({ name: img.name, width: pw, height: ph, dpi: 72 });
  doc.background = { ...doc.background, fill: { kind: 'solid', color: '#000000' } };
  doc.layout = { ...doc.layout, engine: 'none' };
  // the element has the crop's own orientation; turning it fills the page
  const s = e.straighten ? straightenScale(e.straighten, cw / ch) : 1;
  const w = cw * s;
  const h = ch * s;
  doc.assets[img.id] = { path: img.path, name: img.name, width: img.width, height: img.height };
  doc.elements.push(
    createPhoto({
      x: (pw - w) / 2,
      y: (ph - h) / 2,
      w,
      h,
      rotation: (e.rotate ?? 0) + (e.straighten ?? 0),
      asset: img.id,
      // cover-crop: zoom so the visible part is exactly the crop, focal point inside the overflow
      crop: { x: c.w < 1 ? c.x / (1 - c.w) : 0.5, y: c.h < 1 ? c.y / (1 - c.h) : 0.5, zoom: 1 / Math.max(c.w, c.h) },
      adjust: e.adjust ?? null,
      filter: e.filter ?? null,
      hideAreas: e.hideAreas ?? null,
      flipX: e.flipX,
      flipY: e.flipY,
    }),
  );
  return doc;
}

/** Save the edited copy into Outputs/Edited and return its path. */
export async function saveEditedCopy(path: string, e: PhotoEdit): Promise<string> {
  const [img] = await library.import([path]);
  if (!img) throw new Error('This photo could not be opened.');
  const areas = await call<Area[]>('areas');
  const outputs = areas.find((a) => a.kind === 'outputs');
  if (!outputs) throw new Error('No Outputs folder: choose a save folder in Settings → Saving.');
  const dir = outputs.root.replace(/\/+$/, '') + '/Edited';
  await api.ensureDir(dir);
  const base = img.name.replace(/\.[^.]+$/, '');
  const res = await exportDocument(editedCopyDoc(img, e), { path: `${dir}/${base} edited.jpg`, format: 'jpeg', quality: 95, unique: true });
  return res.path;
}
