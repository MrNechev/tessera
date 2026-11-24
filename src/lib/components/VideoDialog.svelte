<script lang="ts">
  /**
   * Make an MP4 slideshow (Video out) from a library selection, a story or the
   * open collage. The engine does the encoding; this dialog collects the
   * options, draws title cards / collage pages, and shows the job's progress.
   * The job keeps running when the dialog is closed (see video/runner).
   */
  import { onMount } from 'svelte';
  import { Film, Music, X } from '@lucide/svelte';
  import Modal from './ui/Modal.svelte';
  import Segmented from './ui/Segmented.svelte';
  import Slider from './ui/Slider.svelte';
  import Toggle from './ui/Toggle.svelte';
  import { api as libApi, fmtDuration } from '../documents/lib/api';
  import type { FileItem } from '../documents/lib/types';
  import { editor } from '../editor/editor.svelte';
  import { videoRunner } from '../video/runner.svelte';
  import {
    itemsFromCollage,
    itemsFromFiles,
    itemsFromStory,
    storyMediaCount,
    titleCard,
    videoApi,
    type FileRef,
    type VideoFraming,
    type VideoItem,
    type VideoSource,
    type VideoSize,
    type VideoSupport,
    type VideoTransition,
  } from '../video/video';

  let { source, onclose }: { source: VideoSource; onclose: () => void } = $props();

  const today = new Date().toISOString().slice(0, 10);
  // The source is fixed while the dialog is open; read it once for the defaults.
  const initial = (() => source)();
  let name = $state(initial.kind === 'story' ? initial.doc.title || 'Story' : initial.kind === 'collage' ? editor.doc.name : `Slideshow ${today}`);
  let size = $state<VideoSize>('landscape');
  let seconds = $state(initial.kind === 'story' && initial.doc.settings?.autoAdvance ? initial.doc.settings.autoAdvance : 4);
  let transition = $state<VideoTransition>('crossfade');
  let framing = $state<VideoFraming>('auto');
  // Collage pages are full of detail (text, small photos): still by default.
  let kenBurns = $state(initial.kind !== 'collage');
  let titleCards = $state(true);
  /** Library selections and collages: an optional opening title. */
  let openingTitle = $state('');
  let pages = $state<'all' | 'current'>('all');
  let music = $state<(FileRef & { label: string }) | null>(
    initial.kind === 'story' && initial.doc.settings?.music
      ? { ...initial.doc.settings.music, label: initial.doc.settings.music.path }
      : null,
  );
  let musicQ = $state('');
  let musicHits = $state<FileItem[]>([]);
  let musicTimer: ReturnType<typeof setTimeout> | undefined;
  let support = $state<VideoSupport | null>(null);

  const count = $derived(
    source.kind === 'files'
      ? itemsFromFiles(source.files).length
      : source.kind === 'story'
        ? storyMediaCount(source.doc)
        : pages === 'all'
          ? editor.pageCount
          : 1,
  );
  const job = $derived(videoRunner.status);
  const running = $derived(videoRunner.busy);

  onMount(() => {
    videoApi
      .support()
      .then((s) => (support = s))
      .catch((e) => (support = { available: false, gpu: false, message: String(e instanceof Error ? e.message : e) }));
    return () => clearTimeout(musicTimer);
  });

  $effect(() => {
    const q = musicQ.trim();
    clearTimeout(musicTimer);
    musicTimer = setTimeout(() => {
      libApi
        .files({ kind: 'audio', q, limit: 20, sort: 'name' })
        .then((r) => (musicHits = r.items))
        .catch(() => (musicHits = []));
    }, 200);
  });

  async function items(): Promise<VideoItem[]> {
    const opening = openingTitle.trim();
    const intro = titleCards && opening ? [await titleCard(size, opening)] : [];
    if (source.kind === 'files') return [...intro, ...itemsFromFiles(source.files)];
    if (source.kind === 'story') return itemsFromStory(source.doc, size, titleCards);
    const list = pages === 'all' ? Array.from({ length: editor.pageCount }, (_, i) => i) : [editor.activePage];
    const slides = await itemsFromCollage(editor.doc, list, size, kenBurns, (d, t) => {
      videoRunner.preparing = `Drawing page ${Math.min(d + 1, t)} of ${t}`;
    });
    // A page is a whole picture: it gets the full time per photo.
    return [...intro, ...slides.map((s) => ({ ...s, seconds }))];
  }

  function make() {
    void videoRunner.start(items, {
      size,
      seconds,
      transition,
      kenBurns,
      framing,
      music: music ? { libraryId: music.libraryId, path: music.path } : null,
      name: name.trim() || 'Slideshow',
    });
  }

  const sizes = [
    { value: 'landscape' as const, label: '16:9', title: 'Landscape, 1920×1080' },
    { value: 'vertical' as const, label: '9:16', title: 'Vertical, 1080×1920 (phones, stories, reels)' },
    { value: 'square' as const, label: '1:1', title: 'Square, 1080×1080' },
  ];
  const transitions = [
    { value: 'crossfade' as const, label: 'Crossfade' },
    { value: 'black' as const, label: 'Dip to black' },
    { value: 'none' as const, label: 'Cut' },
  ];
  const framings = [
    { value: 'auto' as const, label: 'Auto', title: 'Fill the frame when the shapes are close, otherwise fit over a blurred copy' },
    { value: 'fill' as const, label: 'Fill', title: 'Always fill the frame (crops the edges)' },
    { value: 'fit' as const, label: 'Fit', title: 'Always show the whole picture over a blurred copy' },
  ];
</script>

<Modal title="Make a video" width={500} {onclose}>
  <div class="col">
    {#if support && !support.available}
      <p class="notice">{support.message}</p>
    {/if}
    <p class="hint">
      <Film size={14} />
      {#if source.kind === 'collage'}
        {count} page{count === 1 ? '' : 's'}, one slide each.
      {:else}
        {count} photo{count === 1 ? '' : 's'} and clip{count === 1 ? '' : 's'}, in order{source.kind === 'story' ? ', with their crops and clip times' : ''}.
      {/if}
      Saved as MP4 in Outputs/Videos.
    </p>

    <label class="col tight"><span class="label">Name</span><input class="input" bind:value={name} /></label>

    <div class="col tight">
      <span class="label">Shape</span>
      <Segmented options={sizes} value={size} onchange={(v) => (size = v)} />
    </div>

    <Slider label="Seconds per photo" value={seconds} min={1} max={15} step={0.5} decimals={1} unit="s" onchange={(v) => (seconds = v)} />

    <div class="col tight">
      <span class="label">Transition</span>
      <Segmented options={transitions} value={transition} onchange={(v) => (transition = v)} />
    </div>

    <div class="col tight">
      <span class="label">Photos that don't match the shape</span>
      <Segmented options={framings} value={framing} onchange={(v) => (framing = v)} />
    </div>

    <Toggle checked={kenBurns} label="Slow zoom and pan" hint="Ken Burns: each photo moves a little, the same way every time" onchange={(v) => (kenBurns = v)} />

    {#if source.kind === 'story'}
      <Toggle checked={titleCards} label="Title cards" hint="The story's title and each section's title on a card of its own" onchange={(v) => (titleCards = v)} />
    {:else}
      <Toggle checked={titleCards} label="Opening title card" onchange={(v) => (titleCards = v)} />
      {#if titleCards}
        <input class="input" placeholder="Title (leave empty for none)" bind:value={openingTitle} />
      {/if}
    {/if}

    {#if source.kind === 'collage' && editor.pageCount > 1}
      <label class="col tight">
        <span class="label">Pages</span>
        <select class="select" bind:value={pages}>
          <option value="all">All {editor.pageCount} pages</option>
          <option value="current">Only page {editor.activePage + 1}</option>
        </select>
      </label>
    {/if}

    <div class="col tight">
      <span class="label">Music</span>
      {#if music}
        <div class="music">
          <Music size={14} />
          <span class="truncate grow" title={music.label}>{music.label}</span>
          <button class="icon-btn" title="No music" onclick={() => (music = null)}><X size={15} /></button>
        </div>
        <span class="hint">Loops if it is shorter than the video and fades out at the end.</span>
      {:else}
        <input class="input" placeholder="Search audio files in the library…" bind:value={musicQ} />
        <div class="hits">
          {#each musicHits as f (f.id)}
            <button class="hit" onclick={() => (music = { libraryId: f.libraryId, path: f.path, label: f.path })}>
              <Music size={13} /> <span class="truncate grow">{f.path}</span> <span class="muted">{fmtDuration(f.duration)}</span>
            </button>
          {:else}
            <span class="hint">No audio files found{musicQ ? ' for this search' : ' in the libraries'}.</span>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#snippet footer()}
    {#if running}
      <div class="progress grow">
        <div class="bar"><div style:width="{Math.round((job?.progress ?? 0) * 100)}%"></div></div>
        <span class="hint">{videoRunner.preparing ?? job?.stage ?? ''}{job?.encoder ? ` · ${job.encoder}` : ''}</span>
      </div>
      <button class="btn" onclick={onclose} title="The video keeps being made; a message appears when it's ready">Hide</button>
      <button class="btn danger" disabled={!job} onclick={() => videoRunner.cancel()}>Stop</button>
    {:else}
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={!support?.available || !count} onclick={make}><Film size={15} /> Make video</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .tight {
    gap: 4px;
  }
  .hint {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .notice {
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--panel-2);
    color: var(--text);
    font-size: 13px;
  }
  .music {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    min-width: 0;
  }
  .hits {
    display: flex;
    flex-direction: column;
    max-height: 132px;
    overflow: auto;
  }
  .hit {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    padding: 4px 6px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
  }
  .hit:hover {
    background: var(--hover);
  }
  .muted {
    color: var(--text-2);
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-right: auto;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--bg-2);
    overflow: hidden;
  }
  .bar > div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
</style>
