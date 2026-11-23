// Editor state for the open document: autosave with debounce, version check against
// concurrent edits (another tab or computer), and flushing on tab close.
import { api, ApiError, newId } from './api';
import { prefs } from './prefs.svelte';
import type { Doc, Layout, MediaRef, Section } from './types';

export type SaveStatus = 'saved' | 'dirty' | 'saving' | 'error' | 'conflict' | 'loading';

const SAVE_DELAY = 900;

export function emptyText() {
  return { type: 'doc', content: [{ type: 'paragraph' }] };
}

export function newSection(layout: Layout = 'media-top', media: MediaRef[] = []): Section {
  return { id: newId(), title: '', layout, media, text: emptyText(), slide: true };
}

export class DocState {
  doc = $state<Doc | null>(null);
  status = $state<SaveStatus>('loading');
  error = $state('');
  conflict = $state<Doc | null>(null);
  /** bumped whenever the whole document is replaced (editors re-mount) */
  revision = $state(0);
  activeSection = $state<string | null>(null);
  lastSavedAt = $state<string>('');

  /** edits kept in this browser that never reached the server (crash, closed tab, offline) */
  recovery = $state<{ doc: Doc; at: number } | null>(null);

  private baseVersion = 0;
  private localTimer: ReturnType<typeof setTimeout> | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private saving: Promise<void> | null = null;
  private again = false;
  private seq = 0;

  private localKey(id: string) {
    return 'letopis.unsaved.' + id;
  }

  /** keep a copy of unsaved edits in the browser (debounced) */
  private keepLocal() {
    if (this.localTimer) clearTimeout(this.localTimer);
    this.localTimer = setTimeout(() => {
      if (!this.doc) return;
      try {
        localStorage.setItem(this.localKey(this.doc.id), JSON.stringify({ base: this.baseVersion, at: Date.now(), doc: $state.snapshot(this.doc) }));
      } catch {
        /* quota: not fatal */
      }
    }, 400);
  }

  private dropLocal(id: string) {
    if (this.localTimer) clearTimeout(this.localTimer);
    this.localTimer = null;
    try {
      localStorage.removeItem(this.localKey(id));
    } catch {}
  }

  /** put the recovered edits back (they are saved on top of the server version) */
  restoreRecovery() {
    const r = this.recovery;
    if (!r || !this.doc) return;
    const base = this.doc.version;
    this.replace({ ...r.doc, version: base, updatedAt: this.doc.updatedAt });
    this.recovery = null;
    this.touch();
  }

  discardRecovery() {
    if (this.doc) this.dropLocal(this.doc.id);
    this.recovery = null;
  }

  async load(id: string) {
    this.status = 'loading';
    this.error = '';
    try {
      const d = await api.getDoc(id);
      this.replace(d);
      this.status = 'saved';
      try {
        const raw = localStorage.getItem(this.localKey(id));
        if (raw) {
          const v = JSON.parse(raw) as { base: number; at: number; doc: Doc };
          const same = JSON.stringify({ ...v.doc, version: 0, updatedAt: '' }) === JSON.stringify({ ...d, version: 0, updatedAt: '' });
          if (same || !v.doc?.sections) this.dropLocal(id);
          else this.recovery = { doc: v.doc, at: v.at };
        }
      } catch {}
    } catch (e) {
      this.status = 'error';
      this.error = e instanceof ApiError && e.status === 404 ? 'Document not found.' : String((e as Error).message);
    }
  }

  replace(d: Doc) {
    for (const s of d.sections) {
      s.media ??= [];
      s.text ??= emptyText();
    }
    this.doc = d;
    this.baseVersion = d.version;
    this.lastSavedAt = d.updatedAt;
    this.conflict = null;
    this.revision++;
  }

  /** Call after every change. */
  touch() {
    if (!this.doc || this.status === 'conflict') return;
    this.seq++;
    this.keepLocal();
    this.status = 'dirty';
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => this.save(), SAVE_DELAY);
  }

  async save(force = false, keepalive = false): Promise<void> {
    if (this.timer) {
      clearTimeout(this.timer);
      this.timer = null;
    }
    if (!this.doc) return;
    if (this.saving) {
      this.again = true;
      return this.saving;
    }
    const snapshot = $state.snapshot(this.doc) as Doc;
    const startSeq = this.seq;
    this.status = 'saving';
    this.saving = (async () => {
      try {
        const r = await api.saveDoc(snapshot.id, this.baseVersion, snapshot, force, keepalive);
        this.baseVersion = r.version;
        if (this.doc) {
          this.doc.version = r.version;
          this.doc.updatedAt = r.updatedAt;
        }
        this.lastSavedAt = r.updatedAt;
        if (this.seq !== startSeq || this.again) {
          this.status = 'dirty';
          if (!this.timer) this.timer = setTimeout(() => this.save(), SAVE_DELAY);
        } else {
          this.status = 'saved';
          this.dropLocal(snapshot.id);
        }
        this.error = '';
      } catch (e) {
        if (e instanceof ApiError && e.status === 409) {
          this.status = 'conflict';
          this.conflict = e.body?.current ?? null;
        } else {
          this.status = 'error';
          this.error = (e as Error).message;
          // retry later
          this.timer = setTimeout(() => this.save(), 5000);
        }
      } finally {
        this.saving = null;
      }
    })();
    await this.saving;
    if (this.again && (this.status as SaveStatus) !== 'conflict') {
      this.again = false;
      return this.save();
    }
  }

  /** Conflict resolution: keep mine (overwrite) or take theirs. */
  async keepMine() {
    this.status = 'dirty';
    this.conflict = null;
    await this.save(true);
  }
  takeTheirs() {
    if (this.doc) this.dropLocal(this.doc.id);
    if (this.conflict) this.replace(this.conflict);
    this.status = 'saved';
  }

  flush() {
    if (this.status === 'dirty' || this.timer) void this.save(false, true);
  }

  get pending() {
    return this.status === 'dirty' || this.status === 'saving';
  }

  // ---------- mutations ----------

  sectionIndex(id: string) {
    return this.doc ? this.doc.sections.findIndex((s) => s.id === id) : -1;
  }

  addSection(at: number | null, sec: Section = newSection()) {
    if (!this.doc) return sec;
    const i = at === null ? this.doc.sections.length : Math.max(0, Math.min(at, this.doc.sections.length));
    this.doc.sections.splice(i, 0, sec);
    this.activeSection = sec.id;
    this.touch();
    return sec;
  }

  removeSection(id: string) {
    if (!this.doc) return;
    const i = this.sectionIndex(id);
    if (i >= 0) {
      this.doc.sections.splice(i, 1);
      this.touch();
    }
  }

  duplicateSection(id: string) {
    if (!this.doc) return;
    const i = this.sectionIndex(id);
    if (i < 0) return;
    const copy = structuredClone($state.snapshot(this.doc.sections[i])) as Section;
    copy.id = newId();
    this.doc.sections.splice(i + 1, 0, copy);
    this.activeSection = copy.id;
    this.touch();
  }

  moveSection(from: number, to: number) {
    if (!this.doc || from === to || from < 0 || to < 0) return;
    const arr = this.doc.sections;
    if (from >= arr.length || to >= arr.length) return;
    const [s] = arr.splice(from, 1);
    arr.splice(to, 0, s);
    this.touch();
  }

  addMedia(sectionId: string | null, refs: MediaRef[], newAt: number | null = null) {
    if (!this.doc || !refs.length) return;
    prefs.addRecent(refs);
    let sec = sectionId ? this.doc.sections.find((s) => s.id === sectionId) : undefined;
    if (!sec) {
      const layout: Layout = refs.length > 1 ? 'gallery' : 'media-top';
      sec = this.addSection(newAt, newSection(layout, []));
      sec = this.doc.sections.find((s) => s.id === sec!.id)!;
    } else if (sec.layout === 'text-only') {
      sec.layout = 'media-top';
    }
    sec.media.push(...refs);
    if (sec.media.length > 1 && sec.layout === 'media-top' && refs.length > 1) sec.layout = 'gallery';
    this.activeSection = sec.id;
    this.touch();
  }
}
