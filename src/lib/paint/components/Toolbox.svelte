<script lang="ts">
  /** Paint's tool column with the primary/secondary colour swatches at the bottom. */
  import {
    ArrowLeftRight,
    Brush,
    Crop,
    Droplets,
    Eraser,
    Hand,
    Lasso,
    Move,
    PaintBucket,
    Pencil,
    Pipette,
    Shapes,
    SquareDashed,
    Stamp,
    Type,
    WandSparkles,
  } from '@lucide/svelte';
  import { setTool } from '../actions.svelte';
  import { paint, type ToolId } from '../state.svelte';

  const TOOLS: { id: ToolId; label: string; key: string; icon: typeof Brush }[][] = [
    [
      { id: 'select', label: 'Select', key: 'M', icon: SquareDashed },
      { id: 'lasso', label: 'Lasso', key: 'L', icon: Lasso },
      { id: 'wand', label: 'Magic wand', key: 'W', icon: WandSparkles },
      { id: 'move', label: 'Move', key: 'V', icon: Move },
    ],
    [
      { id: 'brush', label: 'Brush', key: 'B', icon: Brush },
      { id: 'pencil', label: 'Pencil', key: 'P', icon: Pencil },
      { id: 'eraser', label: 'Eraser', key: 'E', icon: Eraser },
      { id: 'fill', label: 'Fill', key: 'G', icon: PaintBucket },
      { id: 'picker', label: 'Eyedropper', key: 'I', icon: Pipette },
      { id: 'shape', label: 'Shapes', key: 'U', icon: Shapes },
      { id: 'text', label: 'Text', key: 'T', icon: Type },
    ],
    [
      { id: 'blur', label: 'Blur & smudge', key: 'R', icon: Droplets },
      { id: 'clone', label: 'Clone stamp', key: 'S', icon: Stamp },
      { id: 'crop', label: 'Crop', key: 'C', icon: Crop },
      { id: 'hand', label: 'Hand', key: 'H', icon: Hand },
    ],
  ];

  const on = (id: ToolId) => paint.tool === id || (id === 'blur' && paint.tool === 'smudge');
</script>

<div class="toolbox" role="toolbar" aria-label="Tools" aria-orientation="vertical">
  {#each TOOLS as group, gi (gi)}
    {#if gi}<span class="sep"></span>{/if}
    {#each group as t (t.id)}
      <button class="tb" class:on={on(t.id)} title="{t.label} ({t.key})" aria-label={t.label} aria-pressed={on(t.id)} data-tool={t.id} onclick={() => setTool(t.id === 'blur' && paint.tool === 'smudge' ? 'smudge' : t.id)}>
        <t.icon size={18} />
      </button>
    {/each}
  {/each}
  <span class="spacer"></span>
  <div class="swatches" title="Primary and secondary colour (X swaps, D resets)">
    <button class="sw secondary checker" aria-label="Secondary colour" onclick={() => paint.swap()}><span style:background={paint.secondary}></span></button>
    <button class="sw primary checker" aria-label="Primary colour" onclick={() => (document.querySelector('.paint-colors input.hex') as HTMLInputElement | null)?.focus()}><span style:background={paint.primary}></span></button>
    <button class="swap" title="Swap colours (X)" aria-label="Swap colours" onclick={() => paint.swap()}><ArrowLeftRight size={11} /></button>
  </div>
</div>

<style>
  .toolbox {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
    padding: 8px 6px;
    width: 52px;
    flex: none;
    background: var(--panel);
    border-right: 1px solid var(--border);
    overflow-y: auto;
    scrollbar-width: none;
  }
  .sep {
    width: 26px;
    height: 1px;
    background: var(--border);
    margin: 5px 0;
    flex: none;
  }
  .tb {
    width: 38px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .tb:hover {
    background: var(--hover);
    color: var(--text);
  }
  .tb.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .spacer {
    flex: 1;
    min-height: 10px;
  }
  .swatches {
    position: relative;
    width: 40px;
    height: 40px;
    flex: none;
    margin-bottom: 4px;
  }
  .sw {
    position: absolute;
    width: 26px;
    height: 26px;
    padding: 0;
    border-radius: 6px;
    border: 2px solid var(--panel);
    box-shadow: 0 0 0 1px var(--border-strong);
    overflow: hidden;
    cursor: pointer;
  }
  .sw span {
    display: block;
    width: 100%;
    height: 100%;
  }
  .primary {
    left: 0;
    top: 0;
    z-index: 1;
  }
  .secondary {
    right: 0;
    bottom: 0;
  }
  .swap {
    position: absolute;
    right: -2px;
    top: -3px;
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text-3);
    cursor: pointer;
  }
  .swap:hover {
    color: var(--text);
  }
</style>
