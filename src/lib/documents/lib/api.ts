import type { Doc, DocMeta, DirNode, FileItem, HistoryEntry, Library, MediaRef, PrefItem, Prefs, ProxyState, Resolved, SearchHit, Usage } from './types';

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public body: any = null,
  ) {
    super(message);
  }
}

let onUnauthorized: (() => void) | null = null;
export function setUnauthorizedHandler(fn: () => void) {
  onUnauthorized = fn;
}

async function req<T>(method: string, url: string, body?: unknown, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(url, {
    method,
    headers: body !== undefined ? { 'Content-Type': 'application/json' } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: 'same-origin',
    ...opts,
  });
  if (res.status === 401 && !url.startsWith('/api/login')) {
    onUnauthorized?.();
  }
  const text = await res.text();
  let data: any = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = text;
    }
  }
  if (!res.ok) {
    throw new ApiError(res.status, (data && data.error) || res.statusText, data);
  }
  return data as T;
}

export const api = {
  session: () =>
    req<{ authenticated: boolean; authEnabled: boolean; version: string; https: boolean; httpsAddr: string; proto: string }>(
      'GET',
      '/api/session',
    ),
  login: (password: string) => req<{ ok: boolean }>('POST', '/api/login', { password }),
  logout: () => req('POST', '/api/logout', {}),
  status: () => req<any>('GET', '/api/status'),
  scan: () => req<any>('POST', '/api/scan', {}),

  libraries: () => req<Library[]>('GET', '/api/libraries'),
  tree: (lib: string) => req<DirNode>('GET', `/api/libraries/${encodeURIComponent(lib)}/tree`),
  files: (q: Record<string, string | number | boolean | undefined>, signal?: AbortSignal) => {
    const p = new URLSearchParams();
    for (const [k, v] of Object.entries(q)) if (v !== undefined && v !== '' && v !== false) p.set(k, String(v === true ? 1 : v));
    return req<{ total: number; offset: number; items: FileItem[] }>('GET', `/api/files?${p}`, undefined, { signal });
  },
  resolve: (refs: { libraryId: string; path: string }[]) => req<Resolved[]>('POST', '/api/resolve', { refs }),
  proxy: (key: string, variant: string, retry = false) => req<ProxyState>('POST', '/api/proxy', { key, variant, retry }),

  docs: () => req<DocMeta[]>('GET', '/api/documents'),
  createDoc: (title: string) => req<Doc>('POST', '/api/documents', { title }),
  importDoc: (doc: unknown) => req<Doc>('POST', '/api/documents/import', doc),
  stressTest: () => req<Doc>('POST', '/api/documents/stress-test', {}),
  getDoc: (id: string) => req<Doc>('GET', `/api/documents/${id}`),
  saveDoc: (id: string, baseVersion: number, document: Doc, force = false, keepalive = false) =>
    req<{ version: number; updatedAt: string }>('PUT', `/api/documents/${id}`, { baseVersion, document, force }, { keepalive }),
  deleteDoc: (id: string) => req('DELETE', `/api/documents/${id}`),
  duplicateDoc: (id: string) => req<Doc>('POST', `/api/documents/${id}/duplicate`, {}),
  snapshot: (id: string) => req('POST', `/api/documents/${id}/snapshot`, {}),
  history: (id: string) => req<HistoryEntry[]>('GET', `/api/documents/${id}/history`),
  version: (id: string, v: number) => req<Doc>('GET', `/api/documents/${id}/history/${v}`),
  restore: (id: string, v: number) => req<Doc>('POST', `/api/documents/${id}/history/${v}/restore`, {}),

  prefs: () => req<Prefs>('GET', '/api/prefs'),
  savePrefs: (p: { favorites?: PrefItem[]; hidden?: PrefItem[]; recent?: PrefItem[]; addRecent?: PrefItem[] }) => req<Prefs>('PUT', '/api/prefs', p),
  duplicates: (dist = 5) => req<{ groups: { kind: 'identical' | 'similar'; files: FileItem[] }[]; hashPending: number }>('GET', `/api/duplicates?dist=${dist}`),
  cameras: () => req<{ name: string; count: number }[]>('GET', '/api/cameras'),
  usage: (libraryId: string, path: string, dir = false) =>
    req<Usage[]>('GET', `/api/usage?library=${encodeURIComponent(libraryId)}&path=${encodeURIComponent(path)}${dir ? '&dir=1' : ''}`),
  search: (q: string) => req<SearchHit[]>('GET', `/api/search?q=${encodeURIComponent(q)}`),
  match: (items: { name?: string; size?: number; path?: string }[]) =>
    req<{ exact: boolean; candidates: FileItem[] }[]>('POST', '/api/match', { items }),
  prewarm: (refs: { libraryId: string; path: string }[]) => req('POST', '/api/prewarm', { refs }).catch(() => {}),
};

/** Sprite sheet layout for a video (mirrors media.SpriteLayout on the server). */
export function spriteLayout(f: { duration: number; width: number; height: number }) {
  const n = Math.max(10, Math.min(60, Math.floor(f.duration / 2)));
  let h = 90;
  if (f.width > 0 && f.height > 0) {
    h = Math.floor((160 * f.height) / f.width);
    h -= h % 2;
    h = Math.max(40, Math.min(320, h));
  }
  return { n, cols: 10, w: 160, h };
}

export function refKey(r: { libraryId: string; path: string }) {
  return r.libraryId + '\u0000' + r.path;
}

export function sameRef(a: MediaRef, b: { libraryId: string; path: string }) {
  return a.libraryId === b.libraryId && a.path === b.path;
}

export function thumbUrl(derived: string | undefined, size: 320 | 1280 | 2560) {
  return derived ? `${derived}t${size}.webp` : '';
}

/** srcset with real widths: never claims a variant is wider than the original. */
export function srcset(derived: string | undefined, width: number) {
  if (!derived) return '';
  const out: string[] = [];
  for (const s of [320, 1280, 2560] as const) {
    if (!width || s < width) out.push(`${derived}t${s}.webp ${s}w`);
    else {
      out.push(`${derived}t${s}.webp ${width}w`);
      break;
    }
  }
  return out.join(', ');
}

export function fmtDuration(sec: number) {
  if (!sec || !isFinite(sec)) return '';
  const s = Math.round(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

export function fmtBytes(n: number) {
  if (!n) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(u.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 1024 ** i).toFixed(i >= 2 ? 1 : 0)} ${u[i]}`;
}

export function fmtDate(s: string | number) {
  const d = new Date(s);
  return d.toLocaleString(undefined, { year: 'numeric', month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
}

export function newId() {
  return crypto.randomUUID();
}
