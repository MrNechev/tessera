// Find and replace across a document: section titles, rich text, captions, speaker notes,
// and canvas notes/labels/frame titles. Matches inside one text run (a word split by
// formatting, e.g. half bold, is not found as one).
import type { Doc } from './types';

export interface FindOpts {
  caseSensitive: boolean;
  wholeWord: boolean;
}

export interface Match {
  sectionId?: string;
  itemId?: string;
  where: string;
  before: string;
  text: string;
  after: string;
}

function rx(q: string, o: FindOpts) {
  const esc = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(o.wholeWord ? `(?<![\\p{L}\\p{N}_])${esc}(?![\\p{L}\\p{N}_])` : esc, 'gu' + (o.caseSensitive ? '' : 'i'));
}

type Slot = { get: () => string; set: (v: string) => void; sectionId?: string; itemId?: string; where: string };

function slots(doc: Doc): Slot[] {
  const out: Slot[] = [];
  for (const s of doc.sections) {
    out.push({ get: () => s.title ?? '', set: (v) => (s.title = v), sectionId: s.id, where: 'title' });
    const walk = (n: any) => {
      if (!n || typeof n !== 'object') return;
      if (typeof n.text === 'string') out.push({ get: () => n.text, set: (v) => (n.text = v), sectionId: s.id, where: 'text' });
      if (Array.isArray(n.content)) n.content.forEach(walk);
    };
    walk(s.text);
    s.media.forEach((m) => out.push({ get: () => m.caption ?? '', set: (v) => (m.caption = v || undefined), sectionId: s.id, where: 'caption' }));
    out.push({ get: () => s.notes ?? '', set: (v) => (s.notes = v || undefined), sectionId: s.id, where: 'notes' });
  }
  for (const it of doc.canvas?.items ?? []) {
    if (it.type === 'note') out.push({ get: () => it.text, set: (v) => (it.text = v), itemId: it.id, where: 'canvas note' });
    else if (it.type === 'frame') out.push({ get: () => it.title, set: (v) => (it.title = v), itemId: it.id, where: 'frame title' });
    else if ('text' in it) out.push({ get: () => it.text ?? '', set: (v) => (it.text = v || undefined), itemId: it.id, where: 'canvas label' });
  }
  return out;
}

export function findAll(doc: Doc, q: string, o: FindOpts): Match[] {
  if (!q) return [];
  const r = rx(q, o);
  const out: Match[] = [];
  for (const sl of slots(doc)) {
    const t = sl.get();
    for (const m of t.matchAll(r)) {
      const i = m.index ?? 0;
      out.push({ sectionId: sl.sectionId, itemId: sl.itemId, where: sl.where, before: t.slice(Math.max(0, i - 40), i), text: m[0], after: t.slice(i + m[0].length, i + m[0].length + 40) });
      if (out.length > 5000) return out;
    }
  }
  return out;
}

/** Replace every match; returns how many were replaced. */
export function replaceAll(doc: Doc, q: string, repl: string, o: FindOpts): number {
  if (!q) return 0;
  const r = rx(q, o);
  let n = 0;
  for (const sl of slots(doc)) {
    const t = sl.get();
    const next = t.replace(r, () => {
      n++;
      return repl;
    });
    if (next !== t) sl.set(next);
  }
  return n;
}

/** Replace only the k-th match (0-based, in findAll order). */
export function replaceOne(doc: Doc, q: string, repl: string, o: FindOpts, k: number): boolean {
  if (!q) return false;
  const r = rx(q, o);
  let seen = 0;
  for (const sl of slots(doc)) {
    const t = sl.get();
    let done = false;
    const next = t.replace(r, (m) => {
      if (!done && seen++ === k) {
        done = true;
        return repl;
      }
      return m;
    });
    if (done) {
      sl.set(next);
      return true;
    }
  }
  return false;
}
