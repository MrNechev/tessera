<script lang="ts" module>
  /** drag-and-drop type of a sticker dragged out of the grid (its id) */
  export const STICKER_MIME = 'application/x-tessera-sticker';
</script>

<script lang="ts">
  // The sticker catalogue with search and category chips. Shared by the collage editor's
  // Stickers panel and the story canvas' sticker picker; what a pick does is up to them.
  import { Search } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { stickers, type StickerInfo } from '../stores/stickers.svelte';

  let {
    onpick,
    columns = 4,
    draggable = false,
    autofocus = false,
  }: {
    onpick: (s: StickerInfo) => void;
    columns?: number;
    /** stickers can also be dragged out (as STICKER_MIME) */
    draggable?: boolean;
    autofocus?: boolean;
  } = $props();

  let query = $state('');
  let category = $state('all');
  let input = $state<HTMLInputElement>();

  onMount(() => {
    void stickers.load();
    if (autofocus) input?.focus();
  });

  const list = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return stickers.list.filter(
      (s) =>
        (category === 'all' || s.category === category) &&
        (!q || s.name.toLowerCase().includes(q) || s.tags.some((t) => t.includes(q)) || s.category.includes(q)),
    );
  });

  function dragStart(e: DragEvent, s: StickerInfo) {
    if (!draggable || !e.dataTransfer) return;
    e.dataTransfer.setData(STICKER_MIME, s.id);
    e.dataTransfer.effectAllowed = 'copy';
  }
</script>

<div class="sg">
  <div class="search">
    <Search size={15} />
    <input type="search" placeholder="Search hearts, snow, balloons…" bind:value={query} bind:this={input} />
  </div>
  <div class="chips">
    <button class="chip" class:on={category === 'all'} onclick={() => (category = 'all')}>All</button>
    {#each stickers.categories as c (c.id)}
      <button class="chip" class:on={category === c.id} onclick={() => (category = c.id)}>{c.name}</button>
    {/each}
  </div>
  <div class="grid" style:grid-template-columns="repeat({columns}, 1fr)">
    {#each list as s (s.id)}
      <button class="st" onclick={() => onpick(s)} title={s.name} draggable={draggable} ondragstart={(e) => dragStart(e, s)}>
        <img src="/stickers/{s.file}" alt={s.name} loading="lazy" draggable="false" />
      </button>
    {:else}
      <p class="hint empty">{stickers.loaded ? 'No stickers match.' : 'Loading…'}</p>
    {/each}
  </div>
</div>

<style>
  .sg {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    color: var(--text-3);
  }
  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--text);
    font: inherit;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .chip {
    height: 24px;
    padding: 0 9px;
    border-radius: 99px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-2);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .grid {
    display: grid;
    gap: 6px;
  }
  .st {
    aspect-ratio: 1;
    padding: 8px;
    border-radius: 9px;
    border: 1px solid transparent;
    background: var(--panel-2);
    cursor: pointer;
  }
  .st:hover {
    border-color: var(--accent);
    background: var(--panel-3);
  }
  .st img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .empty {
    grid-column: 1 / -1;
    text-align: center;
    padding: 20px 0;
  }
</style>
