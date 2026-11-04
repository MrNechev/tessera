<script lang="ts">
  import { ArrowDown, ArrowUp, Eye, EyeOff, Image, Lock, LockOpen, Shapes, Sticker, Type } from '@lucide/svelte';
  import { editor } from '../../editor/editor.svelte';
  import type { CollageElement } from '../../model/types';
  import { stickers } from '../../stores/stickers.svelte';

  const icons = { photo: Image, text: Type, sticker: Sticker, shape: Shapes };
  const items = $derived([...editor.doc.elements].reverse());

  function label(e: CollageElement): string {
    if (e.name && !e.name.startsWith('__')) return e.name;
    switch (e.kind) {
      case 'photo':
        return e.asset ? (editor.doc.assets[e.asset]?.name ?? 'Photo') : 'Empty frame';
      case 'text':
        return e.text.split('\n')[0] || 'Text';
      case 'sticker':
        return stickers.info(e.sticker)?.name ?? 'Sticker';
      case 'shape':
        return `${e.shape[0].toUpperCase()}${e.shape.slice(1)}`;
    }
  }

  function move(e: CollageElement, dir: 1 | -1) {
    const i = editor.doc.elements.findIndex((x) => x.id === e.id);
    editor.moveTo(e.id, i + dir);
  }
</script>

<div class="layers">
  {#each items as e (e.id)}
    {@const Icon = icons[e.kind]}
    <div
      class="layer"
      class:sel={editor.selection.includes(e.id)}
      class:hidden={e.hidden}
      role="button"
      tabindex="0"
      onclick={(ev) => editor.select([e.id], ev.shiftKey || ev.ctrlKey)}
      onkeydown={(ev) => ev.key === 'Enter' && editor.select([e.id])}
    >
      <Icon size={14} />
      <span class="name truncate">{label(e)}</span>
      {#if e.kind === 'photo' && e.slot}<span class="pill">layout</span>{/if}
      <span class="tools">
        <button class="icon-btn small" title="Move up" onclick={(ev) => { ev.stopPropagation(); move(e, 1); }}><ArrowUp size={13} /></button>
        <button class="icon-btn small" title="Move down" onclick={(ev) => { ev.stopPropagation(); move(e, -1); }}><ArrowDown size={13} /></button>
      </span>
      <button class="icon-btn small" title={e.locked ? 'Unlock' : 'Lock'} onclick={(ev) => { ev.stopPropagation(); editor.patch(e.id, { locked: !e.locked }); }}>
        {#if e.locked}<Lock size={13} />{:else}<LockOpen size={13} />{/if}
      </button>
      <button class="icon-btn small" title={e.hidden ? 'Show' : 'Hide'} onclick={(ev) => { ev.stopPropagation(); editor.patch(e.id, { hidden: !e.hidden }); }}>
        {#if e.hidden}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
      </button>
    </div>
  {:else}
    <p class="hint empty">The page is empty.</p>
  {/each}
</div>

<style>
  .layers {
    display: flex;
    flex-direction: column;
    padding: 8px;
  }
  .layer {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 4px 0 10px;
    border-radius: 7px;
    cursor: pointer;
    color: var(--text-2);
  }
  .layer:hover {
    background: var(--hover);
  }
  .layer.sel {
    background: var(--accent-soft);
    color: var(--text);
  }
  .layer.hidden .name {
    opacity: 0.45;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: 12.5px;
  }
  .tools {
    display: none;
  }
  .layer:hover .tools {
    display: flex;
  }
  .empty {
    padding: 16px;
    text-align: center;
  }
</style>
