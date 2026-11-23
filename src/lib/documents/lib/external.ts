// Media coming from outside the app: files dropped from the computer, or library paths /
// links pasted as text. Tessera never uploads here; it finds the same file in a library folder.
import { api } from './api';
import type { MediaRef } from './types';

export function hasFiles(e: DragEvent) {
  return !!e.dataTransfer && [...e.dataTransfer.types].includes('Files');
}

export interface ExternalResult {
  refs: MediaRef[];
  missing: string[];
  ambiguous: string[];
}

/** Match files dropped from the OS by name + size. */
export async function matchFiles(files: FileList | File[]): Promise<ExternalResult> {
  const list = [...files].slice(0, 200);
  const res = await api.match(list.map((f) => ({ name: f.name, size: f.size })));
  return collect(res, list.map((f) => f.name));
}

/** Match files on this computer by their full path (desktop app drops). */
export async function matchPaths(paths: string[]): Promise<ExternalResult> {
  const list = paths.slice(0, 200);
  const res = await api.match(list.map((path) => ({ path })));
  return collect(res, list.map((p) => p.split('/').pop() ?? p));
}

/**
 * Web app: upload files that aren't in any library to the server's Inbox and
 * return references to them (the Inbox is a library too).
 */
export async function uploadToInbox(files: File[]): Promise<MediaRef[]> {
  const { call } = await import('../../api/http');
  const { sendFiles } = await import('../../platform/upload');
  const areas = await call<{ id: string; root: string; kind: string }[]>('areas');
  const inbox = areas.find((a) => a.kind === 'inbox');
  if (!inbox) throw new Error('This server has no Inbox folder');
  const paths = await sendFiles(files, 'inbox');
  const root = inbox.root.replace(/\/$/, '') + '/';
  return paths.filter((p) => p.startsWith(root)).map((p) => ({ libraryId: inbox.id, path: p.slice(root.length) }));
}

/** Match pasted text: one path or /media/… link per line. */
export async function matchText(text: string): Promise<ExternalResult | null> {
  const lines = text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && l.length < 2000 && /[./\\]/.test(l) && /\.[a-z0-9]{2,5}(\?.*)?$/i.test(l));
  if (!lines.length) return null;
  const res = await api.match(lines.slice(0, 200).map((path) => ({ path })));
  return collect(res, lines);
}

function collect(res: { exact: boolean; candidates: { libraryId: string; path: string }[] }[], names: string[]): ExternalResult {
  const out: ExternalResult = { refs: [], missing: [], ambiguous: [] };
  res.forEach((r, i) => {
    if (r.candidates.length && (r.exact || r.candidates.length === 1)) out.refs.push({ libraryId: r.candidates[0].libraryId, path: r.candidates[0].path });
    else if (r.candidates.length) {
      out.refs.push({ libraryId: r.candidates[0].libraryId, path: r.candidates[0].path });
      out.ambiguous.push(names[i]);
    } else out.missing.push(names[i]);
  });
  return out;
}

export function describe(r: ExternalResult) {
  const parts: string[] = [];
  if (r.missing.length)
    parts.push(
      `${r.missing.length === 1 ? `"${r.missing[0]}" is` : `${r.missing.length} files are`} not in a library folder yet — add ${r.missing.length === 1 ? 'it' : 'them'} to one of your libraries first`,
    );
  if (r.ambiguous.length) parts.push(`${r.ambiguous.length} matched by name only (picked the closest size)`);
  return parts.join('. ');
}
