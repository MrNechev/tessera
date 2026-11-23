// Resolves document media references ({libraryId, path}) to file info + playable sources.
// Requests are batched per animation frame; entries whose proxies are being generated are
// re-polled until ready.
import { SvelteMap } from 'svelte/reactivity';
import { api, refKey } from './api';
import type { Resolved } from './types';

class Resolver {
  cache = new SvelteMap<string, Resolved>();
  private pending = new Map<string, { libraryId: string; path: string }>();
  private inflight = new Set<string>();
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pollTimer: ReturnType<typeof setInterval> | null = null;

  get(ref: { libraryId: string; path: string }): Resolved | undefined {
    const k = refKey(ref);
    const v = this.cache.get(k);
    if (!v && !this.inflight.has(k)) this.request(ref);
    return v;
  }

  request(ref: { libraryId: string; path: string }) {
    const k = refKey(ref);
    if (this.inflight.has(k)) return;
    this.pending.set(k, { libraryId: ref.libraryId, path: ref.path });
    if (!this.timer) this.timer = setTimeout(() => this.flush(), 16);
  }

  /** Forget and re-fetch (after a proxy request, relink, rescan...). */
  refresh(ref: { libraryId: string; path: string }) {
    this.inflight.delete(refKey(ref));
    this.request(ref);
  }

  prefetch(refs: { libraryId: string; path: string }[]) {
    for (const r of refs) if (!this.cache.has(refKey(r))) this.request(r);
  }

  private async flush() {
    this.timer = null;
    const batch = [...this.pending.values()];
    this.pending.clear();
    for (let i = 0; i < batch.length; i += 400) {
      const chunk = batch.slice(i, i + 400);
      chunk.forEach((r) => this.inflight.add(refKey(r)));
      try {
        const res = await api.resolve(chunk);
        for (const r of res) this.cache.set(refKey(r), r);
      } catch {
        // retry on next access
      } finally {
        chunk.forEach((r) => this.inflight.delete(refKey(r)));
      }
    }
    this.ensurePolling();
  }

  private busy(r: Resolved) {
    const s = (x?: { proxy?: { status: string } }) => x?.proxy && (x.proxy.status === 'queued' || x.proxy.status === 'running');
    return s(r.doc) || s(r.present);
  }

  private ensurePolling() {
    if (this.pollTimer) return;
    this.pollTimer = setInterval(() => {
      const busy = [...this.cache.values()].filter((r) => this.busy(r));
      if (!busy.length) {
        clearInterval(this.pollTimer!);
        this.pollTimer = null;
        return;
      }
      busy.forEach((r) => this.refresh(r));
    }, 2500);
  }

  /** Mark an entry as busy locally so polling starts right away. */
  markQueued(ref: { libraryId: string; path: string }) {
    this.refresh(ref);
    setTimeout(() => this.ensurePolling(), 100);
  }

  clear() {
    this.cache.clear();
  }
}

export const resolver = new Resolver();
