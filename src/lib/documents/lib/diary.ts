// Stories as a diary: calendar days, the diary story, and drafting a story from the photos
// of a date range. Everything here is pure (no API calls) so it can be unit-tested; the
// components do the fetching.
//
// Days are ISO strings ("2026-11-24") in the viewer's local time zone: a photo taken at
// 23:30 belongs to the evening the person remembers, not to the server's UTC day.
import { newId } from './api';
import type { Doc, DocMeta, FileItem, MediaRef, Section } from './types';

export const HOUR = 3_600_000;

const pad = (n: number) => String(n).padStart(2, '0');

/** The local calendar day of a time (ms or Date). */
export function isoDay(t: number | Date): string {
  const d = typeof t === 'number' ? new Date(t) : t;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Local midnight at the start of a day. */
export function parseDay(iso: string): Date {
  const [y, m, d] = iso.split('-').map(Number);
  return new Date(y, m - 1, d);
}

export function addDays(iso: string, n: number): string {
  const d = parseDay(iso);
  d.setDate(d.getDate() + n);
  return isoDay(d);
}

/** [from, to) in ms for the files API; DST days are 23 or 25 hours long, hence Date arithmetic. */
export function dayBounds(fromIso: string, toIso = fromIso): { from: number; to: number } {
  return { from: parseDay(fromIso).getTime(), to: parseDay(addDays(toIso, 1)).getTime() };
}

/** Minutes east of UTC at a moment (what /api/days and the "md" filter expect). */
export function tzMinutes(at: Date = new Date()): number {
  return -at.getTimezoneOffset();
}

/** When a file was taken: its capture time, else its modification time (FileItem.mtime is ns). */
export function takenMs(f: Pick<FileItem, 'taken' | 'mtime'>): number {
  return f.taken || Math.floor(f.mtime / 1e6);
}

/** "Tuesday, 24 November 2026" */
export function fmtDayLong(iso: string, year = true): string {
  return parseDay(iso).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long', year: year ? 'numeric' : undefined });
}

/** "24 Nov 2026" */
export function fmtDayShort(iso: string): string {
  return parseDay(iso).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}

/** "3–7 November 2026", "30 Oct – 2 Nov 2026", "24 November 2026" */
export function fmtRange(from: string, to: string): string {
  if (from === to) return parseDay(from).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' });
  const a = parseDay(from);
  const b = parseDay(to);
  const sameYear = a.getFullYear() === b.getFullYear();
  if (sameYear && a.getMonth() === b.getMonth())
    return `${a.getDate()}–${b.toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}`;
  return `${a.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: sameYear ? undefined : 'numeric' })} – ${b.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' })}`;
}

/**
 * The weeks of a month for a calendar grid: rows of 7 ISO days starting on `weekStart`
 * (1 = Monday), padded with the neighbouring months' days. `month` is 0-based.
 */
export function monthGrid(year: number, month: number, weekStart = 1): string[][] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() - weekStart + 7) % 7;
  let day = addDays(isoDay(first), -lead);
  const last = isoDay(new Date(year, month + 1, 0));
  const weeks: string[][] = [];
  for (;;) {
    const w: string[] = [];
    for (let i = 0; i < 7; i++, day = addDays(day, 1)) w.push(day);
    weeks.push(w);
    if (w[6] >= last) return weeks;
  }
}

// ---------- the diary ----------

export const DIARY_TAG = 'diary';

/**
 * The diary is a story tagged "diary" (the most recently edited one if there are several).
 * A tag rather than a setting: it is visible, works the same on every device and in the web
 * app, and starting a fresh diary (say, one per year) is just moving the tag.
 */
export function findDiary(docs: DocMeta[]): DocMeta | undefined {
  return docs
    .filter((d) => d.tags?.some((t) => t.toLowerCase() === DIARY_TAG))
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))[0];
}

const para = (text = '') => ({ type: 'paragraph', content: text ? [{ type: 'text', text }] : undefined });
export const textDoc = (text = '') => ({ type: 'doc', content: [para(text)] });

/** The layout that shows `n` media well. */
function layoutFor(n: number): Pick<Section, 'layout' | 'gallery'> {
  if (n <= 1) return { layout: 'media-top' };
  return { layout: 'gallery', gallery: { mode: 'grid', columns: n === 2 || n === 4 ? 2 : 3 } };
}

/** "14:05 · Plovdiv, Bulgaria" (the date is already in the chapter heading). */
export function timePlace(f: Pick<FileItem, 'taken' | 'mtime' | 'place'>, withDate = false): string {
  const t = new Date(takenMs(f));
  const parts: string[] = [];
  if (withDate) parts.push(t.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }));
  if (f.taken) parts.push(t.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }));
  if (f.place) parts.push(f.place);
  return parts.join(' · ');
}

export function toRef(f: FileItem, caption = true): MediaRef {
  const c = caption ? timePlace(f, true) : '';
  return { libraryId: f.libraryId, path: f.path, ...(c ? { caption: c } : {}) };
}

/** A diary day: a chapter titled with the date, then a section for the photos and the words. */
export function diaryDay(iso: string, files: FileItem[]): Section[] {
  const place = commonPlace(files);
  return [
    { id: newId(), title: fmtDayLong(iso), layout: 'text-only', media: [], text: textDoc(), slide: true, chapter: true, date: iso, ...(place ? { place } : {}) },
    { id: newId(), title: '', ...layoutFor(files.length), media: files.map((f) => toRef(f)), text: textDoc(), slide: true, date: iso, ...(place ? { place } : {}) },
  ];
}

/** Index of the chapter for a day in a diary, or -1. */
export function findDay(doc: Pick<Doc, 'sections'>, iso: string): number {
  return doc.sections.findIndex((s) => s.chapter && s.date === iso);
}

/** Where a new day goes so the diary stays in date order: before the first later dated chapter. */
export function dayInsertAt(doc: Pick<Doc, 'sections'>, iso: string): number {
  const i = doc.sections.findIndex((s) => s.chapter && s.date && s.date > iso);
  return i < 0 ? doc.sections.length : i;
}

/**
 * Adds `files` to a day of the diary (creating the day if needed) and returns the id of the
 * section to show. Files already in that day are skipped. Mutates `doc`.
 */
export function addToDiary(doc: Pick<Doc, 'sections'>, iso: string, files: FileItem[]): string {
  const at = findDay(doc, iso);
  if (at < 0) {
    const secs = diaryDay(iso, files);
    doc.sections.splice(dayInsertAt(doc, iso), 0, ...secs);
    return secs[0].id;
  }
  if (files.length) {
    // the day's sections run until the next chapter; add to its last one with room, or a new one
    let end = at + 1;
    while (end < doc.sections.length && !doc.sections[end].chapter) end++;
    const have = new Set(doc.sections.slice(at, end).flatMap((s) => s.media.map((m) => m.libraryId + '\u0000' + m.path)));
    const fresh = files.filter((f) => !have.has(f.libraryId + '\u0000' + f.path));
    if (fresh.length) {
      const last = end - 1 > at ? doc.sections[end - 1] : null;
      if (last && last.layout !== 'compare' && last.layout !== 'text-only') {
        last.media.push(...fresh.map((f) => toRef(f)));
        Object.assign(last, layoutFor(last.media.length));
      } else doc.sections.splice(end, 0, diaryDay(iso, fresh)[1]);
    }
  }
  return doc.sections[at].id;
}

// ---------- drafting a story from photos ----------

/** Files grouped by local day, each day in time order; days in order. */
export function groupByDay(files: FileItem[]): { day: string; files: FileItem[] }[] {
  const sorted = [...files].sort((a, b) => takenMs(a) - takenMs(b));
  const out: { day: string; files: FileItem[] }[] = [];
  for (const f of sorted) {
    const d = isoDay(takenMs(f));
    if (out.at(-1)?.day !== d) out.push({ day: d, files: [] });
    out.at(-1)!.files.push(f);
  }
  return out;
}

/**
 * Splits one day (in time order) into moments: a new one after a pause longer than `gapMs`,
 * or when the place changes (both files know where they were taken and the places differ).
 */
export function splitMoments(files: FileItem[], gapMs = 2 * HOUR): FileItem[][] {
  const out: FileItem[][] = [];
  let place = '';
  for (const f of files) {
    const cur = out.at(-1);
    const prev = cur?.at(-1);
    const moved = !!(f.place && place && f.place !== place);
    if (!cur || !prev || takenMs(f) - takenMs(prev) > gapMs || moved) out.push([f]);
    else cur.push(f);
    if (f.place) place = f.place;
  }
  return out;
}

/** Hamming distance of two hex hashes (Infinity if either is missing). */
export function hamming(a?: string, b?: string): number {
  if (!a || !b || a.length !== b.length) return Infinity;
  let n = 0;
  for (let i = 0; i < a.length; i += 8) {
    let x = (parseInt(a.slice(i, i + 8), 16) ^ parseInt(b.slice(i, i + 8), 16)) >>> 0;
    while (x) {
      x &= x - 1;
      n++;
    }
  }
  return n;
}

const pixels = (f: FileItem) => (f.width || 0) * (f.height || 0);

/**
 * The best `n` of a moment: near-duplicates collapse to their largest shot (by perceptual
 * hash when the library has one, else bursts taken within `burstMs` of each other), then the
 * survivors are sampled evenly over time, so the picks show the whole moment rather than
 * five frames of the same second.
 */
export function pickBest(files: FileItem[], n: number, opts: { dupDist?: number; burstMs?: number } = {}): FileItem[] {
  const dupDist = opts.dupDist ?? 6;
  const burstMs = opts.burstMs ?? 3000;
  const kept: FileItem[] = [];
  for (const f of files) {
    const twin = kept.findIndex((k) =>
      k.phash && f.phash ? hamming(k.phash, f.phash) <= dupDist : k.kind === f.kind && Math.abs(takenMs(k) - takenMs(f)) <= burstMs,
    );
    if (twin < 0) kept.push(f);
    else if (pixels(f) > pixels(kept[twin])) kept[twin] = f;
  }
  if (kept.length <= n) return kept;
  if (n <= 1) return [kept[Math.floor(kept.length / 2)]];
  const out: FileItem[] = [];
  for (let i = 0; i < n; i++) out.push(kept[Math.round((i * (kept.length - 1)) / (n - 1))]);
  return out;
}

/** The place most files of a group share ("" if none has one). */
export function commonPlace(files: Pick<FileItem, 'place'>[]): string {
  const n = new Map<string, number>();
  for (const f of files) if (f.place) n.set(f.place, (n.get(f.place) ?? 0) + 1);
  return [...n].sort((a, b) => b[1] - a[1])[0]?.[0] ?? '';
}

/** "Morning", "Afternoon"… from the local hour of the moment's first file. */
export function partOfDay(t: number): string {
  const h = new Date(t).getHours();
  return h < 5 ? 'Night' : h < 12 ? 'Morning' : h < 17 ? 'Afternoon' : h < 21 ? 'Evening' : 'Night';
}

export interface DraftOptions {
  /** a pause longer than this starts a new section */
  gapMs?: number;
  /** photos per section */
  perMoment?: number;
  title?: string;
}

/**
 * A first draft of a story from the photos of a trip or a date range: a chapter per day
 * (when there is more than one), a section per moment with its best few photos, captions
 * with date, time and place. The text is left for the person.
 */
export function draftStory(files: FileItem[], opts: DraftOptions = {}): Pick<Doc, 'title' | 'sections' | 'tags' | 'date'> {
  const days = groupByDay(files);
  const per = opts.perMoment ?? 4;
  const sections: Section[] = [];
  days.forEach(({ day, files: dayFiles }, di) => {
    const dayPlace = commonPlace(dayFiles);
    if (days.length > 1)
      sections.push({
        id: newId(),
        title: `Day ${di + 1} · ${fmtDayLong(day, false)}`,
        layout: 'text-only',
        media: [],
        text: textDoc(),
        slide: true,
        chapter: true,
        date: day,
        ...(dayPlace ? { place: dayPlace } : {}),
      });
    for (const moment of splitMoments(dayFiles, opts.gapMs)) {
      const picks = pickBest(moment, per);
      const place = commonPlace(moment);
      const city = place.split(',')[0].trim();
      sections.push({
        id: newId(),
        title: city ? `${partOfDay(takenMs(moment[0]))} in ${city}` : partOfDay(takenMs(moment[0])),
        ...layoutFor(picks.length),
        media: picks.map((f) => toRef(f)),
        text: textDoc(),
        slide: true,
        date: day,
        ...(place ? { place } : {}),
      });
    }
  });
  const first = days[0]?.day ?? isoDay(Date.now());
  const last = days.at(-1)?.day ?? first;
  const where = commonPlace(files).split(',')[0].trim();
  return {
    title: opts.title?.trim() || [where, fmtRange(first, last)].filter(Boolean).join(' · '),
    date: first,
    tags: days.length > 1 ? ['trip'] : [],
    sections,
  };
}

/** Day → the stories about that day (by their own date or a dated section). */
export function storiesByDay(docs: DocMeta[]): Map<string, DocMeta[]> {
  const m = new Map<string, DocMeta[]>();
  for (const d of docs) for (const day of d.dates ?? (d.date ? [d.date] : [])) m.set(day, [...(m.get(day) ?? []), d]);
  return m;
}
