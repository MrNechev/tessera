<script lang="ts">
  import { FolderPlus, ImagePlus, LayoutGrid, LoaderCircle, Plus, Scissors, X } from '@lucide/svelte';
  import { fileUrl, pickFolder } from '../../../api/backend';
  import { editor } from '../../../editor/editor.svelte';
  import { addPhotos, importLibraryRefs } from '../../../editor/session.svelte';
  import LibraryPanel from '../../../documents/components/LibraryPanel.svelte';
  import type { FileItem } from '../../../documents/lib/types';
  import { app } from '../../../stores/app.svelte';
  import { drag } from '../../../stores/drag.svelte';
  import { library } from '../../../stores/library.svelte';
  import { cutOutImported } from '../../../cutout/collage';

  const used = $derived(editor.usedAssets());

  function setSource(v: 'library' | 'collage') {
    editor.photoSource = v;
    try {
      localStorage.setItem('tessera.photoSource', v);
    } catch {
      /* ignore */
    }
  }

  async function insertFromLibrary(items: FileItem[]) {
    const ids = await importLibraryRefs(items);
    if (ids.length) add(ids);
  }
  let selected = $state<string[]>([]);

  async function addFolder() {
    const dir = await pickFolder('Add a folder of photos', app.settings.recentImportDir);
    if (dir) {
      const imgs = await library.import([dir]);
      if (imgs.length && !editor.doc.elements.length) editor.addToLayout(imgs.map((i) => i.id));
    }
  }

  function add(ids: string[]) {
    editor.placePhotos(ids);
  }

  function onDown(e: PointerEvent, id: string, thumb: string) {
    if (e.button !== 0) return;
    const sx = e.clientX;
    const sy = e.clientY;
    const ids = selected.includes(id) ? [...selected] : [id];
    const move = (ev: PointerEvent) => {
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) > 6) {
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        drag.start(ids, thumb, ev.clientX, ev.clientY);
      }
    };
    const up = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      if (e.ctrlKey || e.metaKey || e.shiftKey) {
        selected = selected.includes(id) ? selected.filter((s) => s !== id) : [...selected, id];
      } else {
        selected = [id];
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
  }
</script>

<div class="tabs" role="tablist" aria-label="Photos from">
  <button role="tab" aria-selected={editor.photoSource === 'library'} class:on={editor.photoSource === 'library'} onclick={() => setSource('library')}>Library</button>
  <button role="tab" aria-selected={editor.photoSource === 'collage'} class:on={editor.photoSource === 'collage'} onclick={() => setSource('collage')}>
    In this collage{#if library.items.length}<span class="n">{library.items.length}</span>{/if}
  </button>
</div>

{#if editor.photoSource === 'library'}
  <div class="lt libwrap">
    <LibraryPanel
      paletteTarget={false}
      hint="Click to add · drag onto the page or a frame · Ctrl/Shift+click: select · double-click: preview"
      onpick={(items) => void insertFromLibrary(items)}
    />
    <p class="libhint"><button class="linkish" onclick={async () => {
      const ids = await addPhotos();
      if (ids.length) add(ids);
    }}>Add photos from {app.isServer ? 'the server or this computer' : 'other folders'}…</button></p>
  </div>
{:else}
<div class="wrap">
  <div class="head">
    <p class="hint">Drag photos onto the page or onto a frame. Drop files from your file manager anywhere.</p>
    <div class="row">
      <button class="btn primary grow" onclick={async () => {
        const ids = await addPhotos();
        // Fill the layout or the template's empty frames; otherwise the photos wait in the panel.
        const emptyFrames = editor.doc.elements.some((e) => e.kind === 'photo' && !e.asset && !e.hidden);
        if (ids.length && (editor.hasLayout || !editor.doc.elements.length || emptyFrames)) editor.placePhotos(ids);
      }}><ImagePlus size={16} /> Add photos</button>
      <button class="btn" title="Add a whole folder" onclick={addFolder}><FolderPlus size={16} /></button>
    </div>
    {#if library.importing}
      <div class="importing"><LoaderCircle size={14} class="spin" /> Importing photos… {library.importing.done} / {library.importing.total}</div>
    {/if}
  </div>

  {#if library.items.length}
    <div class="actions">
      <span class="hint">{library.items.length} photo{library.items.length > 1 ? 's' : ''} · {used.size} used</span>
      <span class="spacer"></span>
      {#if selected.length > 1}
        <button class="btn small" onclick={() => add(selected)}><Plus size={13} /> Add {selected.length}</button>
      {/if}
      <button
        class="btn small"
        title="Put every unused photo into the automatic layout"
        onclick={() => editor.addToLayout(library.items.filter((i) => !used.has(i.id)).map((i) => i.id))}
        disabled={library.items.every((i) => used.has(i.id))}><LayoutGrid size={13} /> Add all</button
      >
    </div>
    <div class="grid">
      {#each library.items as item (item.id)}
        <div
          class="item"
          class:sel={selected.includes(item.id)}
          role="button"
          tabindex="0"
          title="{item.name} · {item.width}×{item.height}"
          onpointerdown={(e) => onDown(e, item.id, fileUrl(item.thumb))}
          ondblclick={() => add([item.id])}
          onkeydown={(e) => e.key === 'Enter' && add([item.id])}
        >
          <img src={fileUrl(item.thumb)} alt={item.name} draggable="false" loading="lazy" />
          {#if used.has(item.id)}<span class="badge">✓</span>{/if}
          <button class="add" title="Add to collage" onpointerdown={(e) => e.stopPropagation()} onclick={() => add([item.id])}><Plus size={14} /></button>
          <button class="cut" title="Cut out the subject (remove the background)" onpointerdown={(e) => e.stopPropagation()} onclick={() => cutOutImported(item)}><Scissors size={13} /></button>
          <button
            class="remove"
            title="Remove from library"
            onpointerdown={(e) => e.stopPropagation()}
            onclick={() => {
              library.remove(item.id);
              selected = selected.filter((s) => s !== item.id);
            }}><X size={12} /></button
          >
        </div>
      {/each}
    </div>
  {:else if !library.importing}
    <div class="empty">
      <div class="art"><ImagePlus size={30} /></div>
      <p>No photos yet</p>
      <p class="hint">Add photos from any drive, or drop image files and folders onto the window.</p>
    </div>
  {/if}
</div>
{/if}

{#if drag.session}
  <div class="ghost" style:left="{drag.session.x}px" style:top="{drag.session.y}px">
    <img src={drag.session.thumb} alt="" />
    {#if drag.session.assetIds.length > 1}<span class="count">{drag.session.assetIds.length}</span>{/if}
  </div>
{/if}

<style>
  .tabs {
    display: flex;
    gap: 2px;
    margin: 12px 12px 8px;
    padding: 2px;
    border-radius: 9px;
    background: var(--panel-2);
    border: 1px solid var(--border);
  }
  .tabs button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 5px 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--text-2);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .tabs button.on {
    background: var(--panel-3);
    color: var(--text);
  }
  .n {
    padding: 0 6px;
    border-radius: 99px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
  }
  .libwrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .libwrap :global(.panel) {
    flex: 1;
    min-height: 0;
    background: transparent;
  }
  .libhint {
    margin: 0;
    padding: 8px 12px 10px;
    border-top: 1px solid var(--border);
    color: var(--text-3);
    font-size: 11.5px;
  }
  .linkish {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .wrap {
    display: flex;
    flex-direction: column;
  }
  .head {
    padding: 16px 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .importing {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-size: 12px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 14px 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
    padding: 0 14px 16px;
  }
  .item {
    position: relative;
    aspect-ratio: 1;
    border-radius: 8px;
    overflow: hidden;
    background: var(--panel-3);
    cursor: grab;
    outline: 2px solid transparent;
    outline-offset: 1px;
  }
  .item.sel {
    outline-color: var(--accent);
  }
  .item img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    pointer-events: none;
  }
  .badge {
    position: absolute;
    left: 5px;
    bottom: 5px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    display: grid;
    place-items: center;
    font-weight: 700;
  }
  .add,
  .cut,
  .remove {
    position: absolute;
    display: none;
    place-items: center;
    border: 0;
    border-radius: 50%;
    cursor: pointer;
    color: #fff;
  }
  .add {
    right: 5px;
    bottom: 5px;
    width: 24px;
    height: 24px;
    background: var(--accent);
  }
  .cut {
    left: 5px;
    bottom: 5px;
    width: 24px;
    height: 24px;
    background: rgba(0, 0, 0, 0.6);
  }
  .remove {
    right: 4px;
    top: 4px;
    width: 20px;
    height: 20px;
    background: rgba(0, 0, 0, 0.6);
  }
  .item:hover .add,
  .item:hover .cut,
  .item:hover .remove {
    display: grid;
  }
  .empty {
    margin: 30px 18px;
    padding: 28px 16px;
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    text-align: center;
    color: var(--text-2);
  }
  .empty p {
    margin: 6px 0;
  }
  .art {
    display: inline-grid;
    place-items: center;
    width: 56px;
    height: 56px;
    border-radius: 16px;
    background: var(--accent-soft);
    color: var(--accent);
    margin-bottom: 6px;
  }
  .ghost {
    position: fixed;
    z-index: 300;
    pointer-events: none;
    transform: translate(-30%, -30%) rotate(-4deg);
  }
  .ghost img {
    width: 92px;
    height: 92px;
    object-fit: cover;
    border-radius: 8px;
    border: 3px solid #fff;
    box-shadow: var(--shadow);
  }
  .count {
    position: absolute;
    top: -8px;
    right: -8px;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    border-radius: 11px;
    background: var(--accent);
    color: #fff;
    font-weight: 700;
    font-size: 12px;
    display: grid;
    place-items: center;
  }
</style>
