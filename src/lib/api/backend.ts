/** Typed wrappers around the engine's tool API, plus platform-specific file pickers. */
import { call, callBinary, fetchBytes, subscribe } from './http';
import { platform } from '../platform';
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
  BatchJob,
  BatchOptions,
  ProgressEvent,
  ProjectSummary,
  Settings,
  UpdateInfo,
} from './types';

/** True inside the desktop app's window (native dialogs, drag-and-drop paths, printing). */
export const isTauri = platform.desktop;
export const isDesktop = platform.desktop;

/** URL of a file on the engine's machine (previews, thumbnails, results). */
export function fileUrl(path: string, version?: string | number): string {
  const v = version === undefined ? '' : `&v=${encodeURIComponent(String(version))}`;
  return `/api/t/file?path=${encodeURIComponent(path)}${v}`;
}

/** URL that downloads a file (attachment). */
export function downloadUrl(path: string, name?: string): string {
  const n = name ? `&name=${encodeURIComponent(name)}` : '';
  return `/api/t/download?path=${encodeURIComponent(path)}${n}`;
}

export const api = {
  appInfo: () => call<AppInfo>('app_info'),
  loadSettings: () => call<Settings>('load_settings'),
  saveSettings: (settings: Settings) => call<void>('save_settings', { settings }),

  importImages: (paths: string[]) => call<ImportResult>('import_images', { paths }),
  probeFiles: (paths: string[]) => call<ProbeResult>('probe_files', { paths }),

  /** Full-quality, upright pixels, fitted inside maxWidth × maxHeight. */
  async loadImagePixels(path: string, maxWidth: number, maxHeight: number): Promise<ImageData> {
    const buf = await fetchBytes('/api/t/load_image_rgba', { path, maxWidth, maxHeight });
    const view = new DataView(buf);
    const w = view.getUint32(0, true);
    const h = view.getUint32(4, true);
    return new ImageData(new Uint8ClampedArray(buf, 8, w * h * 4), w, h);
  },

  exportBegin: (width: number, height: number) => call<number>('export_begin', { width, height }),
  exportTile: (job: number, x: number, y: number, w: number, h: number, rgba: Uint8ClampedArray) =>
    callBinary<void>('/api/t/export_tile', new Uint8Array(rgba.buffer, rgba.byteOffset, rgba.byteLength), {
      'x-job': String(job),
      'x-rect': `${x},${y},${w},${h}`,
    }),
  exportFinish: (job: number, path: string, format: string, quality: number, dpi: number, unique: boolean) =>
    call<ExportResult>('export_finish', { job, path, format, quality, dpi, unique }),
  exportCancel: (job: number) => call<void>('export_cancel', { job }),
  pdfBegin: () => call<number>('pdf_begin'),
  pdfAddPage: (pdf: number, job: number, quality: number, dpi: number) => call<void>('pdf_add_page', { pdf, job, quality, dpi }),
  pdfFinish: (pdf: number, path: string, unique: boolean) => call<ExportResult>('pdf_finish', { pdf, path, unique }),
  pdfCancel: (pdf: number) => call<void>('pdf_cancel', { pdf }),
  /** Native print dialog sized to the page; 'unsupported' means use window.print(). */
  printNative: (widthMm: number, heightMm: number, name: string) =>
    platform.printNative(widthMm, heightMm, name),

  convertImages: (files: string[], options: ConvertOptions) => call<JobResult[]>('convert_images', { files, options }),
  compressImages: (files: string[], options: CompressOptions) => call<JobResult[]>('compress_images', { files, options }),
  compressPreview: (path: string, options: CompressOptions) =>
    call<CompressPreviewInfo>('compress_preview', { path, options }),
  cancelBatch: () => call<void>('cancel_batch'),
  batchProcess: (jobs: BatchJob[], options: BatchOptions) => call<JobResult[]>('batch_process', { jobs, options }),
  saveTempImage: (png: Uint8Array) => callBinary<string>('/api/t/save_temp_image', png),
  readMetadata: (path: string) => call<MetadataSummary>('read_metadata', { path }),

  listProjects: () => call<ProjectSummary[]>('list_projects'),
  loadProject: (path: string) => call<string>('load_project', { path }),
  loadProjectById: (id: string) => call<string>('load_project_by_id', { id }),
  saveProject: (id: string, json: string, thumbnail: string | null) =>
    call<ProjectSummary>('save_project', { id, json, thumbnail }),
  saveProjectFile: (path: string, json: string) => call<void>('save_project_file', { path, json }),
  deleteProject: (id: string) => call<void>('delete_project', { id }),

  /** Desktop: show in the file manager. Browser: download the file. */
  revealPath: (path: string) => platform.revealPath(path),
  /** Desktop: open with the default app. Browser: open in a new tab. */
  openPath: (path: string) => platform.openPath(path),
  ensureDir: (path: string) => call<string>('ensure_dir', { path }),
  checkUpdates: () => call<UpdateInfo>('check_updates'),
  openUrl: (url: string) => platform.openUrl(url),
  clearCache: () => call<number>('clear_cache'),
  cacheSize: () => call<number>('cache_size'),
  /** Whether the engine may write to this path (write guard). */
  writable: (path: string) => call<boolean>('writable', { path }),
};

export function onProgress(fn: (e: ProgressEvent) => void): Promise<() => void> {
  return Promise.resolve(
    subscribe((m) => {
      if (m.type === 'progress') fn(m.data as ProgressEvent);
    }),
  );
}

const FALLBACK_EXTENSIONS = ['jpg', 'jpeg', 'png', 'webp', 'avif', 'heic', 'heif', 'tif', 'tiff', 'bmp', 'gif', 'jxl', 'qoi', 'tga', 'ico', 'dng', 'cr2', 'cr3', 'nef', 'arw', 'raf', 'orf', 'rw2', 'pef'];
const RAW_EXTENSIONS = new Set(['dng', 'cr2', 'cr3', 'crw', 'nef', 'nrw', 'arw', 'srf', 'sr2', 'raf', 'orf', 'rw2', 'pef', 'srw', 'erf', 'mef', 'mos', 'mrw', 'kdc', 'dcr', '3fr', 'iiq', 'x3f', 'rwl', 'raw']);

/** Extensions the backend can import (set once app info is known). */
let importExtensions: string[] = FALLBACK_EXTENSIONS;
export function setImportExtensions(exts: string[]): void {
  if (exts.length) importExtensions = exts;
}
export function getImportExtensions(): string[] {
  return importExtensions;
}

export async function pickImages(defaultPath?: string | null, title = 'Add photos'): Promise<string[]> {
  const raw = importExtensions.filter((e) => RAW_EXTENSIONS.has(e));
  return platform.pickFiles({
    title,
    defaultPath: defaultPath ?? undefined,
    filters: [
      { name: 'Images', extensions: importExtensions },
      ...(raw.length ? [{ name: 'Camera RAW', extensions: raw }] : []),
      { name: 'All files', extensions: ['*'] },
    ],
  });
}

export async function pickFolder(title = 'Choose a folder', defaultPath?: string | null, forSaving = false): Promise<string | null> {
  return platform.pickFolder({ title, defaultPath: defaultPath ?? undefined, forSaving });
}

export async function pickSavePath(
  defaultPath: string,
  filters: { name: string; extensions: string[] }[],
  title = 'Save as',
): Promise<string | null> {
  return platform.pickSavePath({ defaultPath, filters, title });
}

export async function pickProjectFile(): Promise<string | null> {
  const files = await platform.pickFiles({
    title: 'Open project',
    multiple: false,
    filters: [{ name: 'Tessera project', extensions: ['tessera'] }],
  });
  return files[0] ?? null;
}

/** After saving in the browser, offer the result as a download (setting-controlled). */
export function deliver(paths: string[]): void {
  platform.deliver(paths);
}
