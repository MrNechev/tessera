/**
 * Transport to Tessera's engine. The same HTTP API serves the desktop window
 * (a private server on 127.0.0.1) and browsers (the Docker server).
 */

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

async function failure(res: Response): Promise<ApiError> {
  let msg = res.statusText || `HTTP ${res.status}`;
  try {
    const body = await res.json();
    if (body && typeof body.error === 'string') msg = body.error;
  } catch {
    /* not JSON */
  }
  if (res.status === 401) window.dispatchEvent(new CustomEvent('tessera:unauthorized'));
  return new ApiError(msg, res.status);
}

/** Call a tool command (`POST /api/t/<cmd>`) with JSON arguments. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const res = await fetch(`/api/t/${cmd}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args ?? {}),
    credentials: 'same-origin',
  });
  if (!res.ok) throw await failure(res);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

/** POST raw bytes; JSON result. */
export async function callBinary<T>(path: string, body: Uint8Array, headers: Record<string, string> = {}): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/octet-stream', ...headers },
    body: body as BodyInit,
    credentials: 'same-origin',
  });
  if (!res.ok) throw await failure(res);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

/** POST JSON; raw bytes back. */
export async function fetchBytes(path: string, args: Record<string, unknown>): Promise<ArrayBuffer> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(args),
    credentials: 'same-origin',
  });
  if (!res.ok) throw await failure(res);
  return res.arrayBuffer();
}

type Listener = (msg: { type: string; data: unknown }) => void;
let source: EventSource | null = null;
const listeners = new Set<Listener>();

/** Subscribe to server-sent events (`/api/events`). */
export function subscribe(fn: Listener): () => void {
  listeners.add(fn);
  if (!source) {
    source = new EventSource('/api/events', { withCredentials: true });
    source.onmessage = (e) => {
      try {
        const msg = JSON.parse(e.data);
        for (const l of listeners) l(msg);
      } catch {
        /* ignore malformed */
      }
    };
  }
  return () => {
    listeners.delete(fn);
    if (!listeners.size && source) {
      source.close();
      source = null;
    }
  };
}
