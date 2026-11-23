<script lang="ts">
  import Sortable from 'sortablejs';
  import { onMount } from 'svelte';
  import type { DocState } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { toHTML } from '../lib/tiptap';
  import type { Section } from '../lib/types';
  import Icon from './Icon.svelte';

  let { ds, onjump }: { ds: DocState; onjump: (id: string) => void } = $props();
  let list = $state<HTMLDivElement>();

  function label(s: Section, i: number) {
    if (s.title) return s.title;
    const t = toHTML(s.text).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    if (t) return t.slice(0, 60);
    if (s.media.length) return s.media[0].path.split('/').pop()!;
    return `Section ${i + 1}`;
  }

  function thumb(s: Section) {
    const m = s.media[0];
    if (!m) return '';
    const r = resolver.cache.get(m.libraryId + '\u0000' + m.path);
    return r?.found ? `${r.derived}t320.webp` : '';
  }

  onMount(() => {
    let next: Node | null = null;
    const sortable = Sortable.create(list!, {
      animation: 150,
      draggable: '.oi',
      ghostClass: 'ghost',
      onStart: (e) => (next = e.item.nextSibling),
      onEnd: (e) => {
        // let Svelte own the DOM: undo Sortable's move, then reorder the data
        e.from.insertBefore(e.item, next);
        if (e.oldDraggableIndex !== undefined && e.newDraggableIndex !== undefined)
          ds.moveSection(e.oldDraggableIndex, e.newDraggableIndex);
      },
    });
    return () => sortable.destroy();
  });
</script>

<div class="outline">
  <div class="head">Outline <span class="muted">· drag to reorder</span></div>
  <div class="list" bind:this={list}>
    {#each ds.doc?.sections ?? [] as s, i (s.id)}
      {@const t = thumb(s)}
      <button class="oi" class:active={ds.activeSection === s.id} class:noslide={!s.slide} onclick={() => onjump(s.id)}>
        <span class="n">{i + 1}</span>
        {#if t}<img src={t} alt="" loading="lazy" />{:else}<span class="noimg"><Icon name={s.layout === 'text-only' ? 'notes' : 'image'} size={13} /></span>{/if}
        <span class="l">{label(s, i)}</span>
        {#if !s.slide}<span class="hid" title="Not in the presentation"><Icon name="eye-off" size={12} /></span>{/if}
      </button>
    {/each}
  </div>
  <button class="btn sm add" onclick={() => ds.addSection(null)}><Icon name="plus" size={14} /> Section</button>
</div>

<style>
  .outline {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-2);
  }
  .head {
    padding: 12px 12px 6px;
    font-weight: 600;
    font-size: 12.5px;
  }
  .list {
    flex: 1;
    overflow: auto;
    padding: 0 6px;
  }
  .oi {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border: none;
    background: none;
    border-radius: 7px;
    cursor: grab;
    text-align: left;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .oi:hover {
    background: var(--surface-2);
  }
  .oi.active {
    background: var(--accent-soft);
    color: var(--text);
  }
  .oi.noslide {
    opacity: 0.6;
  }
  .n {
    font: 11px var(--mono);
    color: var(--text-3);
    min-width: 16px;
  }
  img,
  .noimg {
    width: 34px;
    height: 24px;
    object-fit: cover;
    border-radius: 4px;
    flex: none;
    background: var(--surface-3);
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .l {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hid {
    color: var(--text-3);
    display: grid;
  }
  :global(.oi.ghost) {
    opacity: 0.4;
    background: var(--accent-soft);
  }
  .add {
    margin: 8px 12px 12px;
    justify-content: center;
  }
</style>
