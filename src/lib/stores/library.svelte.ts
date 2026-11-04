/** Imported photos and the bitmaps the canvas renders them with. */
import { api, fileUrl, isTauri } from '../api/backend';
import type { ImportedImage } from '../api/types';
import type { AssetRef } from '../model/types';
import type { ImageEntry } from '../render/renderer';
import { toasts } from './toasts.svelte';

interface Loaded {
  img: HTMLImageElement;
  ready: boolean;
  failed: boolean;
}

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
      this.importing = null;
    }
  }

  /** Make sure every asset referenced by a document is available (e.g. after opening a project). */
  async ensureAssets(assets: Record<string, AssetRef>): Promise<string[]> {
    const missing = Object.entries(assets).filter(([id]) => !this.byId.has(id));
    if (!missing.length) return [];
    const imported = await this.import(missing.map(([, a]) => a.path));
    const ok = new Set(imported.map((i) => i.path));
    return missing.filter(([, a]) => !ok.has(a.path)).map(([id]) => id);
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
      l = { img, ready: false, failed: false };
      map.set(id, l);
      const rec = l;
      img.onload = () => {
        rec.ready = true;
        this.version++;
      };
      img.onerror = () => {
        rec.failed = true;
      };
      img.src = url;
    }
    return l;
  }

  /**
   * Bitmap for the editor canvas. Small on-screen sizes use the 384 px thumbnail,
   * larger ones the 2048 px proxy (falling back to whatever is loaded).
   */
  entry(id: string, neededPx = Infinity): ImageEntry | null {
    const info = this.byId.get(id);
    if (!info) return null;
    const thumb = this.load(this.thumbs, id, fileUrl(info.thumb));
    if (neededPx <= 360 && thumb.ready) return this.wrap(id, thumb, 'thumb');
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

  /** Resolves when the proxies of the given assets are decoded. */
  async preload(ids: string[]): Promise<void> {
    await Promise.all(
      ids.map(
        (id) =>
          new Promise<void>((resolve) => {
            if (this.entry(id, Infinity)?.key.endsWith('@proxy')) return resolve();
            const l = this.bitmaps.get(id);
            if (!l || l.failed) return resolve();
            const done = () => resolve();
            l.img.addEventListener('load', done, { once: true });
            l.img.addEventListener('error', done, { once: true });
          }),
      ),
    );
  }
}

export const library = new Library();
