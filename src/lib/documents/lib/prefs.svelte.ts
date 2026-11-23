// Library preferences shared by every computer (stored on the server): favourite files
// and folders, recently inserted media, hidden folders. Loaded once, updated optimistically.
import { api } from './api';
import type { PrefItem, Prefs } from './types';

const key = (x: { libraryId: string; path: string }) => x.libraryId + '\u0000' + x.path;

class PrefsState {
  favorites = $state<PrefItem[]>([]);
  recent = $state<PrefItem[]>([]);
  hidden = $state<PrefItem[]>([]);
  loaded = $state(false);
  private loading: Promise<void> | null = null;

  load() {
    this.loading ??= api
      .prefs()
      .then((p) => this.set(p))
      .catch(() => {
        this.loading = null;
      });
    return this.loading;
  }

  private set(p: Prefs) {
    this.favorites = p.favorites;
    this.recent = p.recent;
    this.hidden = p.hidden;
    this.loaded = true;
  }

  isFavorite(x: { libraryId: string; path: string }) {
    const k = key(x);
    return this.favorites.some((f) => key(f) === k);
  }

  async toggleFavorite(x: { libraryId: string; path: string; dir?: boolean }) {
    const k = key(x);
    const next = this.isFavorite(x)
      ? this.favorites.filter((f) => key(f) !== k)
      : [{ libraryId: x.libraryId, path: x.path, dir: x.dir, at: Date.now() }, ...this.favorites];
    this.favorites = next;
    this.set(await api.savePrefs({ favorites: next }));
  }

  /** remember inserted media (most recent first); fire and forget */
  addRecent(items: { libraryId: string; path: string }[]) {
    if (!items.length) return;
    const add = items.map((x) => ({ libraryId: x.libraryId, path: x.path }));
    const ks = new Set(add.map(key));
    this.recent = [...add.map((x) => ({ ...x, at: Date.now() })), ...this.recent.filter((r) => !ks.has(key(r)))].slice(0, 200);
    api.savePrefs({ addRecent: add }).catch(() => {});
  }

  async clearRecent() {
    this.recent = [];
    this.set(await api.savePrefs({ recent: [] }));
  }

  async setHidden(next: PrefItem[]) {
    this.hidden = next;
    this.set(await api.savePrefs({ hidden: next }));
  }
}

export const prefs = new PrefsState();
