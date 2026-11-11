/** Imported photos and the bitmaps the canvas renders them with. */
import { api, fileUrl, isTauri, onProgress } from '../api/backend';
import type { ImportedImage } from '../api/types';
import type { AssetRef } from '../model/types';
import type { ImageEntry } from '../render/renderer';
import { toasts } from './toasts.svelte';

interface Loaded {
  img: HTMLImageElement;
  ready: boolean;
  failed: boolean;
  /** performance.now() of the last time a render asked for it. */
  used: number;
}

/**
 * Decoded 2048 px proxies cost ~11 MB each. Keep at most this many, dropping
 * the least recently drawn ones (never anything drawn in the last few seconds,
 * so a page with more large photos than this doesn't thrash).
 */
const MAX_PROXIES = 64;
const PROXY_GRACE_MS = 5000;

class Library {
  items = $state<ImportedImage[]>([]);
  importing = $state<{ done: number; total: number } | null>(null);
  /** Bumped whenever a bitmap finishes loading (canvas redraw trigger). */
  version = $state(0);

  private byId = new Map<string, ImportedImage>();
  private bitmaps = new Map<string, Loaded>();
  private thumbs = new Map<string, Loaded>();

  get(id: string): ImportedImage | undefined {
    return this.byId.get(id);
  }

  has(id: string): boolean {
    return this.byId.has(id);
  }

  private add(images: ImportedImage[]): ImportedImage[] {
    const fresh: ImportedImage[] = [];
    for (const img of images) {
      if (!this.byId.has(img.id)) {
        this.byId.set(img.id, img);
        fresh.push(img);
      }
    }
    if (fresh.length) this.items = [...this.items, ...fresh];
    return images;
  }

  /** Import files and/or folders. Returns every successfully imported image (new or existing). */
  async import(paths: string[]): Promise<ImportedImage[]> {
    if (!paths.length || !isTauri) return [];
    this.importing = { done: 0, total: paths.length };
    // Folders expand to many files in the backend; it reports the real count.
    const unlisten = await onProgress((e) => {
      if (e.task === 'import' && this.importing) this.importing = { done: e.done, total: e.total };
    }).catch(() => null);
    try {
      const res = await api.importImages(paths);
      if (res.failed.length) {
        const names = res.failed.slice(0, 3).map((f) => f.path.split('/').pop()).join(', ');
        toasts.push({
          kind: 'warning',
          title: `${res.failed.length} file${res.failed.length > 1 ? 's' : ''} could not be imported`,
          message: `${names}${res.failed.length > 3 ? '…' : ''} — ${res.failed[0].error}`,
        });
      }
      return this.add(res.images);
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Import failed', message: String(e) });
      return [];
    } finally {
      unlisten?.();
      this.importing = null;
    }
  }

  /**
   * Make sure every asset referenced by a document is available (e.g. after
   * opening a project). A photo that was edited since gets a new id when it
   * is imported again; it is registered under the document's old id too, and
   * the document's asset entry is updated in place to the new size. Returns
   * the ids whose files could not be imported.
   */
  async ensureAssets(assets: Record<string, AssetRef>): Promise<string[]> {
    const missing = Object.entries(assets).filter(([id]) => !this.byId.has(id));
    if (!missing.length) return [];
    const imported = await this.import(missing.map(([, a]) => a.path));
    const byPath = new Map(imported.map((i) => [i.path, i]));
    const lost: string[] = [];
    for (const [id, a] of missing) {
      const img = byPath.get(a.path);
      if (!img) {
        lost.push(id);
        continue;
      }
      if (img.id !== id) {
        this.byId.set(id, { ...img, id });
        assets[id] = { ...a, name: img.name, width: img.width, height: img.height };
      }
    }
    return lost;
  }

  /** Recreate proxies and thumbnails after the cache folder was emptied. */
  async rebuild(): Promise<void> {
    const known = [...this.byId];
    this.thumbs.clear();
    this.bitmaps.clear();
    if (!known.length) return;
    const fresh = await this.import([...new Set(known.map(([, i]) => i.path))]);
    const byPath = new Map(fresh.map((i) => [i.path, i]));
    for (const [id, old] of known) {
      const img = byPath.get(old.path);
      if (img) this.byId.set(id, { ...img, id });
    }
    this.items = this.items.map((i) => this.byId.get(i.id) ?? i);
    this.version++;
  }

  remove(id: string): void {
    this.byId.delete(id);
    this.bitmaps.delete(id);
    this.thumbs.delete(id);
    this.items = this.items.filter((i) => i.id !== id);
  }

  clear(): void {
    this.byId.clear();
    this.bitmaps.clear();
    this.thumbs.clear();
    this.items = [];
  }

  assetRef(id: string): AssetRef | null {
    const i = this.byId.get(id);
    return i ? { path: i.path, name: i.name, width: i.width, height: i.height } : null;
  }

  private load(map: Map<string, Loaded>, id: string, url: string): Loaded {
    let l = map.get(id);
    if (!l) {
      const img = new Image();
      img.crossOrigin = 'anonymous';
      img.decoding = 'async';
      l = { img, ready: false, failed: false, used: performance.now() };
      map.set(id, l);
      const rec = l;
      img.onload = () => {
        rec.ready = true;
        this.version++;
      };
      img.onerror = () => {
        rec.failed = true;
        // Let a later render retry (e.g. after the cache was rebuilt).
        if (map.get(id) === rec) setTimeout(() => map.get(id) === rec && map.delete(id), 3000);
      };
      img.src = url;
      if (map === this.bitmaps) this.evictProxies();
    } else {
      l.used = performance.now();
    }
    return l;
  }

  private evictProxies(): void {
    if (this.bitmaps.size <= MAX_PROXIES) return;
    const cutoff = performance.now() - PROXY_GRACE_MS;
    const old = [...this.bitmaps].filter(([, l]) => l.used < cutoff).sort((a, b) => a[1].used - b[1].used);
    for (const [id] of old.slice(0, this.bitmaps.size - MAX_PROXIES)) this.bitmaps.delete(id);
  }

  /**
   * Bitmap for the editor canvas. Small on-screen sizes use the 384 px thumbnail
   * (or the proxy if that happens to be decoded already), larger ones the
   * 2048 px proxy (falling back to the thumbnail while it loads).
   */
  entry(id: string, neededPx = Infinity): ImageEntry | null {
    const info = this.byId.get(id);
    if (!info) return null;
    const thumb = this.load(this.thumbs, id, fileUrl(info.thumb));
    if (neededPx <= 360) {
      if (thumb.ready) return this.wrap(id, thumb, 'thumb');
      const p = this.bitmaps.get(id);
      return p?.ready ? this.wrap(id, p, 'proxy') : null;
    }
    const proxy = this.entryProxy(id, info);
    if (proxy) return proxy;
    return thumb.ready ? this.wrap(id, thumb, 'thumb') : null;
  }

  private wrap(id: string, l: Loaded, kind: string): ImageEntry {
    return { source: l.img, width: l.img.naturalWidth, height: l.img.naturalHeight, key: `${id}@${kind}` };
  }

  private entryProxy(id: string, info: ImportedImage): ImageEntry | null {
    const l = this.load(this.bitmaps, id, fileUrl(info.proxy));
    return l.ready ? this.wrap(id, l, 'proxy') : null;
  }

  /** Resolves when the bitmaps `entry(id, neededPx)` would use are decoded (or failed). */
  async preload(ids: string[], neededPx = Infinity): Promise<void> {
    await Promise.all(
      ids.map(
        (id) =>
          new Promise<void>((resolve) => {
            const info = this.byId.get(id);
            if (!info) return resolve();
            const l =
              neededPx <= 360
                ? this.load(this.thumbs, id, fileUrl(info.thumb))
                : this.load(this.bitmaps, id, fileUrl(info.proxy));
            if (l.ready || l.failed) return resolve();
            l.img.addEventListener('load', () => resolve(), { once: true });
            l.img.addEventListener('error', () => resolve(), { once: true });
          }),
      ),
    );
  }
}

export const library = new Library();
