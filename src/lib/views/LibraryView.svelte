<script lang="ts">
  /**
   * The media library (from Letopis): every photo and video in the configured
   * libraries, with search, filters, duplicates and previews. Picked items can
   * start a collage or go to the image tools.
   */
  import { ArrowLeftRight, BookOpen, CalendarDays, Film, LayoutDashboard, Minimize2, Wand, X } from '@lucide/svelte';
  import LibraryPanel from '../documents/components/LibraryPanel.svelte';
  import VideoDialog from '../components/VideoDialog.svelte';
  import type { FileItem } from '../documents/lib/types';
  import { call } from '../api/http';
  import { newQuickCollage } from '../editor/session.svelte';
  import { library } from '../stores/library.svelte';
  import { app } from '../stores/app.svelte';
  import { compressQueue, convertQueue, toolsQueue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { palette } from '../documents/lib/palette.svelte';

  let picked = $state<FileItem[]>([]);
  /** The selection the video dialog was opened with (kept if the selection changes). */
  let videoFiles = $state<FileItem[] | null>(null);
  let panel: LibraryPanel | undefined = $state();

  function clear() {
    picked = [];
    panel?.clearSelection();
  }

  // Ctrl+K: actions on the selection.
  $effect(() =>
    palette.register('library', () => {
      if (!images.length) return [];
      const n = `${images.length} photo${images.length === 1 ? '' : 's'}`;
      return [
        { id: 'lib:collage', group: 'Selection', icon: 'layout', title: `Make a collage from ${n}`, run: () => use('collage') },
        { id: 'lib:book', group: 'Selection', icon: 'notes', title: `Make a photo book from ${n}`, run: () => use('book') },
        { id: 'lib:calendar', group: 'Selection', icon: 'calendar', title: `Make a calendar from ${n}`, run: () => use('calendar') },
        { id: 'lib:convert', group: 'Selection', icon: 'refresh', title: `Convert ${n}`, run: () => use('convert') },
        { id: 'lib:compress', group: 'Selection', icon: 'package', title: `Compress ${n}`, run: () => use('compress') },
        { id: 'lib:video', group: 'Selection', icon: 'video', title: `Make a video from ${n}`, run: () => void (videoFiles = [...visual]) },
        { id: 'lib:batch', group: 'Selection', icon: 'sliders', title: `Resize, watermark or rename ${n}`, run: () => use('batch') },
      ];
    }),
  );
  let busy = $state(false);

  const images = $derived(picked.filter((f) => f.kind === 'image'));
  // Photos, clips and GIFs can go into a video.
  const visual = $derived(picked.filter((f) => f.kind !== 'audio'));

  async function paths(items: FileItem[]): Promise<string[]> {
    return Promise.all(items.map((f) => call<string>('resolve', { libraryId: f.libraryId, path: f.path })));
  }

  async function use(target: 'collage' | 'book' | 'calendar' | 'convert' | 'compress' | 'batch') {
    if (!images.length || busy) return;
    busy = true;
    try {
      const files = await paths(images);
      if (target === 'collage') await newQuickCollage(files);
      else if (target === 'book' || target === 'calendar') {
        const imported = await library.import(files);
        if (!imported.length) return;
        app.newProjectPhotos = imported;
        app.newProject = target;
      } else {
        const q = { convert: convertQueue, compress: compressQueue, batch: toolsQueue }[target];
        await q.add(files);
        app.view = target;
      }
      clear();
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not use these photos', message: String(e instanceof Error ? e.message : e) });
    } finally {
      busy = false;
    }
  }
</script>

<div class="lt library-view">
  <LibraryPanel bind:this={panel} onpick={(items) => (picked = items)} onselect={(items) => (picked = items)} hint="Click or Ctrl/Shift+click to select · then make a collage, photo book or calendar, or convert · double-click: preview" />
  {#if picked.length}
    <div class="use card">
      <span class="count">{picked.length} selected{images.length < picked.length ? ` (${images.length} photos)` : ''}</span>
      <button class="btn primary" disabled={busy || !images.length} onclick={() => void use('collage')}><LayoutDashboard size={15} /> Collage</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('book')}><BookOpen size={15} /> Photo book</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('calendar')}><CalendarDays size={15} /> Calendar</button>
      <button class="btn" disabled={busy || !visual.length} title="An MP4 slideshow of the selected photos and clips" onclick={() => (videoFiles = [...visual])}><Film size={15} /> Video</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('convert')}><ArrowLeftRight size={15} /> Convert</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('compress')}><Minimize2 size={15} /> Compress</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('batch')}><Wand size={15} /> Batch</button>
      <button class="icon-btn" title="Clear" onclick={clear}><X size={16} /></button>
    </div>
  {/if}
</div>

{#if videoFiles}
  <VideoDialog source={{ kind: 'files', files: videoFiles }} onclose={() => (videoFiles = null)} />
{/if}

<style>
  .library-view {
    height: 100%;
    display: flex;
    flex-direction: column;
    position: relative;
    background: var(--bg);
  }
  .library-view :global(.panel) {
    flex: 1;
    min-height: 0;
  }
  .use {
    position: absolute;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 12px;
    background: var(--panel-2);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow);
    z-index: 20;
  }
  .count {
    padding: 0 6px;
    color: var(--text-2);
    white-space: nowrap;
  }
</style>
