<script lang="ts">
  /**
   * The media library (from Letopis): every photo and video in the configured
   * libraries, with search, filters, duplicates and previews. Picked items can
   * start a collage or go to the image tools.
   */
  import { ArrowLeftRight, BookOpen, CalendarDays, Film, LayoutDashboard, Laugh, Minimize2, Scissors, SlidersHorizontal, Wand, X } from '@lucide/svelte';
  import LibraryPanel from '../documents/components/LibraryPanel.svelte';
  import VideoDialog from '../components/VideoDialog.svelte';
  import type { FileItem } from '../documents/lib/types';
  import { call } from '../api/http';
  import { memeFromPaths, newQuickCollage } from '../editor/session.svelte';
  import PhotoDialog, { type PhotoEdit } from '../documents/components/PhotoDialog.svelte';
  import { saveEditedCopy } from '../look/copy';
  import { deliver } from '../api/backend';
  import { library } from '../stores/library.svelte';
  import { app } from '../stores/app.svelte';
  import { compressQueue, convertQueue, toolsQueue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { palette } from '../documents/lib/palette.svelte';
  import { openCutout } from '../cutout/open.svelte';

  let picked = $state<FileItem[]>([]);
  let editing = $state<FileItem | null>(null);
  let saving = $state(false);

  async function saveCopy(f: FileItem, e: PhotoEdit) {
    saving = true;
    try {
      const [path] = await paths([f]);
      const out = await saveEditedCopy(path, e);
      editing = null;
      toasts.push({ kind: 'success', title: 'Edited copy saved', message: `Outputs/Edited/${out.split('/').pop()}` });
      deliver([out]);
    } catch (err) {
      toasts.push({ kind: 'error', title: 'Could not save the copy', message: String(err instanceof Error ? err.message : err) });
    } finally {
      saving = false;
    }
  }
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
        ...(images.length === 1 ? [{ id: 'lib:cutout', group: 'Selection', icon: 'image', title: 'Cut out the background', run: cutOut }] : []),
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

  /** Remove the background of the one selected photo; the PNG lands in Outputs/Cut-outs. */
  function cutOut() {
    const f = images[0];
    if (f) openCutout({ source: { libraryId: f.libraryId, path: f.path, name: f.name }, actions: [{ id: 'save', label: 'Save to Outputs' }] });
  }

  async function use(target: 'collage' | 'meme' | 'book' | 'calendar' | 'convert' | 'compress' | 'batch') {
    if (!images.length || busy) return;
    busy = true;
    try {
      const files = await paths(images);
      if (target === 'collage') await newQuickCollage(files);
      else if (target === 'meme') await memeFromPaths(files);
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

<div class="lt library-view" class:picking={picked.length > 0}>
  <LibraryPanel bind:this={panel} onpick={(items) => (picked = items)} onselect={(items) => (picked = items)} hint="Click a photo to select it · tick ✓ more (or Ctrl+click), Shift+click for a range · then make a collage, book, video… · double-click: preview" />
  {#if picked.length}
    <div class="use" role="toolbar" aria-label="Use the selected files">
      <span class="count"><b>{picked.length}</b> selected{images.length < picked.length ? ` · ${images.length} photo${images.length === 1 ? '' : 's'}` : ''}</span>
      <span class="div"></span>
      <button class="btn primary" disabled={busy || !images.length} onclick={() => void use('collage')}><LayoutDashboard size={15} /> Collage</button>
      {#if images.length === 1}
        <button class="btn" disabled={busy} onclick={() => (editing = images[0])} title="Crop, turn, filters, hide areas — saved as a new file in Outputs"><SlidersHorizontal size={15} /> Edit a copy</button>
        <button class="btn" disabled={busy} onclick={() => void use('meme')}><Laugh size={15} /> Meme</button>
      {/if}
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('book')}><BookOpen size={15} /> Photo book</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('calendar')}><CalendarDays size={15} /> Calendar</button>
      <button class="btn" disabled={busy || !visual.length} title="An MP4 slideshow of the selected photos and clips" onclick={() => (videoFiles = [...visual])}><Film size={15} /> Video</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('convert')}><ArrowLeftRight size={15} /> Convert</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('compress')}><Minimize2 size={15} /> Compress</button>
      <button class="btn" disabled={busy || !images.length} onclick={() => void use('batch')}><Wand size={15} /> Batch</button>
      {#if images.length === 1}<button class="btn" title="Remove the background (on this device); saved as a transparent PNG in Outputs › Cut-outs" disabled={busy} onclick={cutOut}><Scissors size={15} /> Cut out background</button>{/if}
      <span class="div"></span>
      <button class="btn ghost clear" title="Clear the selection (Esc)" onclick={clear}><X size={16} /> Clear</button>
    </div>
  {/if}
</div>

{#if editing}
  {@const f = editing}
  <PhotoDialog src="/d/{f.key}/t1280.webp" width={f.width} height={f.height} value={{}} mode="copy" saveLabel="Save copy" busy={saving} onclose={() => (editing = null)} onsave={(e) => void saveCopy(f, e)} />
{/if}
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
  /* leave room under the last row for the action bar */
  .library-view.picking :global(.grid) {
    padding-bottom: 96px;
  }
  .use {
    position: absolute;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    align-items: center;
    gap: 8px;
    width: max-content;
    max-width: calc(100% - 32px);
    padding: 10px 12px;
    border-radius: 16px;
    background: var(--panel);
    border: 2px solid var(--accent);
    box-shadow:
      0 18px 50px rgba(0, 0, 0, 0.45),
      0 0 0 6px color-mix(in srgb, var(--accent) 18%, transparent);
    z-index: 20;
    animation: rise 0.16s ease-out;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translate(-50%, 12px);
    }
  }
  .use :global(.btn) {
    height: 36px;
    padding: 0 12px;
    font-size: 13.5px;
  }
  .count {
    padding: 0 4px 0 6px;
    color: var(--text);
    white-space: nowrap;
    font-size: 14px;
  }
  .count b {
    display: inline-block;
    min-width: 26px;
    padding: 2px 8px;
    margin-right: 4px;
    border-radius: 12px;
    background: var(--accent);
    color: #fff;
    text-align: center;
  }
  .div {
    width: 1px;
    align-self: stretch;
    background: var(--border-strong);
  }
</style>
