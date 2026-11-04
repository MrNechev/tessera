<script lang="ts">
  import { Search } from '@lucide/svelte';
  import { editor } from '../../../editor/editor.svelte';
  import { createSticker } from '../../../model/factory';
  import { stickers, type StickerInfo } from '../../../stores/stickers.svelte';

  let query = $state('');
  let category = $state('all');

  const list = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return stickers.list.filter(
      (s) =>
        (category === 'all' || s.category === category) &&
        (!q || s.name.toLowerCase().includes(q) || s.tags.some((t) => t.includes(q)) || s.category.includes(q)),
    );
  });

  function add(s: StickerInfo) {
    const size = editor.pageShort * 0.18;
    const aspect = s.width / s.height;
    const w = aspect >= 1 ? size : size * aspect;
    const h = aspect >= 1 ? size / aspect : size;
    // Cascade new stickers a little so repeated clicks don't stack exactly.
    const n = editor.doc.elements.filter((e) => e.kind === 'sticker').length % 6;
    const off = n * editor.pageShort * 0.02;
    editor.place(createSticker({ sticker: s.id, x: 0, y: 0, w, h }), {
      x: editor.doc.page.width / 2 + off,
      y: editor.doc.page.height / 2 + off,
    });
  }
</script>

<div class="head">
  <h3>Stickers</h3>
  <div class="search">
    <Search size={15} />
    <input type="search" placeholder="Search hearts, snow, balloons…" bind:value={query} />
  </div>
</div>
<div class="chips">
  <button class="chip" class:on={category === 'all'} onclick={() => (category = 'all')}>All</button>
  {#each stickers.categories as c (c.id)}
    <button class="chip" class:on={category === c.id} onclick={() => (category = c.id)}>{c.name}</button>
  {/each}
</div>
<div class="grid">
  {#each list as s (s.id)}
    <button class="st" onclick={() => add(s)} title={s.name}>
      <img src="/stickers/{s.file}" alt={s.name} loading="lazy" draggable="false" />
    </button>
  {:else}
    <p class="hint empty">{stickers.loaded ? 'No stickers match.' : 'Loading…'}</p>
  {/each}
</div>

<style>
  .head {
    padding: 16px 14px 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  h3 {
    font-size: 15px;
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
    border: 0;
    outline: none;
    background: transparent;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 2px 14px 12px;
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
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    padding: 0 14px 20px;
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
