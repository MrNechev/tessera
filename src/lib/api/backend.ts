/** Thin, typed wrappers around Tauri commands. */
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import type {
  AppInfo,
  CompressOptions,
  CompressPreviewInfo,
  ConvertOptions,
  ExportResult,
  ImportResult,
  JobResult,
  MetadataSummary,
  ProbeResult,
  ProgressEvent,
  ProjectSummary,
  Settings,
  UpdateInfo,
} from './types';

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function fileUrl(path: string): string {
  return isTauri ? convertFileSrc(path) : path;
}

export const api = {
  appInfo: () => invoke<AppInfo>('app_info'),
  loadSettings: () => invoke<Settings>('load_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),

  importImages: (paths: string[]) => invoke<ImportResult>('import_images', { paths }),
  probeFiles: (paths: string[]) => invoke<ProbeResult>('probe_files', { paths }),
  thumbnail: (path: string) => invoke<string>('thumbnail', { path }),

  /** Full-quality, upright pixels, fitted inside maxWidth × maxHeight. */
  async loadImagePixels(path: string, maxWidth: number, maxHeight: number): Promise<ImageData> {
    const buf = await invoke<ArrayBuffer>('load_image_rgba', { path, maxWidth, maxHeight });
    const view = new DataView(buf);
    const w = view.getUint32(0, true);
    const h = view.getUint32(4, true);
    return new ImageData(new Uint8ClampedArray(buf, 8, w * h * 4), w, h);
  },

  exportBegin: (width: number, height: number) => invoke<number>('export_begin', { width, height }),
  exportTile: (job: number, x: number, y: number, w: number, h: number, rgba: Uint8ClampedArray) =>
    invoke<void>('export_tile', new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength), {
      headers: { 'x-job': String(job), 'x-rect': `${x},${y},${w},${h}` },
    }),
  exportFinish: (job: number, path: string, format: string, quality: number, dpi: number, unique: boolean) =>
    invoke<ExportResult>('export_finish', { job, path, format, quality, dpi, unique }),
  exportCancel: (job: number) => invoke<void>('export_cancel', { job }),

  convertImages: (files: string[], options: ConvertOptions) => invoke<JobResult[]>('convert_images', { files, options }),
  compressImages: (files: string[], options: CompressOptions) => invoke<JobResult[]>('compress_images', { files, options }),
  compressPreview: (path: string, options: CompressOptions) =>
    invoke<CompressPreviewInfo>('compress_preview', { path, options }),
  cancelBatch: () => invoke<void>('cancel_batch'),
  readMetadata: (path: string) => invoke<MetadataSummary>('read_metadata', { path }),

  listProjects: () => invoke<ProjectSummary[]>('list_projects'),
  loadProject: (path: string) => invoke<string>('load_project', { path }),
  saveProject: (id: string, json: string, thumbnail: string | null) =>
    invoke<ProjectSummary>('save_project', { id, json, thumbnail }),
  saveProjectFile: (path: string, json: string) => invoke<void>('save_project_file', { path, json }),
  deleteProject: (id: string) => invoke<void>('delete_project', { id }),

  revealPath: (path: string) => invoke<void>('reveal_path', { path }),
  openPath: (path: string) => invoke<void>('open_path', { path }),
  ensureDir: (path: string) => invoke<string>('ensure_dir', { path }),
  checkUpdates: () => invoke<UpdateInfo>('check_updates'),
  openUrl: (url: string) => invoke<void>('open_url', { url }),
  clearCache: () => invoke<number>('clear_cache'),
  cacheSize: () => invoke<number>('cache_size'),
};

export function onProgress(fn: (e: ProgressEvent) => void): Promise<UnlistenFn> {
  return listen<ProgressEvent>('tessera://progress', (e) => fn(e.payload));
}

export async function pickImages(defaultPath?: string | null): Promise<string[]> {
  const res = await openDialog({
    multiple: true,
    directory: false,
    defaultPath: defaultPath ?? undefined,
    title: 'Add photos',
    filters: [
      {
        name: 'Images',
        extensions: ['jpg', 'jpeg', 'png', 'webp', 'avif', 'heic', 'heif', 'tif', 'tiff', 'bmp', 'gif', 'jxl', 'qoi', 'tga', 'ico'],
      },
      { name: 'All files', extensions: ['*'] },
    ],
  });
  if (!res) return [];
  return Array.isArray(res) ? res : [res];
}

export async function pickFolder(title = 'Choose a folder', defaultPath?: string | null): Promise<string | null> {
  const res = await openDialog({ directory: true, multiple: false, title, defaultPath: defaultPath ?? undefined });
  return typeof res === 'string' ? res : null;
}

export async function pickSavePath(
  defaultPath: string,
  filters: { name: string; extensions: string[] }[],
  title = 'Save as',
): Promise<string | null> {
  const res = await saveDialog({ defaultPath, filters, title });
  return res ?? null;
}

export async function pickProjectFile(): Promise<string | null> {
  const res = await openDialog({
    multiple: false,
    directory: false,
    title: 'Open project',
    filters: [{ name: 'Tessera project', extensions: ['tessera'] }],
  });
  return typeof res === 'string' ? res : null;
}
