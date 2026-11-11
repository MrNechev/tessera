<script lang="ts">
  import { ArrowLeft, ArrowRight, Copy, Plus, Trash2 } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { onFontsLoaded } from '../../assets/fonts';
  import { editor } from '../../editor/editor.svelte';
  import { allPages, pageDoc } from '../../model/multipage';
  import type { PageContent } from '../../model/types';
  import { AdjustCache } from '../../render/adjust';
  import { renderDocument, type RenderResources } from '../../render/renderer';
  import { library } from '../../stores/library.svelte';
  import { stickers } from '../../stores/stickers.svelte';

  const HEIGHT = 64;
  /** Set while rendering when a photo or sticker isn't decoded yet. */
  let missing = false;
  const resources: RenderResources = {
    image: (id, need) => {
      const e = library.entry(id, need);
      if (!e && library.has(id)) missing = true;
      return e;
    },
    sticker: (id) => {
      const img = stickers.image(id);
      if (!img) missing = true;
      return img;
    },
    adjustCache: new AdjustCache(32),
  };

  // Unchanged pages keep the same objects (immutable updates), so thumbnails
  // are cached by the identity of their element list, background and border.
  // A thumbnail drawn while images were still loading is redrawn when they
  // arrive; complete ones are kept.
  interface Thumb {
    bg: object;
    frame: object | null;
    size: string;
    url: string;
    complete: boolean;
    gen: number;
  }
  let cache = new WeakMap<object, Thumb>();
  let gen = 0;
  let urls = $state<string[]>([]);
  let fontTick = $state(0);
  let timer: ReturnType<typeof setTimeout> | null = null;

  const pages = $derived(allPages(editor.doc));
  const aspect = $derived(editor.doc.page.width / editor.doc.page.height);

  function thumb(p: PageContent, i: number): string {
    const size = `${editor.doc.page.width}x${editor.doc.page.height}`;
    const hit = cache.get(p.elements);
    if (hit && hit.bg === p.background && hit.frame === p.frame && hit.size === size && (hit.complete || hit.gen === gen)) return hit.url;
    const doc = pageDoc(editor.doc, i);
    const s = (HEIGHT * 2) / doc.page.height;
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(doc.page.width * s));
    canvas.height = Math.max(1, Math.round(doc.page.height * s));
    missing = false;
    renderDocument(canvas.getContext('2d')!, doc, { scale: s, resources, placeholders: true });
    const url = canvas.toDataURL('image/jpeg', 0.75);
    cache.set(p.elements, { bg: p.background, frame: p.frame, size, url, complete: !missing, gen });
    return url;
  }

  $effect(() => {
    const list = pages;
    void library.version;
    void stickers.version;
    void fontTick;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      // Something finished loading since the last pass: incomplete thumbnails are stale.
      gen++;
      urls = list.map((p, i) => thumb(p, i));
    }, 250);
    return () => {
      if (timer) clearTimeout(timer);
    };
  });

  onMount(() =>
    onFontsLoaded(() => {
      cache = new WeakMap();
      fontTick++;
    }),
  );
</script>

<div class="strip" role="toolbar" aria-label="Pages">
  {#each pages as p, i (p.id)}
    <div class="page" class:on={i === editor.activePage}>
      <button
        class="thumb"
        style:aspect-ratio="{aspect}"
        style:height="{HEIGHT}px"
        title="Page {i + 1}"
        onclick={() => editor.goToPage(i)}
      >
        {#if urls[i]}<img src={urls[i]} alt="Page {i + 1}" draggable="false" />{/if}
      </button>
      {#if i !== editor.activePage || pages.length === 1}<span class="num">{i + 1}</span>{/if}
      {#if i === editor.activePage && pages.length > 1}
        <div class="tools">
          <button class="icon-btn small" title="Move left" disabled={i === 0} onclick={() => editor.movePage(i, i - 1)}><ArrowLeft size={12} /></button>
          <button class="icon-btn small" title="Duplicate page" onclick={() => editor.duplicatePage(i)}><Copy size={12} /></button>
          <button class="icon-btn small danger" title="Delete page" onclick={() => editor.removePage(i)}><Trash2 size={12} /></button>
          <button class="icon-btn small" title="Move right" disabled={i === pages.length - 1} onclick={() => editor.movePage(i, i + 1)}><ArrowRight size={12} /></button>
        </div>
      {/if}
    </div>
  {/each}
  <div class="page">
    <button class="thumb add" style:aspect-ratio="{aspect}" style:height="{HEIGHT}px" title="Add a page" onclick={() => editor.addPage()}>
      <Plus size={18} />
    </button>
    <span class="num">{pages.length > 1 ? 'Add' : 'Add page'}</span>
  </div>
</div>

<style>
  .strip {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 10px 14px 6px;
    overflow-x: auto;
    background: var(--panel);
    border-top: 1px solid var(--border);
    flex: none;
  }
  .page {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    flex: none;
  }
  .thumb {
    padding: 0;
    border: 2px solid transparent;
    border-radius: 5px;
    background: var(--panel-3);
    overflow: hidden;
    cursor: pointer;
    box-shadow: var(--shadow-sm);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    display: block;
    object-fit: cover;
  }
  .page.on .thumb {
    border-color: var(--accent);
  }
  .thumb.add {
    display: grid;
    place-items: center;
    color: var(--text-3);
    border: 1.5px dashed var(--border-strong);
    background: transparent;
    box-shadow: none;
    min-width: 40px;
  }
  .thumb.add:hover {
    color: var(--accent);
    border-color: var(--accent);
  }
  .num {
    font-size: 10.5px;
    color: var(--text-3);
    font-weight: 600;
  }
  .page.on .num {
    color: var(--accent);
  }
  .tools {
    display: flex;
    gap: 0;
    margin-top: -1px;
  }
  .tools :global(.icon-btn.small) {
    width: 20px;
    height: 18px;
  }
  .danger:hover {
    color: var(--danger);
  }
</style>
