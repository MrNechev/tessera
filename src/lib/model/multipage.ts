/**
 * Multi-page projects. The editor, renderer and layouts all work on the
 * top-level page fields of a CollageDoc; other pages are parked in
 * `doc.pages`. These pure helpers move content in and out of that slot.
 */
import { defaultBackground, defaultLayout, uid } from './factory';
import type { CollageDoc, PageContent } from './types';

export function pageCount(doc: CollageDoc): number {
  return doc.pages?.length ?? 1;
}

export function activePage(doc: CollageDoc): number {
  return doc.pages ? Math.min(doc.activePage ?? 0, doc.pages.length - 1) : 0;
}

/** The active page's content, taken from the top-level fields. */
export function currentContent(doc: CollageDoc): PageContent {
  const id = doc.pages ? (doc.activePageId ?? `${doc.id}-p${activePage(doc)}`) : `${doc.id}-p0`;
  return { id, background: doc.background, frame: doc.frame, layout: doc.layout, elements: doc.elements };
}

/** All pages in order, with the active one filled in. */
export function allPages(doc: CollageDoc): PageContent[] {
  if (!doc.pages) return [currentContent(doc)];
  const active = activePage(doc);
  return doc.pages.map((p, i) => (i === active || !p ? currentContent(doc) : p));
}

/** Rebuild a document from a page list and the index to make active. */
export function fromPages(doc: CollageDoc, pages: PageContent[], active: number): CollageDoc {
  const i = Math.max(0, Math.min(pages.length - 1, active));
  const p = pages[i];
  const out: CollageDoc = {
    ...doc,
    background: p.background,
    frame: p.frame,
    layout: p.layout,
    elements: p.elements,
  };
  if (pages.length > 1) {
    out.pages = pages.map((q, k) => (k === i ? null : q));
    out.activePage = i;
    out.activePageId = p.id;
  } else {
    delete out.pages;
    delete out.activePage;
    delete out.activePageId;
  }
  return out;
}

/** A single-page document showing page `i` (for thumbnails, export and print). */
export function pageDoc(doc: CollageDoc, i: number): CollageDoc {
  const pages = allPages(doc);
  const p = pages[Math.max(0, Math.min(pages.length - 1, i))];
  const out: CollageDoc = { ...doc, background: p.background, frame: p.frame, layout: p.layout, elements: p.elements };
  delete out.pages;
  delete out.activePage;
  delete out.activePageId;
  return out;
}

export function switchPage(doc: CollageDoc, i: number): CollageDoc {
  if (i === activePage(doc) || !doc.pages) return doc;
  return fromPages(doc, allPages(doc), i);
}

export function blankPage(like?: PageContent): PageContent {
  return {
    id: uid('pg'),
    background: like ? structuredClone(like.background) : defaultBackground(),
    frame: like?.frame ? structuredClone(like.frame) : null,
    layout: { ...defaultLayout(like?.layout.engine === 'none' ? 'none' : (like?.layout.engine ?? 'none')), spacing: like?.layout.spacing ?? 0.012, margin: like?.layout.margin ?? 0.012 },
    elements: [],
  };
}

/** Insert pages after `after` (default: the active page) and make the first one active. */
export function insertPages(doc: CollageDoc, newPages: PageContent[], after = activePage(doc)): CollageDoc {
  const pages = allPages(doc);
  pages.splice(after + 1, 0, ...newPages);
  return fromPages(doc, pages, after + 1);
}

export function duplicatePage(doc: CollageDoc, i = activePage(doc)): CollageDoc {
  const pages = allPages(doc);
  const copy = structuredClone(pages[i]);
  copy.id = uid('pg');
  copy.elements = copy.elements.map((e) => ({ ...e, id: uid(e.id.split('_')[0] || 'e') }));
  pages.splice(i + 1, 0, copy);
  return fromPages(doc, pages, i + 1);
}

export function removePage(doc: CollageDoc, i = activePage(doc)): CollageDoc {
  const pages = allPages(doc);
  if (pages.length <= 1) return doc;
  pages.splice(i, 1);
  const active = activePage(doc);
  return fromPages(doc, pages, i < active ? active - 1 : Math.min(active, pages.length - 1));
}

export function movePage(doc: CollageDoc, from: number, to: number): CollageDoc {
  const pages = allPages(doc);
  if (from === to || from < 0 || to < 0 || from >= pages.length || to >= pages.length) return doc;
  const activeId = pages[activePage(doc)].id;
  const [p] = pages.splice(from, 1);
  pages.splice(to, 0, p);
  return fromPages(doc, pages, pages.findIndex((q) => q.id === activeId));
}

/** Replace the active page's content (e.g. applying a template to one page). */
export function replaceCurrent(doc: CollageDoc, content: Omit<PageContent, 'id'>): CollageDoc {
  const pages = allPages(doc);
  const i = activePage(doc);
  pages[i] = { ...content, id: pages[i].id };
  return fromPages(doc, pages, i);
}
