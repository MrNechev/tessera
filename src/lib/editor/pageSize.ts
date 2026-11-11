/** Changing the page size of a document, scaling every page's content. */
import { produce } from 'immer';
import { relayout } from '../collage/layout';
import type { CollageDoc, CollageElement, TextElement } from '../model/types';
import { allPages, fromPages, activePage, pageDoc } from '../model/multipage';
import { syncTextHeight } from '../render/text';
import { center } from './geometry';

/** Resize a single-page document (pages ignored), keeping relative positions. */
function resizeOne(doc: CollageDoc, width: number, height: number, dpi: number, preset?: string): CollageDoc {
  const old = doc.page;
  const sx = width / old.width;
  const sy = height / old.height;
  const s = Math.sqrt(sx * sy);
  let out = produce(doc, (d) => {
    d.page = { width, height, dpi, preset };
    for (const e of d.elements) {
      const c = center(e as CollageElement);
      const w = e.w * s;
      const h = e.h * s;
      e.x = c.x * sx - w / 2;
      e.y = c.y * sy - h / 2;
      e.w = w;
      e.h = h;
      if (e.kind === 'text') {
        e.size *= s;
        if (e.stroke) e.stroke.width *= s;
        syncTextHeight(e as TextElement);
      }
      if (e.kind === 'photo' && e.border) e.border.width *= s;
      if (e.kind === 'shape' && e.stroke) e.stroke.width *= s;
    }
  });
  if (out.layout.engine !== 'none') out = relayout(out);
  return out;
}

/** Resize every page of `doc`. */
export function resizeDoc(doc: CollageDoc, width: number, height: number, dpi: number, preset?: string): CollageDoc {
  if (!doc.pages) return resizeOne(doc, width, height, dpi, preset);
  const pages = allPages(doc).map((p, i) => {
    const r = resizeOne(pageDoc(doc, i), width, height, dpi, preset);
    return { id: p.id, background: r.background, frame: r.frame, layout: r.layout, elements: r.elements };
  });
  return fromPages({ ...doc, page: { width, height, dpi, preset } }, pages, activePage(doc));
}

/** Fit a document made at another size (e.g. a template) onto this page size. */
export function fitToPage(doc: CollageDoc, page: CollageDoc['page']): CollageDoc {
  if (doc.page.width === page.width && doc.page.height === page.height) return { ...doc, page: { ...page } };
  return resizeOne(doc, page.width, page.height, page.dpi, page.preset);
}
