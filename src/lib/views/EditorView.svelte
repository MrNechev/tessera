<script lang="ts">
  import {
    ChevronDown,
    FilePlus,
    FolderOpen,
    Images,
    LayoutGrid,
    LayoutTemplate,
    Maximize,
    Minus,
    Paintbrush,
    Plus,
    Redo2,
    Save,
    Shapes,
    Smile,
    Sticker,
    Type,
    Undo2,
    Download,
    FileImage,
    Ruler,
    CalendarDays,
    Printer,
    Copy,
    Trash2,
  } from '@lucide/svelte';
  import EditorCanvas from '../components/editor/EditorCanvas.svelte';
  import Inspector from '../components/editor/Inspector.svelte';
  import ExportDialog from '../components/editor/ExportDialog.svelte';
  import PhotosPanel from '../components/editor/panels/PhotosPanel.svelte';
  import LayoutPanel from '../components/editor/panels/LayoutPanel.svelte';
  import TemplatesPanel from '../components/editor/panels/TemplatesPanel.svelte';
  import TextPanel from '../components/editor/panels/TextPanel.svelte';
  import StickersPanel from '../components/editor/panels/StickersPanel.svelte';
  import ShapesPanel from '../components/editor/panels/ShapesPanel.svelte';
  import BackgroundPanel from '../components/editor/panels/BackgroundPanel.svelte';
  import PagePanel from '../components/editor/panels/PagePanel.svelte';
  import CalendarPanel from '../components/editor/panels/CalendarPanel.svelte';
  import PageStrip from '../components/editor/PageStrip.svelte';
  import { onFontsLoaded } from '../assets/fonts';
  import { editor, type Tool } from '../editor/editor.svelte';
  import { addPhotos, newBlank, openProjectDialog, printProject, quickSave, saveProjectAs, saveState, scheduleAutosave } from '../editor/session.svelte';
  import { describePage } from '../model/pages';
  import { app } from '../stores/app.svelte';
  import { onMount } from 'svelte';

  let canvasRef: EditorCanvas | undefined = $state();
  let exportOpen = $state(false);
  let fileMenu = $state(false);
  void Smile;

  const tools: { id: Tool; label: string; icon: typeof Images }[] = [
    { id: 'photos', label: 'Photos', icon: Images },
    { id: 'layouts', label: 'Layouts', icon: LayoutGrid },
    { id: 'templates', label: 'Templates', icon: LayoutTemplate },
    { id: 'text', label: 'Text', icon: Type },
    { id: 'stickers', label: 'Stickers', icon: Sticker },
    { id: 'shapes', label: 'Shapes', icon: Shapes },
    { id: 'calendar', label: 'Calendar', icon: CalendarDays },
    { id: 'background', label: 'Background', icon: Paintbrush },
    { id: 'page', label: 'Page', icon: Ruler },
  ];

  // Autosave into the project library after every change.
  $effect(() => {
    void editor.doc;
    if (editor.open && editor.dirty) scheduleAutosave();
  });

  /** Replace / fill the photo of a frame via the file picker. */
  async function requestPhoto(elementId: string) {
    const ids = await addPhotos();
    if (!ids.length) return;
    const el = editor.element(elementId);
    if (!el || el.kind !== 'photo') return;
    if (el.name === '__auto_placeholder') editor.addToLayout(ids);
    else {
      editor.setPhotoAsset(elementId, ids[0]);
      if (ids.length > 1) {
        if (editor.hasLayout) editor.addToLayout(ids.slice(1));
        else editor.addFreePhotos(ids.slice(1));
      }
    }
  }

  export function dropFiles(assetIds: string[], x: number, y: number): boolean {
    return canvasRef?.dropAssets(assetIds, x, y) ?? false;
  }

  export function dragOverFiles(x: number, y: number): void {
    canvasRef?.dragOver(x, y);
  }

  function isTyping(e: KeyboardEvent): boolean {
    const t = e.target as HTMLElement | null;
    return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
  }

  function onKey(e: KeyboardEvent) {
    if (app.view !== 'editor' || exportOpen || app.modals > 0) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.toLowerCase();
    if (mod && k === 's') {
      e.preventDefault();
      // Commit a half-typed name (inputs apply on change/blur) before saving.
      if (isTyping(e)) (e.target as HTMLElement).blur();
      if (e.shiftKey) exportOpen = true;
      else void quickSave();
      return;
    }
    if (mod && k === 'r') {
      e.preventDefault();
      if (!isTyping(e)) editor.regenerate();
      return;
    }
    if (mod && k === 'p') {
      e.preventDefault();
      void printProject('all');
      return;
    }
    if (mod && (e.key === 'PageDown' || e.key === 'PageUp')) {
      e.preventDefault();
      editor.goToPage(Math.max(0, Math.min(editor.pageCount - 1, editor.activePage + (e.key === 'PageDown' ? 1 : -1))));
      return;
    }
    if (isTyping(e)) return;
    if (mod && k === 'z') {
      e.preventDefault();
      if (e.shiftKey) editor.redo();
      else editor.undo();
    } else if (mod && k === 'y') {
      e.preventDefault();
      editor.redo();
    } else if (mod && k === 'd') {
      e.preventDefault();
      editor.duplicateSelected();
    } else if (mod && k === 'c') {
      editor.copy();
    } else if (mod && k === 'v') {
      editor.paste();
    } else if (mod && k === 'a') {
      e.preventDefault();
      editor.selectAll();
    } else if (mod && (k === '=' || k === '+')) {
      e.preventDefault();
      canvasRef?.zoomBy(1.25);
    } else if (mod && k === '-') {
      e.preventDefault();
      canvasRef?.zoomBy(0.8);
    } else if (mod && k === '0') {
      e.preventDefault();
      canvasRef?.fitView();
    } else if (mod && k === '1') {
      e.preventDefault();
      canvasRef?.zoomTo(1);
    } else if (mod && e.key === ']') {
      editor.arrange(e.shiftKey ? 'front' : 'forward');
    } else if (mod && e.key === '[') {
      editor.arrange(e.shiftKey ? 'back' : 'backward');
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      editor.deleteSelected();
    } else if (e.key === 'Escape') {
      if (editor.cropTarget) editor.cropTarget = null;
      else editor.clearSelection();
    } else if (e.key === 'Enter') {
      if (editor.cropTarget) editor.cropTarget = null;
      else if (editor.single?.kind === 'text') {
        e.preventDefault();
        editor.editingText = editor.single.id;
      }
    } else if (e.key.startsWith('Arrow')) {
      e.preventDefault();
      const step = (e.shiftKey ? 10 : 1) / editor.zoom;
      const dx = e.key === 'ArrowLeft' ? -step : e.key === 'ArrowRight' ? step : 0;
      const dy = e.key === 'ArrowUp' ? -step : e.key === 'ArrowDown' ? step : 0;
      editor.nudge(dx, dy);
    }
  }

  onMount(() => {
    const close = () => (fileMenu = false);
    window.addEventListener('click', close);
    // Letter layouts are shaped by a font: re-run them once it has loaded.
    const offFonts = onFontsLoaded(() => editor.refreshLayoutQuietly());
    return () => {
      window.removeEventListener('click', close);
      offFonts();
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="editor">
  <header class="topbar">
    <div class="menu-wrap">
      <button
        class="btn ghost"
        onclick={(e) => {
          e.stopPropagation();
          fileMenu = !fileMenu;
        }}>File <ChevronDown size={14} /></button
      >
      {#if fileMenu}
        <div class="menu card" role="menu">
          <button role="menuitem" onclick={() => newBlank()}><FilePlus size={15} /> New blank collage</button>
          <button role="menuitem" onclick={() => void openProjectDialog()}><FolderOpen size={15} /> Open project…</button>
          <button role="menuitem" onclick={() => void saveProjectAs()}><Save size={15} /> Save project as…</button>
          <div class="divider"></div>
          <button role="menuitem" onclick={() => void quickSave()}><Download size={15} /> Save image <span class="kbd">Ctrl+S</span></button>
          <button role="menuitem" onclick={() => (exportOpen = true)}><FileImage size={15} /> Save image as… <span class="kbd">Ctrl+⇧+S</span></button>
          <button role="menuitem" disabled={saveState.busy} onclick={() => void printProject('all')}><Printer size={15} /> Print… <span class="kbd">Ctrl+P</span></button>
          <div class="divider"></div>
          <button role="menuitem" onclick={() => editor.addPage()}><FilePlus size={15} /> Add a page</button>
          {#if editor.pageCount > 1}
            <button role="menuitem" onclick={() => editor.duplicatePage()}><Copy size={15} /> Duplicate this page</button>
            <button role="menuitem" onclick={() => editor.removePage()}><Trash2 size={15} /> Delete this page</button>
          {/if}
        </div>
      {/if}
    </div>
    <input class="name" value={editor.doc.name} onchange={(e) => editor.rename((e.target as HTMLInputElement).value)} aria-label="Collage name" />
    <div class="group">
      <button class="icon-btn" title="Undo (Ctrl+Z)" disabled={!editor.canUndo} onclick={() => editor.undo()}><Undo2 size={17} /></button>
      <button class="icon-btn" title="Redo (Ctrl+Shift+Z)" disabled={!editor.canRedo} onclick={() => editor.redo()}><Redo2 size={17} /></button>
    </div>
    <div class="spacer"></div>
    <div class="group zoom">
      <button class="icon-btn" title="Zoom out (Ctrl+−)" onclick={() => canvasRef?.zoomBy(0.8)}><Minus size={16} /></button>
      <button class="zoom-val" title="Fit to window (Ctrl+0)" onclick={() => canvasRef?.fitView()}>{Math.round(editor.zoom * 100)}%</button>
      <button class="icon-btn" title="Zoom in (Ctrl+=)" onclick={() => canvasRef?.zoomBy(1.25)}><Plus size={16} /></button>
      <button class="icon-btn" title="Fit (Ctrl+0)" onclick={() => canvasRef?.fitView()}><Maximize size={15} /></button>
    </div>
    <div class="spacer"></div>
    <button class="btn" onclick={() => (exportOpen = true)} disabled={saveState.busy} title="Choose format, size and location (Ctrl+Shift+S)">Save as…</button>
    <button class="btn primary" onclick={() => void quickSave()} disabled={saveState.busy} title={app.settings.saveDirectory ? `Save a copy to ${app.settings.saveDirectory} (Ctrl+S)` : 'Set a save folder in Settings'}>
      <Save size={16} />
      {saveState.busy ? `${Math.round(saveState.progress * 100)}%` : 'Save'}
    </button>
  </header>

  <div class="body">
    <nav class="rail" aria-label="Editor tools">
      {#each tools as t (t.id)}
        <button class="tool" class:on={editor.tool === t.id} onclick={() => (editor.tool = t.id)} aria-pressed={editor.tool === t.id}>
          <t.icon size={20} />
          <span>{t.label}</span>
        </button>
      {/each}
    </nav>

    <aside class="panel">
      {#if editor.tool === 'photos'}<PhotosPanel />
      {:else if editor.tool === 'layouts'}<LayoutPanel />
      {:else if editor.tool === 'templates'}<TemplatesPanel />
      {:else if editor.tool === 'text'}<TextPanel />
      {:else if editor.tool === 'stickers'}<StickersPanel />
      {:else if editor.tool === 'shapes'}<ShapesPanel />
      {:else if editor.tool === 'calendar'}<CalendarPanel />
      {:else if editor.tool === 'background'}<BackgroundPanel />
      {:else if editor.tool === 'page'}<PagePanel />
      {/if}
    </aside>

    <main class="stage">
      <EditorCanvas bind:this={canvasRef} onRequestPhoto={(id) => void requestPhoto(id)} />
      <PageStrip />
      <footer class="status">
        <span class="shrink">{describePage(editor.doc.page.width, editor.doc.page.height, editor.doc.page.dpi)}</span>
        {#if editor.pageCount > 1}<span>Page {editor.activePage + 1} of {editor.pageCount}</span>{/if}
        <span class="spacer"></span>
        {#if saveState.busy}
          <span class="saving"><span class="bar"><span style:width="{saveState.progress * 100}%"></span></span><span class="shrink">{saveState.stage}</span></span>
        {/if}
        <span>{editor.doc.elements.length} elements</span>
        {#if editor.dirty}<span class="pill">Unsaved image</span>{/if}
      </footer>
    </main>

    <aside class="inspector">
      <Inspector />
    </aside>
  </div>
</div>

{#if exportOpen}
  <ExportDialog onclose={() => (exportOpen = false)} />
{/if}

<style>
  .editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-width: 0;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 52px;
    padding: 0 12px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    flex: none;
  }
  .name {
    width: 240px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    font-weight: 600;
    font-size: 14px;
    outline: none;
  }
  .name:hover {
    border-color: var(--border-strong);
  }
  .name:focus {
    border-color: var(--accent);
    background: var(--bg-2);
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .zoom-val {
    min-width: 56px;
    height: 30px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .zoom-val:hover {
    background: var(--hover);
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    top: 40px;
    left: 0;
    z-index: 60;
    width: 260px;
    padding: 6px;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    cursor: pointer;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu .kbd {
    margin-left: auto;
  }
  .body {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  .rail {
    width: 76px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 6px;
    background: var(--panel);
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  .tool {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 9px 2px 8px;
    border: 0;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-3);
    font-size: 10.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .tool:hover {
    color: var(--text);
    background: var(--hover);
  }
  .tool.on {
    color: var(--accent);
    background: var(--accent-soft);
  }
  .panel {
    width: 300px;
    flex: none;
    background: var(--panel);
    border-right: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
  }
  .stage {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 28px;
    padding: 0 12px;
    font-size: 11.5px;
    color: var(--text-3);
    background: var(--panel);
    border-top: 1px solid var(--border);
    flex: none;
    min-width: 0;
  }
  .status > span {
    white-space: nowrap;
    flex: none;
  }
  .status .shrink {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status > .spacer {
    flex: 1 1 0;
  }
  .status > .saving {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    flex: 0 1 auto;
    min-width: 0;
  }
  .bar {
    flex: none;
  }
  .bar {
    width: 120px;
    height: 4px;
    border-radius: 4px;
    background: var(--border-strong);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent-grad);
  }
  .inspector {
    width: 296px;
    flex: none;
    background: var(--panel);
    border-left: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
  }
</style>
