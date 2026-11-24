import { describe, expect, it } from 'vitest';
import {
  addDays,
  addToDiary,
  dayBounds,
  dayInsertAt,
  draftStory,
  findDiary,
  groupByDay,
  hamming,
  isoDay,
  monthGrid,
  pickBest,
  splitMoments,
  storiesByDay,
} from './diary';
import type { DocMeta, FileItem, Section } from './types';

// Local times, so the tests pass in any time zone.
const at = (d: number, h: number, m = 0, s = 0) => new Date(2026, 10, d, h, m, s).getTime();
let seq = 0;
function file(taken: number, o: Partial<FileItem> = {}): FileItem {
  seq++;
  return {
    id: seq, libraryId: 'm', path: `p${seq}.jpg`, dir: '', name: `p${seq}.jpg`, ext: 'jpg', kind: 'image', size: 1, mtime: 0, key: `k${seq}`,
    probed: true, width: 4000, height: 3000, duration: 0, playable: true, taken, ...o,
  };
}
const meta = (o: Partial<DocMeta>): DocMeta => ({ id: 'x', title: '', createdAt: '', updatedAt: '', version: 1, sections: 0, media: 0, tags: [], ...o });

describe('calendar days', () => {
  it('formats and steps local days', () => {
    expect(isoDay(at(24, 23, 59))).toBe('2026-11-24');
    expect(addDays('2026-11-30', 1)).toBe('2026-12-01');
    expect(addDays('2026-03-01', -1)).toBe('2026-02-28');
    const b = dayBounds('2026-11-24');
    expect(b.from).toBe(at(24, 0));
    expect(b.to).toBe(at(25, 0));
    expect(dayBounds('2026-11-01', '2026-11-30').to).toBe(new Date(2026, 11, 1).getTime());
  });

  it('builds a Monday-first month grid', () => {
    const g = monthGrid(2026, 10); // November 2026 starts on a Sunday
    expect(g[0][0]).toBe('2026-10-26');
    expect(g[0][6]).toBe('2026-11-01');
    expect(g.at(-1)!.at(-1)).toBe('2026-12-06');
    expect(g.every((w) => w.length === 7)).toBe(true);
    // Sunday-first: the 1st opens the grid
    expect(monthGrid(2026, 10, 0)[0][0]).toBe('2026-11-01');
  });

  it('maps stories to their days', () => {
    const m = storiesByDay([meta({ id: 'a', dates: ['2026-11-24', '2026-11-25'] }), meta({ id: 'b', date: '2026-11-24' })]);
    expect(m.get('2026-11-24')!.map((d) => d.id)).toEqual(['a', 'b']);
    expect(m.get('2026-11-25')!.length).toBe(1);
  });
});

describe('the diary', () => {
  it('is the most recently edited story tagged diary', () => {
    const docs = [
      meta({ id: 'old', tags: ['Diary'], updatedAt: '2025-01-01T00:00:00Z' }),
      meta({ id: 'trip', tags: ['trip'], updatedAt: '2026-11-24T00:00:00Z' }),
      meta({ id: 'new', tags: ['diary'], updatedAt: '2026-11-01T00:00:00Z' }),
    ];
    expect(findDiary(docs)?.id).toBe('new');
    expect(findDiary([docs[1]])).toBeUndefined();
  });

  it('adds days in date order and reuses an existing day', () => {
    const doc = { sections: [] as Section[] };
    const first = addToDiary(doc, '2026-11-24', [file(at(24, 9))]);
    expect(doc.sections.length).toBe(2);
    expect(doc.sections[0]).toMatchObject({ id: first, chapter: true, date: '2026-11-24' });
    expect(doc.sections[1].media.length).toBe(1);
    expect(doc.sections[1].media[0].caption).toContain(':');
    // an earlier day goes in front
    addToDiary(doc, '2026-11-20', []);
    expect(doc.sections.map((s) => s.date)).toEqual(['2026-11-20', '2026-11-20', '2026-11-24', '2026-11-24']);
    expect(dayInsertAt(doc, '2026-11-22')).toBe(2);
    // opening the day again: same chapter, only new photos are added
    const again = addToDiary(doc, '2026-11-24', [file(at(24, 10)), { ...doc.sections[3].media[0], ...file(0), path: doc.sections[3].media[0].path }]);
    expect(again).toBe(first);
    expect(doc.sections.length).toBe(4);
    expect(doc.sections[3].media.length).toBe(2);
    expect(doc.sections[3].layout).toBe('gallery');
  });
});

describe('drafting a story', () => {
  it('groups by day and splits moments by gaps and places', () => {
    const files = [file(at(4, 9)), file(at(3, 18)), file(at(3, 9)), file(at(3, 9, 30)), file(at(3, 10, 0), { place: 'Plovdiv, Bulgaria' }), file(at(3, 10, 5), { place: 'Sofia, Bulgaria' })];
    const days = groupByDay(files);
    expect(days.map((d) => [d.day, d.files.length])).toEqual([
      ['2026-11-03', 5],
      ['2026-11-04', 1],
    ]);
    const moments = splitMoments(days[0].files, 2 * 3_600_000);
    // 9:00, 9:30, 10:00 Plovdiv | 10:05 Sofia (moved) | 18:00 (gap)
    expect(moments.map((m) => m.length)).toEqual([3, 1, 1]);
  });

  it('skips near-duplicates and spreads the picks over time', () => {
    expect(hamming('ff00ff00ff00ff00', 'ff00ff00ff00ff01')).toBe(1);
    expect(hamming('ffffffffffffffff', '0000000000000000')).toBe(64);
    expect(hamming(undefined, 'ff')).toBe(Infinity);
    // a burst of three (similar hashes) keeps only its largest frame
    const burst = [file(at(3, 9, 0, 0), { phash: 'aaaaaaaaaaaaaaaa' }), file(at(3, 9, 0, 1), { phash: 'aaaaaaaaaaaaaaab', width: 6000 }), file(at(3, 9, 0, 2), { phash: 'aaaaaaaaaaaaaaa8' })];
    const other = file(at(3, 9, 5), { phash: '5555555555555555' });
    const best = pickBest([...burst, other], 5);
    expect(best.map((f) => f.id)).toEqual([burst[1].id, other.id]);
    // without hashes: shots seconds apart are a burst
    const noHash = [file(at(3, 12, 0, 0)), file(at(3, 12, 0, 2)), file(at(3, 12, 10))];
    expect(pickBest(noHash, 5).length).toBe(2);
    // evenly spaced picks keep the first and the last
    const many = Array.from({ length: 10 }, (_, i) => file(at(3, 13, i)));
    const picks = pickBest(many, 3);
    expect(picks.map((f) => f.id)).toEqual([many[0].id, many[5].id, many[9].id]);
  });

  it('drafts chapters per day, sections per moment, captions and dates', () => {
    const files = [
      file(at(3, 9), { place: 'Plovdiv, Bulgaria' }),
      file(at(3, 9, 20), { place: 'Plovdiv, Bulgaria' }),
      file(at(3, 15), { place: 'Plovdiv, Bulgaria' }),
      file(at(4, 11), { place: 'Sofia, Bulgaria' }),
    ];
    const d = draftStory(files, { perMoment: 4 });
    expect(d.date).toBe('2026-11-03');
    expect(d.title).toContain('Plovdiv');
    expect(d.tags).toEqual(['trip']);
    const kinds = d.sections.map((s) => (s.chapter ? `#${s.date}` : `${s.title}:${s.media.length}`));
    expect(kinds).toEqual(['#2026-11-03', 'Morning in Plovdiv:2', 'Afternoon in Plovdiv:1', '#2026-11-04', 'Morning in Sofia:1']);
    expect(d.sections[1].layout).toBe('gallery');
    expect(d.sections[1].place).toBe('Plovdiv, Bulgaria');
    expect(d.sections[1].media[0].caption).toContain('Plovdiv, Bulgaria');
    // one day: no chapters
    expect(draftStory(files.slice(0, 2)).sections.every((s) => !s.chapter)).toBe(true);
  });
});
