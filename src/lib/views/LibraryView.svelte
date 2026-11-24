<script lang="ts">
  /**
   * The media library (from Letopis): every photo and video in the configured
   * libraries, with search, filters, duplicates and previews. Picked items can
   * start a collage or go to the image tools.
   */
  import { ArrowLeftRight, LayoutDashboard, Minimize2, Wand, X } from '@lucide/svelte';
  import LibraryPanel from '../documents/components/LibraryPanel.svelte';
  import type { FileItem } from '../documents/lib/types';
  import { call } from '../api/http';
  import { newQuickCollage } from '../editor/session.svelte';
  import { app } from '../stores/app.svelte';
  import { compressQueue, convertQueue, toolsQueue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { palette } from '../documents/lib/palette.svelte';

  let picked = $state<FileItem[]>([]);

  // Ctrl+K: actions on the selection.
  $effect(() =>
    palette.register('library', () => {
      if (!images.length) return [];
      const n = `${images.length} photo${images.length === 1 ? '' : 's'}`;
      return [
        { id: 'lib:collage', group: 'Selection', icon: 'layout', title: `Make a collage from ${n}`, run: () => use('collage') },
        { id: 'lib:convert', group: 'Selection', icon: 'refresh', title: `Convert ${n}`, run: () => use('convert') },
        { id: 'lib:compress', group: 'Selection', icon: 'package', title: `Compress ${n}`, run: () => use('compress') },
        { id: 'lib:batch', group: 'Selection', icon: 'sliders', title: `Resize, watermark or rename ${n}`, run: () => use('batch') },
      ];
    }),
  );
  let busy = $state(false);

  const images = $derived(picked.filter((f) => f.kind === 'image'));

  async function paths(items: FileItem[]): Promise<string[]> {
    return Promise.all(items.map((f) => call<string>('resolve', { libraryId: f.libraryId, path: f.path })));
  }

  async function use(target: 'collage' | 'convert' | 'compress' | 'batch') {
    if (!images.length || busy) return;
    busy = true;
    try {
      const files = await paths(images);
      if (target === 'collage') await newQuickCollage(files);
      else {
        const q = { convert: convertQueue, compress: compressQueue, batch: toolsQueue }[target];
        await q.add(files);
        app.view = target;
      }
      picked = [];
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not use these photos', message: String(e instanceof Error ? e.message : e) });
    } finally {
      busy = false;
    }
  }
</script>

<div class="lt library-view">
  <LibraryPanel onpick={(items) => (picked = items)} />
  {#if picked.length}
    <div class="use card">
      <span class="count">{picked.length} selected{images.length < picked.length ? ` (${images.length} photos)` : ''}</span>
      <button class="btn primary" disabled={busy || !images.length} onclick={() => void use('collage')}><LayoutDashboard size={15} /> Make a collage</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('convert')}><ArrowLeftRight size={15} /> Convert</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('compress')}><Minimize2 size={15} /> Compress</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('batch')}><Wand size={15} /> Batch</button>
      <button class="icon-btn" title="Clear" onclick={() => (picked = [])}><X size={16} /></button>
    </div>
  {/if}
</div>

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
