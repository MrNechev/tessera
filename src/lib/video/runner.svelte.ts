/**
 * The running video job, kept outside the dialog: closing the dialog doesn't
 * stop the video, and the toast with "Open"/"Download" comes when it is done.
 * Progress arrives as `video` events; a slow poll covers a dropped event stream.
 */
import { subscribe } from '../api/http';
import { api, downloadUrl, isDesktop } from '../api/backend';
import { platform } from '../platform';
import { toasts } from '../stores/toasts.svelte';
import { videoApi, type VideoItem, type VideoOptions, type VideoStatus } from './video';

function download(path: string): void {
  const a = document.createElement('a');
  a.href = downloadUrl(path);
  a.download = '';
  document.body.appendChild(a);
  a.click();
  a.remove();
}

class VideoRunner {
  /** The current (or last) job. */
  status = $state<VideoStatus | null>(null);
  /** Preparing items in the browser (title cards, collage pages) before the engine starts. */
  preparing = $state<string | null>(null);

  get busy(): boolean {
    return this.preparing !== null || this.status?.state === 'queued' || this.status?.state === 'running';
  }

  async start(items: () => Promise<VideoItem[]>, options: VideoOptions): Promise<void> {
    if (this.busy) return;
    this.status = null;
    this.preparing = 'Preparing';
    let first: VideoStatus;
    try {
      const list = await items();
      if (!list.length) throw new Error('There are no photos or clips to put in the video.');
      this.preparing = 'Starting';
      first = await videoApi.start(list, options);
    } catch (e) {
      this.preparing = null;
      toasts.push({ kind: 'error', title: 'Could not make the video', message: e instanceof Error ? e.message : String(e) });
      return;
    }
    this.preparing = null;
    this.status = first;
    await this.follow(first.job, options.name);
  }

  cancel(): void {
    if (this.status && this.busy) void videoApi.cancel(this.status.job).catch(() => {});
  }

  private follow(job: number, name: string): Promise<void> {
    return new Promise((resolve) => {
      let timer: ReturnType<typeof setInterval> | undefined;
      const finish = (s: VideoStatus) => {
        unsubscribe();
        clearInterval(timer);
        this.report(s, name);
        resolve();
      };
      const seen = (s: VideoStatus) => {
        if (s.job !== job || !this.status || this.status.job !== job) return;
        this.status = s;
        if (s.state === 'done' || s.state === 'failed' || s.state === 'cancelled') finish(s);
      };
      const unsubscribe = subscribe((m) => {
        if (m.type === 'video') seen(m.data as VideoStatus);
      });
      timer = setInterval(() => {
        videoApi
          .status(job)
          .then(seen)
          .catch(() => {});
      }, 2000);
    });
  }

  private report(s: VideoStatus, name: string): void {
    if (s.state === 'cancelled') {
      toasts.push({ kind: 'info', title: 'Video cancelled' });
      return;
    }
    if (s.state === 'failed') {
      toasts.push({ kind: 'error', title: 'The video could not be made', message: s.error });
      return;
    }
    const path = s.path!;
    const file = path.split('/').pop() ?? name;
    const len = `${Math.floor(s.seconds / 60)}:${String(Math.round(s.seconds % 60)).padStart(2, '0')}`;
    toasts.push({
      kind: 'success',
      title: 'Video ready',
      message: `${file} · ${len} · saved in Outputs/Videos`,
      timeout: 12000,
      action: isDesktop
        ? { label: 'Open', run: () => void api.openPath(path) }
        : // Download now, whatever the automatic-download setting says (the user asked).
          { label: 'Download', run: () => download(path) },
    });
    // Browsers: hand the file over like every other tool result (setting-controlled).
    platform.deliver([path]);
  }
}

export const videoRunner = new VideoRunner();
