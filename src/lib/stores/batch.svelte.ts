/** File queues and options for the converter, the compressor and the batch
 *  tools (kept while switching views for the rest of the session). */
import { api, isTauri } from '../api/backend';
import {
  defaultMetadataPolicy,
  type ColorMode,
  type CompressLevel,
  type ConflictPolicy,
  type ImageFormat,
  type JobResult,
  type MetadataPolicy,
  type OutputSpec,
  type ProbeInfo,
} from '../api/types';
import { DEFAULT_RENAME, type RenameSpec } from '../tools/rename';
import type { TextWatermark, WatermarkPosition } from '../tools/watermark';
import { toasts } from './toasts.svelte';

export class BatchQueue {
  /** Tool name shown when another view has to wait for this queue. */
  readonly label: string;
  files = $state<ProbeInfo[]>([]);
  results = $state<JobResult[]>([]);
  running = $state(false);
  progress = $state<{ done: number; total: number; current: string | null } | null>(null);
  adding = $state(false);

  constructor(label: string) {
    this.label = label;
  }

  async add(paths: string[]): Promise<void> {
    if (!paths.length || !isTauri) return;
    this.adding = true;
    try {
      const res = await api.probeFiles(paths);
      const known = new Set(this.files.map((f) => f.path));
      const fresh = res.files.filter((f) => !known.has(f.path));
      this.files = [...this.files, ...fresh];
      if (res.failed.length) {
        toasts.push({
          kind: 'warning',
          title: `${res.failed.length} file${res.failed.length > 1 ? 's' : ''} skipped`,
          message: `${res.failed[0].path.split('/').pop()}: ${res.failed[0].error}`,
        });
      }
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not read files', message: String(e) });
    } finally {
      this.adding = false;
    }
  }

  remove(path: string): void {
    this.files = this.files.filter((f) => f.path !== path);
  }

  clear(): void {
    this.files = [];
    this.results = [];
  }
}

export const convertQueue = new BatchQueue('Convert');
export const compressQueue = new BatchQueue('Compress');
export const toolsQueue = new BatchQueue('Batch tools');

const toolQueues = [convertQueue, compressQueue, toolsQueue];

/**
 * The backend has a single cancel flag for all three tools (`cancel_batch`),
 * so only one tool job may run at a time: returns the other queue that is
 * currently running, if any.
 */
export function busyElsewhere(queue: BatchQueue): BatchQueue | undefined {
  return toolQueues.find((q) => q !== queue && q.running);
}

/* ------------------------------------------------------------- options */

export const convertOptions = $state({
  target: 'jpeg' as ImageFormat | 'keep',
  qualityMode: 'original' as 'original' | 'reduce',
  reduce: 20,
  resizeMode: 'none' as 'none' | 'percent' | 'longEdge',
  percent: 50,
  longEdge: 2048,
  metadata: defaultMetadataPolicy() as MetadataPolicy,
  output: { directory: null, suffix: '', conflict: 'rename' } as OutputSpec,
  bg: '#ffffff',
  color: 'srgb' as ColorMode,
});

export const compressOptions = $state({
  level: 'visuallyLossless' as CompressLevel['kind'],
  quality: 80,
  targetKb: 500,
  format: 'keep' as ImageFormat | 'keep',
  longEdge: 0,
  metadata: defaultMetadataPolicy() as MetadataPolicy,
  output: { directory: null, suffix: '-compressed', conflict: 'rename' } as OutputSpec,
  keepIfLarger: true,
  color: 'keep' as ColorMode,
});

export const batchOptions = $state({
  doResize: true,
  resizeKind: 'longEdge' as 'longEdge' | 'width' | 'height' | 'percent',
  resizeValue: 2048,

  doWatermark: false,
  wmKind: 'text' as 'text' | 'image',
  wmText: { text: '© Your Name', font: 'montserrat', weight: 600, color: '#ffffff', outline: false, shadow: true } as TextWatermark,
  wmLogo: null as string | null,
  wmSize: 0.25,
  wmOpacity: 0.75,
  wmMargin: 0.03,
  wmPosition: 'br' as WatermarkPosition,

  doRename: false,
  rename: { ...DEFAULT_RENAME, pattern: '{date} {n}' } as RenameSpec,
  suffix: '-edited',

  format: 'keep' as ImageFormat | 'keep',
  quality: 90,
  metadata: defaultMetadataPolicy() as MetadataPolicy,
  directory: null as string | null,
  conflict: 'rename' as ConflictPolicy,
  moveOriginals: false,
});

/* ------------------------------------------------------------- helpers */

/** Whether a `bind:value` number input holds a number (Svelte yields `null` when it is emptied). */
export function isNum(v: unknown): v is number {
  return typeof v === 'number' && Number.isFinite(v);
}

/** A number input's value as an integer clamped to [min, max], or `fallback` when empty/invalid. */
export function toInt(v: unknown, min: number, max: number, fallback: number): number {
  return isNum(v) ? Math.min(max, Math.max(min, Math.round(v))) : fallback;
}

/** Toast for a finished job: success, partial failure (warning) or total failure (error). */
export function reportResults(results: JobResult[], t: { verb: string; title: string; failTitle: string }, extra = ''): void {
  const total = results.length;
  const ok = results.filter((r) => r.status === 'done').length;
  const failedList = results.filter((r) => r.status === 'failed');
  const failed = failedList.length;
  const images = (n: number) => `image${n === 1 ? '' : 's'}`;
  if (failed && failed === total) {
    const first = failedList[0].message;
    toasts.push({
      kind: 'error',
      title: t.failTitle,
      message: `${total === 1 ? 'The image' : `All ${total} ${images(total)}`} failed.${first ? ` ${first}` : ''}`,
    });
  } else if (failed) {
    toasts.push({ kind: 'warning', title: `${failed} of ${total} failed`, message: `${ok} of ${total} ${images(total)} ${t.verb}.` });
  } else {
    toasts.push({ kind: 'success', title: t.title, message: `${ok} of ${total} ${images(total)} ${t.verb}${extra}.` });
  }
}

export function formatBytes(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(n < 10 * 1024 * 1024 ? 2 : 1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
