<script lang="ts">
  // The canvas tool column (left): drawing tools, stickers, ink colours, undo/redo, help.
  import { ink } from '../lib/canvas';
  import { setPref, TOOLS, type CanvasEditor } from '../lib/canvasEditor.svelte';
  import Icon from './Icon.svelte';

  let { ed }: { ed: CanvasEditor } = $props();
</script>

<div class="tools">
  {#each TOOLS as t, i (t.id)}
    {#if i === 2 || i === 8}<span class="hsep"></span>{/if}
    <button class="icon-btn" class:on={ed.tool === t.id} title="{t.label} ({t.key})" onclick={() => (ed.tool = t.id)}><Icon name={t.icon} /></button>
  {/each}
  <!-- not a drawing tool: opens the catalogue; a click (or a drag) puts a sticker on the canvas -->
  <button class="icon-btn" class:on={ed.stickerPicker} title="Stickers (S)" onclick={() => (ed.stickerPicker = !ed.stickerPicker)}><Icon name="sticker" /></button>
  {#if ed.tool === 'pen' || ed.tool === 'marker'}
    <span class="hsep"></span>
    {#each ['red', 'yellow', 'green', 'blue', 'purple', 'ink'] as c (c)}
      <button
        class="sw tsw"
        class:on={ed.inkColor === c}
        style="background:{ink(c)}"
        title="Colour"
        aria-label="Ink colour"
        onclick={() => {
          ed.inkColor = c;
          setPref('ink', c);
        }}
      ></button>
    {/each}
  {/if}
  <span class="hsep"></span>
  <button class="icon-btn" title="Undo (Ctrl+Z)" onclick={() => ed.undo()}><Icon name="undo" /></button>
  <button class="icon-btn" title="Redo (Ctrl+Shift+Z)" onclick={() => ed.redo()}><Icon name="redo" /></button>
  <span class="hsep"></span>
  <button class="icon-btn" class:on={ed.help} title="Shortcuts (?)" onclick={() => (ed.help = !ed.help)}><Icon name="help" /></button>
</div>

<style>
  .tools {
    position: absolute;
    left: 12px;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 5;
  }
  .tsw {
    margin: 2px auto;
  }
</style>
