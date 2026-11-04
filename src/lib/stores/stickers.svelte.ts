/** Sticker catalogue (public/stickers/manifest.json) and bitmap cache. */

export interface StickerInfo {
  id: string;
  name: string;
  category: string;
  file: string;
  width: number;
  height: number;
  tags: string[];
}

export interface StickerCategory {
  id: string;
  name: string;
}

interface Manifest {
  version: number;
  categories: StickerCategory[];
  stickers: StickerInfo[];
}

class Stickers {
  categories = $state<StickerCategory[]>([]);
  list = $state<StickerInfo[]>([]);
  loaded = $state(false);
  version = $state(0);
  private byId = new Map<string, StickerInfo>();
  private images = new Map<string, { img: HTMLImageElement; ready: boolean }>();
  private loading: Promise<void> | null = null;

  load(): Promise<void> {
    if (!this.loading) {
      this.loading = fetch('/stickers/manifest.json')
        .then((r) => (r.ok ? r.json() : { version: 1, categories: [], stickers: [] }))
        .then((m: Manifest) => {
          this.categories = m.categories;
          this.list = m.stickers;
          this.byId = new Map(m.stickers.map((s) => [s.id, s]));
          this.loaded = true;
        })
        .catch(() => {
          this.loaded = true;
        });
    }
    return this.loading;
  }

  info(id: string): StickerInfo | undefined {
    return this.byId.get(id);
  }

  url(id: string): string {
    const info = this.byId.get(id);
    return info ? `/stickers/${info.file}` : '';
  }

  /** Bitmap for the renderer (null until loaded). */
  image(id: string): HTMLImageElement | null {
    let e = this.images.get(id);
    if (!e) {
      const info = this.byId.get(id);
      if (!info) return null;
      const img = new Image();
      e = { img, ready: false };
      this.images.set(id, e);
      const rec = e;
      img.onload = () => {
        rec.ready = true;
        this.version++;
      };
      img.src = `/stickers/${info.file}`;
    }
    return e.ready ? e.img : null;
  }

  async preload(ids: string[]): Promise<void> {
    await this.load();
    await Promise.all(
      ids.map(
        (id) =>
          new Promise<void>((resolve) => {
            if (this.image(id)) return resolve();
            const e = this.images.get(id);
            if (!e) return resolve();
            e.img.addEventListener('load', () => resolve(), { once: true });
            e.img.addEventListener('error', () => resolve(), { once: true });
          }),
      ),
    );
  }

  aspect(id: string): number {
    const i = this.byId.get(id);
    return i && i.height > 0 ? i.width / i.height : 1;
  }
}

export const stickers = new Stickers();
