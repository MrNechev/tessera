import { describe, expect, it } from 'vitest';
import { DEFAULT_RENAME, buildNames, sanitize, sortFiles, type RenameInput } from './rename';

const files: RenameInput[] = [
  { path: '/photos/Trip/IMG_0012.JPG', takenAt: '2026-08-21T17:58:22', width: 4000, height: 3000 },
  { path: '/photos/Trip/IMG_0003.JPG', takenAt: '2026-08-20T09:01:05', width: 3000, height: 4000 },
  { path: '/photos/Trip/scan.png', takenAt: null, width: 800, height: 600 },
];

describe('batch rename', () => {
  it('keeps names with the default pattern', () => {
    expect(buildNames(files, DEFAULT_RENAME)).toEqual(['IMG_0012', 'IMG_0003', 'scan']);
  });

  it('expands counters, dates and sizes', () => {
    const names = buildNames(files, { ...DEFAULT_RENAME, pattern: '{date} {folder} {n} {size}', start: 9, padding: 2 });
    expect(names).toEqual(['2026-08-21 Trip 09 4000x3000', '2026-08-20 Trip 10 3000x4000', 'no-date Trip 11 800x600']);
    expect(buildNames(files.slice(0, 1), { ...DEFAULT_RENAME, pattern: 'x{n:4}' })).toEqual(['x0001']);
    expect(buildNames(files.slice(0, 1), { ...DEFAULT_RENAME, pattern: '{year}{month}{day}_{time}' })).toEqual(['20260821_17-58-22']);
  });

  it('applies case and space options', () => {
    const names = buildNames(files.slice(0, 1), { ...DEFAULT_RENAME, pattern: 'Summer Trip {n}', letterCase: 'lower', spaces: '_' });
    expect(names).toEqual(['summer_trip_001']);
  });

  it('makes names unique and safe', () => {
    expect(buildNames(files, { ...DEFAULT_RENAME, pattern: 'Holiday' })).toEqual(['Holiday', 'Holiday (2)', 'Holiday (3)']);
    expect(sanitize(' a/b:c*? .')).toBe('a-b-c--');
    expect(buildNames(files.slice(0, 1), { ...DEFAULT_RENAME, pattern: '...' })).toEqual(['image-1']);
  });

  it('sorts by date with undated files last, and by name naturally', () => {
    expect(sortFiles(files, 'date').map((f) => f.path.split('/').pop())).toEqual(['IMG_0003.JPG', 'IMG_0012.JPG', 'scan.png']);
    expect(sortFiles(files, 'name').map((f) => f.path.split('/').pop())).toEqual(['IMG_0003.JPG', 'IMG_0012.JPG', 'scan.png']);
  });
});
