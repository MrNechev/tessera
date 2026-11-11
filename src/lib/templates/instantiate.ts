import { relayout } from '../collage/layout';
import { createCalendar, createDoc, createPhoto, createShape, createSticker, createText, defaultLayout, uid } from '../model/factory';
import type { AssetRef, CollageDoc, CollageElement, PhotoElement } from '../model/types';
import { syncTextHeight } from '../render/text';
import type { SlotDef, TemplateDef } from './types';

/** Aspect ratios used for placeholder photos in previews. */
const PREVIEW_ASPECTS = [1.5, 0.75, 1, 1.33, 0.8, 1.5, 1.2, 0.67, 1.5, 1, 1.33, 0.75, 1.5, 1, 0.8, 1.33];

export interface InstantiateOptions {
  /** Photos to drop into the template, in order. */
  photos?: { id: string; ref: AssetRef }[];
  /** Sticker aspect lookup (width / height). */
  stickerAspect: (id: string) => number;
  /** Fill auto layouts with placeholder frames when there are no photos. */
  placeholders?: boolean;
}

function slotPhoto(def: Omit<SlotDef, 'x' | 'y' | 'w' | 'h'>, S: number): Partial<PhotoElement> {
  return {
    rotation: def.rotation ?? 0,
    mask: def.mask ?? { shape: 'rect', radius: 0 },
    frame: def.frame ? { ...def.frame } : null,
    border: def.border ? { width: def.border.width * S, color: def.border.color } : null,
    shadow: def.shadow ? { ...def.shadow } : null,
  };
}

export function instantiateTemplate(t: TemplateDef, opts: InstantiateOptions): CollageDoc {
  const { width: W, height: H } = t.page;
  const S = Math.min(W, H);
  const doc = createDoc({
    name: t.name,
    width: W,
    height: H,
    dpi: t.page.dpi ?? 300,
    preset: t.page.preset,
    background: structuredClone(t.background),
  });
  doc.template = t.id;
  doc.frame = t.frame ? { ...t.frame } : null;
  const photos = opts.photos ?? [];
  for (const p of photos) doc.assets[p.id] = p.ref;

  const below: CollageElement[] = [];
  const above: CollageElement[] = [];
  for (const it of t.items ?? []) {
    if (it.kind === 'shape') {
      const el = createShape({
        x: it.x * W,
        y: it.y * H,
        w: it.w * W,
        h: it.h * H,
        shape: it.shape,
        fill: it.fill ? structuredClone(it.fill) : null,
        stroke: it.stroke ? { width: it.stroke.width * S, color: it.stroke.color } : null,
        radius: it.radius ?? 0,
        rotation: it.rotation ?? 0,
        opacity: it.opacity ?? 1,
        shadow: it.shadow ?? null,
      });
      (it.below ? below : above).push(el);
    } else if (it.kind === 'calendar') {
      const now = new Date();
      above.push(
        createCalendar({
          x: it.x * W,
          y: it.y * H,
          w: it.w * W,
          h: it.h * H,
          month: it.month ?? now.getMonth() + 1,
          year: it.year ?? now.getFullYear(),
          ...(it.props ?? {}),
        }),
      );
    } else if (it.kind === 'sticker') {
      const aspect = opts.stickerAspect(it.sticker) || 1;
      const w = it.size * S;
      const h = w / aspect;
      above.push(
        createSticker({
          sticker: it.sticker,
          x: it.cx * W - w / 2,
          y: it.cy * H - h / 2,
          w,
          h,
          rotation: it.rotation ?? 0,
          opacity: it.opacity ?? 1,
          flipX: it.flipX,
          shadow: it.shadow ?? null,
        }),
      );
    } else {
      const size = it.size * S;
      const el = createText({
        text: it.text,
        x: 0,
        y: 0,
        w: it.w * W,
        font: it.font,
        size,
        fill: it.color,
        weight: it.weight ?? 400,
        italic: it.italic ?? false,
        align: it.align ?? 'center',
        lineHeight: it.lineHeight ?? 1.15,
        letterSpacing: it.letterSpacing ?? 0,
        uppercase: it.uppercase ?? false,
        curve: it.curve ?? 0,
        rotation: it.rotation ?? 0,
        stroke: it.stroke ? { width: it.stroke.width * size, color: it.stroke.color } : null,
        shadow: it.shadow ?? null,
        background: it.background ?? null,
      });
      syncTextHeight(el);
      el.x = it.cx * W - el.w / 2;
      el.y = it.cy * H - el.h / 2;
      above.push(el);
    }
  }

  let photoEls: PhotoElement[] = [];
  if (t.slots?.length) {
    photoEls = t.slots.map((s, i) =>
      createPhoto({
        x: s.x * W,
        y: s.y * H,
        w: s.w * W,
        h: s.h * H,
        asset: photos[i]?.id ?? null,
        ...slotPhoto(s, S),
      }),
    );
  }

  doc.elements = [...below, ...photoEls, ...above];

  if (t.layout && t.layout.engine !== 'none') {
    doc.layout = { ...defaultLayout(t.layout.engine), ...t.layout };
    let ids = photos.map((p) => p.id);
    const fake = !ids.length && opts.placeholders !== false;
    if (fake) {
      const n = t.previewPhotos ?? 9;
      ids = Array.from({ length: n }, (_, i) => `__placeholder_${i}`);
      ids.forEach((id, i) => {
        const a = PREVIEW_ASPECTS[i % PREVIEW_ASPECTS.length];
        doc.assets[id] = { path: '', name: 'placeholder', width: Math.round(1000 * a), height: 1000 };
      });
    }
    // Seed managed photos carrying the template's photo style, then lay out.
    const style = t.photoStyle ? slotPhoto(t.photoStyle, S) : {};
    const seeds = ids.map((id) => createPhoto({ x: 0, y: 0, w: 1, h: 1, asset: id, slot: true, ...style }));
    const firstAbove = doc.elements.length - above.length;
    doc.elements = [...doc.elements.slice(0, firstAbove), ...seeds, ...doc.elements.slice(firstAbove)];
    let laid = relayout(doc);
    if (fake) {
      // Turn the preview photos into empty frames that disappear once real photos arrive.
      laid = {
        ...laid,
        assets: Object.fromEntries(Object.entries(laid.assets).filter(([k]) => !k.startsWith('__placeholder_'))),
        elements: laid.elements.map((e) =>
          e.kind === 'photo' && e.asset?.startsWith('__placeholder_')
            ? { ...e, asset: null, slot: false, name: AUTO_PLACEHOLDER, id: uid('ph') }
            : e,
        ),
      };
    }
    return laid;
  }
  return doc;
}

/** Name given to auto-generated placeholder frames of layout templates. */
export const AUTO_PLACEHOLDER = '__auto_placeholder';

/** Preview document for gallery thumbnails. */
export function previewDoc(t: TemplateDef, stickerAspect: (id: string) => number): CollageDoc {
  const doc = instantiateTemplate(t, { stickerAspect, placeholders: true });
  return doc;
}
