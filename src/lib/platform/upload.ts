/**
 * Browser → server file transfer (web mode). Each dropped/picked file is
 * first matched against the server's libraries (same name and size: no upload
 * needed); the rest are uploaded in resumable 8 MB chunks.
 */
import { call } from '../api/http';

export type UploadDest = 'temp' | 'inbox';

const CHUNK = 8 * 1024 * 1024;

export interface UploadProgress {
  file: string;
  sent: number;
  total: number;
  index: number;
  count: number;
}

interface MatchResult {
  exact: boolean;
  candidates: { libraryId: string; path: string; size: number }[];
}

/** Server paths of files already in a library, by index into `files`. */
async function matchExisting(files: File[]): Promise<Map<number, string>> {
  const out = new Map<number, string>();
  try {
    const res = await fetch('/api/match', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ items: files.map((f) => ({ name: f.name, size: f.size })) }),
      credentials: 'same-origin',
    });
    if (!res.ok) return out;
    const matches: MatchResult[] = await res.json();
    await Promise.all(
      matches.map(async (m, i) => {
        const c = m.exact ? m.candidates.find((c) => c.size === files[i].size) : undefined;
        if (!c) return;
        try {
          out.set(i, await call<string>('resolve', { libraryId: c.libraryId, path: c.path }));
        } catch {
          /* not resolvable: upload instead */
        }
      }),
    );
  } catch {
    /* matching is an optimisation only */
  }
  return out;
}

function batchId(): string {
  const a = new Uint8Array(8);
  crypto.getRandomValues(a);
  return Array.from(a, (b) => b.toString(16).padStart(2, '0')).join('');
}

async function uploadOne(file: File, batch: string, dest: UploadDest, onChunk: (sent: number) => void): Promise<string> {
  let offset = 0;
  let attempts = 0;
  for (;;) {
    const end = Math.min(file.size, offset + CHUNK);
    const done = end >= file.size;
    const q = new URLSearchParams({ name: file.name, batch, dest, offset: String(offset), done: String(done) });
    let res: Response;
    try {
      res = await fetch(`/api/t/upload?${q}`, {
        method: 'POST',
        headers: { 'content-type': 'application/octet-stream' },
        body: file.slice(offset, end),
        credentials: 'same-origin',
      });
    } catch (e) {
      if (++attempts > 5) throw e;
      await new Promise((r) => setTimeout(r, 1000 * attempts));
      continue;
    }
    if (!res.ok) {
      let msg = `Upload failed (${res.status})`;
      try {
        msg = (await res.json()).error ?? msg;
      } catch {
        /* ignore */
      }
      if (res.status >= 500 && ++attempts <= 5) {
        await new Promise((r) => setTimeout(r, 1000 * attempts));
        continue;
      }
      throw new Error(msg);
    }
    const r: { path: string; received: number } = await res.json();
    offset = r.received;
    onChunk(offset);
    if (r.path) return r.path;
    if (done && offset >= file.size) throw new Error('Upload did not complete');
  }
}

/**
 * Make `files` available on the server and return their server paths, in
 * order. Files already in a library are used in place.
 */
export async function sendFiles(
  files: File[],
  dest: UploadDest = 'temp',
  onProgress?: (p: UploadProgress) => void,
): Promise<string[]> {
  const existing = await matchExisting(files);
  const batch = batchId();
  const out: string[] = [];
  for (let i = 0; i < files.length; i++) {
    const f = files[i];
    const known = existing.get(i);
    if (known && dest === 'temp') {
      out.push(known);
      continue;
    }
    out.push(
      await uploadOne(f, batch, dest, (sent) => onProgress?.({ file: f.name, sent, total: f.size, index: i, count: files.length })),
    );
  }
  return out;
}

/** Let the user choose files from this computer (browser file dialog). */
export function chooseLocalFiles(accept: string[], multiple = true): Promise<File[]> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = multiple;
    if (accept.length) input.accept = accept.map((e) => `.${e}`).join(',');
    input.style.display = 'none';
    input.addEventListener('change', () => {
      resolve(Array.from(input.files ?? []));
      input.remove();
    });
    input.addEventListener('cancel', () => {
      resolve([]);
      input.remove();
    });
    document.body.appendChild(input);
    input.click();
  });
}
