/**
 * Platform adapter: the few things that differ between the desktop window
 * (Tauri: native dialogs, real file paths, GTK printing) and a browser
 * talking to the Docker server (in-app server file picker, uploads, downloads).
 */
import { picker } from './picker.svelte';

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface PickFilesOptions {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
  multiple?: boolean;
}

export interface PickFolderOptions {
  title?: string;
  defaultPath?: string;
  /** The folder will receive results: only writable folders can be chosen. */
  forSaving?: boolean;
}

export interface PickSaveOptions {
  title?: string;
  defaultPath: string;
  filters: FileFilter[];
}

export interface Platform {
  desktop: boolean;
  pickFiles(o: PickFilesOptions): Promise<string[]>;
  pickFolder(o: PickFolderOptions): Promise<string | null>;
  pickSavePath(o: PickSaveOptions): Promise<string | null>;
  printNative(widthMm: number, heightMm: number, name: string): Promise<'printing' | 'cancelled' | 'unsupported'>;
  revealPath(path: string): Promise<void>;
  openPath(path: string): Promise<void>;
  openUrl(url: string): Promise<void>;
  /** Hand finished results to the user (browser: download them). */
  deliver(paths: string[]): void;
}

const isDesktop = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(cmd, args);
}

const desktop: Platform = {
  desktop: true,
  async pickFiles(o) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const res = await open({
      multiple: o.multiple ?? true,
      directory: false,
      defaultPath: o.defaultPath,
      title: o.title,
      filters: o.filters,
    });
    if (!res) return [];
    return Array.isArray(res) ? res : [res];
  },
  async pickFolder(o) {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const res = await open({ directory: true, multiple: false, title: o.title, defaultPath: o.defaultPath });
    return typeof res === 'string' ? res : null;
  },
  async pickSavePath(o) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    return (await save({ defaultPath: o.defaultPath, filters: o.filters, title: o.title })) ?? null;
  },
  printNative: (widthMm, heightMm, name) => invoke('print_native', { widthMm, heightMm, name }),
  revealPath: (path) => invoke('reveal_path', { path }),
  openPath: (path) => invoke('open_path', { path }),
  openUrl: (url) => invoke('open_url', { url }),
  deliver() {
    /* files are already on this computer */
  },
};

function download(href: string): void {
  const a = document.createElement('a');
  a.href = href;
  a.download = '';
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
}

/** POST a zip request as a form so the browser streams it straight to disk. */
function downloadZip(paths: string[], name: string): void {
  const form = document.createElement('form');
  form.method = 'POST';
  form.action = '/api/t/zip';
  form.style.display = 'none';
  const field = document.createElement('input');
  field.type = 'hidden';
  field.name = 'json';
  field.value = JSON.stringify({ paths, name });
  form.appendChild(field);
  document.body.appendChild(form);
  form.submit();
  form.remove();
}

const web: Platform = {
  desktop: false,
  async pickFiles(o) {
    const res = await picker.open({
      mode: 'files',
      title: o.title ?? 'Choose files',
      startPath: o.defaultPath,
      extensions: o.filters?.flatMap((f) => f.extensions).filter((e) => e !== '*') ?? [],
      multiple: o.multiple ?? true,
    });
    return res ?? [];
  },
  async pickFolder(o) {
    const res = await picker.open({
      mode: 'folder',
      title: o.title ?? 'Choose a folder',
      startPath: o.defaultPath,
      writableOnly: o.forSaving ?? false,
    });
    return res?.[0] ?? null;
  },
  async pickSavePath(o) {
    const slash = o.defaultPath.lastIndexOf('/');
    const res = await picker.open({
      mode: 'save',
      title: o.title ?? 'Save as',
      startPath: slash > 0 ? o.defaultPath.slice(0, slash) : undefined,
      fileName: slash >= 0 ? o.defaultPath.slice(slash + 1) : o.defaultPath,
      extensions: o.filters.flatMap((f) => f.extensions),
      writableOnly: true,
    });
    return res?.[0] ?? null;
  },
  async printNative() {
    return 'unsupported';
  },
  async revealPath(path) {
    download(`/api/t/download?path=${encodeURIComponent(path)}`);
  },
  async openPath(path) {
    window.open(`/api/t/file?path=${encodeURIComponent(path)}`, '_blank', 'noopener');
  },
  async openUrl(url) {
    window.open(url, '_blank', 'noopener');
  },
  deliver(paths) {
    if (!paths.length || !picker.downloadResults) return;
    if (paths.length === 1) download(`/api/t/download?path=${encodeURIComponent(paths[0])}`);
    else downloadZip(paths, 'Tessera results.zip');
  },
};

export const platform: Platform = isDesktop ? desktop : web;
