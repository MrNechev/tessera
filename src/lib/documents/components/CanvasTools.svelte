<script lang="ts">
  // The canvas tool column (left): drawing tools with the shape picker, eraser modes,
  // stickers & icons, diagram templates / import, undo/redo, help.
  import { ink } from '../lib/canvas';
  import { setPref, TOOLS, type CanvasEditor, type Tool } from '../lib/canvasEditor.svelte';
  import { outlineExtra, outlinePath, shapeFields, SHAPES, outlineKind, type ShapeId } from '../lib/diagram/shapes';
  import { DIAGRAM_TEMPLATES } from '../lib/diagram/templates';
  import Icon from './Icon.svelte';

  let { ed }: { ed: CanvasEditor } = $props();
  let fileEl = $state<HTMLInputElement>();

  const tool = (id: Tool) => TOOLS.find((t) => t.id === id)!;
  const FIRST: Tool[] = ['select', 'hand'];
  const MAKE: Tool[] = ['card', 'note', 'text'];
  const LINK: Tool[] = ['arrow', 'line', 'frame'];
  const DRAW: Tool[] = ['pen', 'marker', 'eraser'];

  function kindOf(id: ShapeId) {
    const f = shapeFields(id);
    return outlineKind({ type: f.type, shape: f.shape });
  }
  function radiusOf(id: ShapeId, h: number) {
    return id === 'round' ? h / 4 : 2;
  }
  const label = (id: ShapeId) => SHAPES.find((s) => s.id === id)?.label ?? 'Shape';

  function onFile(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0];
    if (f) void ed.importDrawio(f);
    (e.currentTarget as HTMLInputElement).value = '';
  }
</script>

{#snippet mini(id: ShapeId, size: number)}
  {@const k = kindOf(id)}
  {@const w = size}
  {@const h = id === 'ellipse' || id === 'star' || id === 'triangle' ? size : Math.round(size * 0.72)}
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    <g transform="translate(0 {(size - h) / 2})">
      <path d={outlinePath(k, w, h, 1.8, radiusOf(id, h))} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
      {#if outlineExtra(k, w, h, 1.8)}<path d={outlineExtra(k, w, h, 1.8)} fill="none" stroke="currentColor" stroke-width="1.8" />{/if}
    </g>
  </svg>
{/snippet}

{#snippet btn(id: Tool)}
  {@const t = tool(id)}
  <button class="icon-btn" class:on={ed.tool === id} title="{t.label} ({t.key})" onclick={() => (ed.tool = id)}><Icon name={t.icon} /></button>
{/snippet}

<div class="tools">
  {#each FIRST as id (id)}{@render btn(id)}{/each}
  <span class="hsep"></span>
  {#each MAKE as id (id)}{@render btn(id)}{/each}
  <!-- the shape tool shows the current shape; the corner opens the picker -->
  <div class="pick">
    <button
      class="icon-btn"
      class:on={ed.tool === 'shape'}
      title="{label(ed.shapeKind)} (R rectangle · O ellipse · D last picked shape)"
      onclick={() => (ed.tool === 'shape' ? (ed.shapePicker = !ed.shapePicker) : (ed.tool = 'shape'))}>{@render mini(ed.shapeKind, 18)}</button
    >
    <button class="more" class:on={ed.shapePicker} title="More shapes" aria-label="More shapes" onclick={() => (ed.shapePicker = !ed.shapePicker)}></button>
    {#if ed.shapePicker}
      <div class="pop shapes">
        <div class="ph">Shapes</div>
        <div class="grid">
          {#each SHAPES as s (s.id)}
            <button class="cell" class:on={ed.shapeKind === s.id} title={s.label} onclick={() => ed.pickShape(s.id)}>{@render mini(s.id, 26)}</button>
          {/each}
        </div>
        <div class="hint">Draw by dragging, or click for the usual size. Double-click a shape to write in it.</div>
      </div>
    {/if}
  </div>
  {#each LINK as id (id)}{@render btn(id)}{/each}
  <span class="hsep"></span>
  {#each DRAW as id (id)}{@render btn(id)}{/each}
  <!-- not a drawing tool: opens the catalogue; a click (or a drag) puts a sticker on the canvas -->
  <button class="icon-btn" class:on={ed.stickerPicker} title="Stickers and icons (S)" onclick={() => (ed.stickerPicker = !ed.stickerPicker)}><Icon name="sticker" /></button>
  <div class="pick">
    <button class="icon-btn" class:on={ed.diagramMenu} title="Diagram templates and import (Mermaid, draw.io)" onclick={() => (ed.diagramMenu = !ed.diagramMenu)}
      ><Icon name="workflow" /></button
    >
    {#if ed.diagramMenu}
      <div class="pop menu">
        <div class="ph">Add a template</div>
        {#each DIAGRAM_TEMPLATES as t (t.id)}
          <button onclick={() => ed.insertTemplate(t.id)}>
            <Icon name={t.icon} size={15} />
            <span><b>{t.name}</b><small>{t.description}</small></span>
          </button>
        {/each}
        <div class="ph">Import</div>
        <button
          onclick={() => {
            ed.diagramMenu = false;
            ed.mermaidText = '';
          }}
        >
          <Icon name="code2" size={15} />
          <span><b>Mermaid flowchart…</b><small>Paste or type “flowchart TD …” (pasting it on the canvas works too).</small></span>
        </button>
        <button onclick={() => fileEl?.click()}>
          <Icon name="import" size={15} />
          <span><b>draw.io file…</b><small>A .drawio or .xml diagram from diagrams.net.</small></span>
        </button>
        <input bind:this={fileEl} type="file" accept=".drawio,.xml,.svg,application/xml,text/xml" hidden onchange={onFile} />
      </div>
    {/if}
  </div>
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
  {#if ed.tool === 'eraser'}
    <span class="hsep"></span>
    <button class="seg tseg" class:on={ed.eraserMode === 'stroke'} title="Erase whole strokes (E switches)" onclick={() => ed.setEraser('stroke')}>Whole</button>
    <button class="seg tseg" class:on={ed.eraserMode === 'split'} title="Erase only where you rub, cutting strokes apart (E switches)" onclick={() => ed.setEraser('split')}>Part</button>
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
    z-index: 6;
    max-height: calc(100% - 24px);
  }
  .tsw {
    margin: 2px auto;
  }
  .tseg {
    font-size: 11px;
    padding: 0 3px;
    min-width: 0;
  }
  .pick {
    position: relative;
  }
  .pick > .icon-btn {
    color: var(--text);
  }
  .more {
    position: absolute;
    right: 1px;
    bottom: 1px;
    width: 9px;
    height: 9px;
    padding: 0;
    border: none;
    background: linear-gradient(135deg, transparent 50%, var(--text-3) 50%);
    border-radius: 0 0 4px 0;
    cursor: pointer;
  }
  .more.on,
  .more:hover {
    background: linear-gradient(135deg, transparent 50%, var(--accent) 50%);
  }
  .pop {
    position: absolute;
    left: calc(100% + 12px);
    top: -8px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    padding: 8px;
    z-index: 8;
  }
  .ph {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    padding: 4px 4px 6px;
  }
  .shapes {
    width: 232px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }
  .cell {
    height: 46px;
    display: grid;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
  }
  .cell:hover {
    border-color: var(--line-strong);
  }
  .cell.on {
    border-color: var(--accent);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .hint {
    font-size: 11.5px;
    color: var(--text-3);
    padding: 8px 4px 2px;
    line-height: 1.35;
  }
  .menu {
    width: 300px;
    top: auto;
    bottom: -8px;
    display: flex;
    flex-direction: column;
  }
  .menu button {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 7px 8px;
    border: none;
    background: none;
    border-radius: 8px;
    text-align: left;
    cursor: pointer;
    color: var(--text);
  }
  .menu button :global(svg) {
    margin-top: 2px;
    flex: none;
  }
  .menu button:hover {
    background: var(--surface-2);
  }
  .menu span {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .menu b {
    font-weight: 600;
    font-size: 13px;
  }
  .menu small {
    font-size: 11.5px;
    color: var(--text-3);
    line-height: 1.3;
  }
</style>
