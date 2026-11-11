/**
 * Batch rename: turns a pattern like "{date} Holiday {n:3}" into file names.
 * The preview and the backend use the names computed here, so what you see
 * in the table is exactly what gets written.
 */

export interface RenameInput {
  path: string;
  takenAt: string | null;
  width: number;
  height: number;
}

export type RenameSort = 'added' | 'name' | 'date';
export type RenameCase = 'keep' | 'lower' | 'upper';

export interface RenameSpec {
  pattern: string;
  /** First value of {n}. */
  start: number;
  /** Minimum digits of {n} (0 = no padding); {n:3} overrides per token. */
  padding: number;
  sort: RenameSort;
  letterCase: RenameCase;
  /** Replace spaces with this string (null = keep spaces). */
  spaces: string | null;
}

export const DEFAULT_RENAME: RenameSpec = {
  pattern: '{name}',
  start: 1,
  padding: 3,
  sort: 'added',
  letterCase: 'keep',
  spaces: null,
};

export const RENAME_TOKENS: { token: string; help: string }[] = [
  { token: '{name}', help: 'Original file name' },
  { token: '{n}', help: 'Counter (use {n:4} for 4 digits)' },
  { token: '{date}', help: 'Date taken, 2026-08-21' },
  { token: '{time}', help: 'Time taken, 17-58-22' },
  { token: '{year}', help: 'Year taken' },
  { token: '{month}', help: 'Month taken, 01–12' },
  { token: '{day}', help: 'Day taken, 01–31' },
  { token: '{size}', help: 'Pixel size, 4000x3000' },
  { token: '{folder}', help: 'Name of the folder it’s in' },
];

export function baseName(path: string): string {
  const file = path.split('/').pop() ?? path;
  const dot = file.lastIndexOf('.');
  return dot > 0 ? file.slice(0, dot) : file;
}

function folderName(path: string): string {
  const parts = path.split('/');
  return parts.length > 1 ? (parts[parts.length - 2] ?? '') : '';
}

/** Characters not allowed in file names on Linux, Windows or macOS. */
export function sanitize(name: string): string {
  return name
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, '-')
    .replace(/\s+/g, ' ')
    .replace(/^[\s.]+|[\s.]+$/g, '');
}

export function sortFiles<T extends RenameInput>(files: T[], sort: RenameSort): T[] {
  const out = [...files];
  if (sort === 'name') out.sort((a, b) => baseName(a.path).localeCompare(baseName(b.path), undefined, { numeric: true }));
  if (sort === 'date')
    out.sort((a, b) => {
      // Photos without a capture date go last, in their added order.
      if (a.takenAt && b.takenAt) return a.takenAt.localeCompare(b.takenAt);
      return a.takenAt ? -1 : b.takenAt ? 1 : 0;
    });
  return out;
}

/** New base names (without extension) for `files`, in the given order. Unique within the batch. */
export function buildNames(files: RenameInput[], spec: RenameSpec): string[] {
  const used = new Map<string, number>();
  return files.map((f, i) => {
    const n = spec.start + i;
    const [date, time] = (f.takenAt ?? '').split('T');
    const [year = '', month = '', day = ''] = (date ?? '').split('-');
    let name = spec.pattern.replace(/\{(\w+)(?::(\d+))?\}/g, (whole, key: string, digits?: string) => {
      switch (key) {
        case 'name':
          return baseName(f.path);
        case 'n':
          return String(n).padStart(digits ? parseInt(digits) : spec.padding, '0');
        case 'date':
          return date || 'no-date';
        case 'time':
          return time ? time.replace(/:/g, '-') : 'no-time';
        case 'year':
          return year || 'no-date';
        case 'month':
          return month || '00';
        case 'day':
          return day || '00';
        case 'size':
          return `${f.width}x${f.height}`;
        case 'folder':
          return folderName(f.path);
        default:
          return whole;
      }
    });
    if (spec.letterCase === 'lower') name = name.toLowerCase();
    if (spec.letterCase === 'upper') name = name.toUpperCase();
    if (spec.spaces !== null) name = name.replace(/ /g, spec.spaces);
    name = sanitize(name) || `image-${n}`;
    // Two files mapping to the same name get " (2)", " (3)"…
    const key = name.toLowerCase();
    const count = (used.get(key) ?? 0) + 1;
    used.set(key, count);
    return count > 1 ? `${name} (${count})` : name;
  });
}
