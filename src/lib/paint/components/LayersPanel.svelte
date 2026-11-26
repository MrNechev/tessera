<script lang="ts">
  /**
   * Layers, top first: thumbnails, show/hide, lock, rename (double-click),
   * drag to reorder; opacity and blend mode of the current layer; add,
   * duplicate, delete, merge down, flatten.
   */
  import { ArrowDownToLine, Copy, Eye, EyeOff, Layers2, Lock, LockOpen, Plus, Trash2, Type } from '@lucide/svelte';
  import type { BlendMode } from '../lib/ora';
  import { paint } from '../state.svelte';
  import { beginTextEdit } from '../tools.svelte';
  import { setTool } from '../actions.svelte';
  import LayerThumb from './LayerThumb.svelte';

  const d = $derived(paint.doc!);
  const top = $derived([...d.layers].reverse());
  const act = $derived(d.active);
  let renaming = $state<string | null>(null);
  let drag = $state<{ id: string; startY: number; over: number; moved: boolean } | null>(null);
  let list: HTMLDivElement | undefined = $state();

  const BLENDS: [BlendMode, string][] = [
    ['source-over', 'Normal'],
    ['multiply', 'Multiply'],
    ['screen', 'Screen'],
    ['overlay', 'Overlay'],
    ['darken', 'Darken'],
    ['lighten', 'Lighten'],
    ['color-dodge', 'Colour dodge'],
    ['color-burn', 'Colour burn'],
    ['hard-light', 'Hard light'],
    ['soft-light', 'Soft light'],
    ['difference', 'Difference'],
    ['exclusion', 'Exclusion'],
    ['hue', 'Hue'],
    ['saturation', 'Saturation'],
    ['color', 'Colour'],
    ['luminosity', 'Luminosity'],
  ];

  function rowIndexAt(y: number): number {
    const rows = [...(list?.querySelectorAll<HTMLElement>('.lrow') ?? [])];
    for (let i = 0; i < rows.length; i++) {
      const r = rows[i].getBoundingClientRect();
      if (y < r.top + r.height / 2) return i;
    }
    return rows.length - 1;
  }

  function down(e: PointerEvent, id: string) {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, input')) return;
    d.activeId = id;
    drag = { id, startY: e.clientY, over: top.findIndex((l) => l.id === id), moved: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }
  function move(e: PointerEvent) {
    if (!drag) return;
    if (!drag.moved && Math.abs(e.clientY - drag.startY) < 5) return;
    drag.moved = true;
    drag.over = rowIndexAt(e.clientY);
  }
  function up() {
    const g = drag;
    drag = null;
    if (!g?.moved) return;
    // rows are top first; the stack is bottom first
    d.moveLayer(g.id, d.layers.length - 1 - g.over);
  }

  function rename(id: string, name: string) {
    renaming = null;
    const n = name.trim();
    if (n && n !== d.layer(id)?.name) d.setLayerProps(id, { name: n }, 'Rename layer');
  }
</script>

<div class="layers">
  <div class="list" bind:this={list} role="listbox" aria-label="Layers">
    {#each top as l, i (l.id)}
      <div
        class="lrow"
        class:on={l.id === act?.id}
        class:dragging={drag?.moved && drag.id === l.id}
        class:drop-above={drag?.moved && drag.over === i && drag.id !== l.id && top.findIndex((x) => x.id === drag!.id) > i}
        class:drop-below={drag?.moved && drag.over === i && drag.id !== l.id && top.findIndex((x) => x.id === drag!.id) < i}
        role="option"
        aria-selected={l.id === act?.id}
        tabindex="-1"
        onpointerdown={(e) => down(e, l.id)}
        onpointermove={move}
        onpointerup={up}
        ondblclick={() => {
          if (l.text) {
            setTool('text');
            beginTextEdit(l.id);
          } else renaming = l.id;
        }}
      >
        <button class="ib" title={l.visible ? 'Hide' : 'Show'} aria-label={l.visible ? 'Hide layer' : 'Show layer'} onclick={() => d.setLayerProps(l.id, { visible: !l.visible }, l.visible ? 'Hide layer' : 'Show layer')}>
          {#if l.visible}<Eye size={14} />{:else}<EyeOff size={14} />{/if}
        </button>
        <LayerThumb doc={d} id={l.id} rev={l.rev} />
        <div class="name">
          {#if renaming === l.id}
            <!-- svelte-ignore a11y_autofocus -->
            <input class="input" value={l.name} autofocus onkeydown={(e) => { if (e.key === 'Enter') rename(l.id, e.currentTarget.value); if (e.key === 'Escape') renaming = null; }} onblur={(e) => rename(l.id, e.currentTarget.value)} />
          {:else}
            <span class="truncate" title="Double-click to rename">{l.name}</span>
            <span class="meta">{#if l.text}<Type size={11} /> text · {/if}{l.opacity < 1 ? `${Math.round(l.opacity * 100)}%` : ''}{l.blend !== 'source-over' ? ` ${BLENDS.find((b) => b[0] === l.blend)?.[1] ?? ''}` : ''}</span>
          {/if}
        </div>
        <button class="ib" class:lit={l.locked} title={l.locked ? 'Unlock' : 'Lock'} aria-label={l.locked ? 'Unlock layer' : 'Lock layer'} onclick={() => d.setLayerProps(l.id, { locked: !l.locked }, l.locked ? 'Unlock layer' : 'Lock layer')}>
          {#if l.locked}<Lock size={13} />{:else}<LockOpen size={13} />{/if}
        </button>
      </div>
    {/each}
  </div>

  {#if act}
    <div class="props">
      <label class="op">
        <span>Opacity</span>
        <input type="range" min="0" max="1" step="0.01" value={act.opacity} style:--fill="{act.opacity * 100}%" oninput={(e) => d.setLayerProps(act.id, { opacity: +e.currentTarget.value }, 'Layer opacity')} />
        <span class="v">{Math.round(act.opacity * 100)}%</span>
      </label>
      <select class="select" aria-label="Blend mode" value={act.blend} onchange={(e) => d.setLayerProps(act.id, { blend: e.currentTarget.value as BlendMode }, 'Blend mode')}>
        {#each BLENDS as [id, label] (id)}<option value={id}>{label}</option>{/each}
      </select>
    </div>
  {/if}

  <div class="bar">
    <button class="icon-btn small" title="New layer" aria-label="New layer" onclick={() => d.addLayer()}><Plus size={15} /></button>
    <button class="icon-btn small" title="Duplicate layer" aria-label="Duplicate layer" onclick={() => d.duplicateLayer()}><Copy size={14} /></button>
    <button class="icon-btn small" title="Merge down" aria-label="Merge down" disabled={!act || d.index(act.id) <= 0} onclick={() => d.mergeDown()}><ArrowDownToLine size={14} /></button>
    <button class="icon-btn small" title="Flatten image" aria-label="Flatten image" disabled={d.layers.length < 2} onclick={() => d.flatten()}><Layers2 size={14} /></button>
    <span class="spacer"></span>
    <button class="icon-btn small" title="Delete layer" aria-label="Delete layer" disabled={d.layers.length < 2} onclick={() => d.deleteLayer()}><Trash2 size={14} /></button>
  </div>
</div>

<style>
  .layers {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    max-height: 320px;
    min-height: 44px;
  }
  .lrow {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 4px;
    border-radius: 8px;
    border: 1px solid transparent;
    cursor: pointer;
    touch-action: none;
  }
  .lrow:hover {
    background: var(--hover);
  }
  .lrow.on {
    background: var(--accent-soft);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .lrow.dragging {
    opacity: 0.5;
  }
  .lrow.drop-above {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .lrow.drop-below {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name .input {
    height: 24px;
    padding: 0 6px;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10.5px;
    color: var(--text-3);
  }
  .ib {
    width: 24px;
    height: 24px;
    flex: none;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .ib:hover {
    background: var(--hover);
    color: var(--text);
  }
  .ib.lit {
    color: var(--warning);
  }
  .props {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .op {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .op input {
    flex: 1;
    min-width: 0;
  }
  .v {
    width: 36px;
    text-align: right;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .select {
    height: 30px;
  }
  .bar {
    display: flex;
    gap: 2px;
    align-items: center;
  }
</style>
