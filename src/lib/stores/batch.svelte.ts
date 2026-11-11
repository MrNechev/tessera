/** File queues for the converter and the compressor (kept while switching views). */
import { api, isTauri } from '../api/backend';
import type { JobResult, ProbeInfo } from '../api/types';
import { toasts } from './toasts.svelte';

export class BatchQueue {
  files = $state<ProbeInfo[]>([]);
  results = $state<JobResult[]>([]);
  running = $state(false);
  progress = $state<{ done: number; total: number; current: string | null } | null>(null);
  adding = $state(false);

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

export const convertQueue = new BatchQueue();
export const compressQueue = new BatchQueue();
export const toolsQueue = new BatchQueue();

export function formatBytes(n: number): string {
  if (!Number.isFinite(n)) return '—';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(n < 10 * 1024 * 1024 ? 2 : 1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
