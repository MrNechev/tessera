<script lang="ts">
  /**
   * Paint: a layered raster editor. Without an open drawing it shows the
   * start screen (new image, open, recent drawings); with one, the tool
   * column, tool options, the canvas, colours and layers.
   */
  import {
    Brush,
    ClipboardPaste,
    FolderOpen,
    HelpCircle,
    ImageDown,
    ImagePlus,
    Images,
    Laugh,
    Maximize,
    Minus,
    Plus,
    Redo2,
    Save,
    Trash2,
    Undo2,
    X,
  } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, fileUrl, isTauri, pickImages } from '../api/backend';
  import { call } from '../api/http';
  import Modal from '../components/ui/Modal.svelte';
  import { palette } from '../documents/lib/palette.svelte';
  import type { FileItem } from '../documents/lib/types';
  import { app } from '../stores/app.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { addMemeText, applyCrop, copySelection, handleKey, layerFrom, onPaste, pasteShortcut, selectSubject, setTool, zoomBy, zoomTo } from './actions.svelte';
  import { blobCanvas, imageDataCanvas } from './io';
  import { drawings, type DrawingSummary } from './lib/store';
  import { closePaint, dropPaint, flushPaint, newPaintFromClipboard, openPaint, openPaintDoc, saveNow } from './session.svelte';
  import { paint } from './state.svelte';
  import ColorsPanel from './components/ColorsPanel.svelte';
  import ExportDialog from './components/ExportDialog.svelte';
  import LayersPanel from './components/LayersPanel.svelte';
  import LibraryPickDialog from './components/LibraryPickDialog.svelte';
  import LookDialog from './components/LookDialog.svelte';
  import Menu, { type MenuItem } from './components/Menu.svelte';
  import NewImageDialog from './components/NewImageDialog.svelte';
  import PaintCanvas from './components/PaintCanvas.svelte';
  import ResizeDialog from './components/ResizeDialog.svelte';
  import ShortcutsDialog from './components/ShortcutsDialog.svelte';
  import TextEditor from './components/TextEditor.svelte';
  import Toolbox from './components/Toolbox.svelte';
  import ToolOptions from './components/ToolOptions.svelte';

  const d = $derived(paint.doc);
  let canvasView: PaintCanvas | undefined = $state();
  let recent = $state<DrawingSummary[]>([]);
  let picking = $state<null | 'open' | 'layer' | 'meme'>(null);
  let deleting = $state<DrawingSummary | null>(null);
  let fileInput: HTMLInputElement | undefined = $state();
  let fileFor: 'open' | 'layer' = 'open';

  async function refresh() {
    recent = await drawings.list().catch(() => []);
  }

  function fit() {
    canvasView?.fit();
  }

  /* ---------------------------------------------------------- opening */

  let pickTarget: 'open' | 'layer' | 'meme' = 'open';
  async function fromLibrary(f: FileItem) {
    if (pickTarget === 'layer') return addLayerFrom(f);
    const doc = await openPaint({ libraryId: f.libraryId, path: f.path, name: f.name });
    if (doc && pickTarget === 'meme') await addMemeText();
  }

  async function addLayerFrom(f: FileItem) {
    try {
      const path = await call<string>('resolve', { libraryId: f.libraryId, path: f.path });
      const px = await api.loadImagePixels(path, 8192, 8192);
      addLayerCanvas(imageDataCanvas(px), f.name);
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not add the image', message: String(e instanceof Error ? e.message : e) });
    }
  }

  function addLayerCanvas(src: HTMLCanvasElement, name: string) {
    const c = layerFrom(src);
    if (c && paint.doc) paint.doc.addLayer(name.replace(/\.[^.]+$/, ''), c, 'Add image');
  }

  /** Open from this computer: a file input in the browser, the native dialog on the desktop. */
  async function openFile(target: 'open' | 'layer') {
    if (!isTauri) {
      fileFor = target;
      fileInput?.click();
      return;
    }
    const files = await pickImages(app.settings.recentImportDir, target === 'open' ? 'Open an image or drawing' : 'Add an image as a layer');
    const path = files[0];
    if (!path) return;
    if (target === 'open') await openPaint({ path });
    else {
      const px = await api.loadImagePixels(path, 8192, 8192);
      addLayerCanvas(imageDataCanvas(px), path.split('/').pop() ?? 'Image');
    }
  }

  async function onFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    const f = input.files?.[0];
    input.value = '';
    if (!f) return;
    if (fileFor === 'open') await openPaint({ blob: f, name: f.name });
    else addLayerCanvas(await blobCanvas(f), f.name);
  }

  function pickFromLibrary(target: 'open' | 'layer' | 'meme') {
    pickTarget = target;
    picking = target;
  }

  async function removeDrawing(s: DrawingSummary) {
    deleting = null;
    try {
      if (paint.doc?.id === s.id) dropPaint();
      await drawings.delete(s.id);
      toasts.push({ kind: 'info', title: `Deleted “${s.name}”`, message: 'Images you exported from it are kept.' });
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not delete the drawing', message: String(e) });
    }
    await refresh();
  }

  function ago(iso: string): string {
    const s = (Date.now() - Date.parse(iso)) / 1000;
    if (s < 60) return 'just now';
    if (s < 3600) return `${Math.floor(s / 60)} min ago`;
    if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
    return new Date(iso).toLocaleDateString();
  }

  /* ---------------------------------------------------------- menus */

  const imageMenu = (): MenuItem[] => {
    const doc = paint.doc!;
    return [
      { label: 'Resize image…', run: () => (paint.dialog = 'resize') },
      { label: 'Canvas size…', run: () => (paint.dialog = 'canvas') },
      { label: 'Crop…', run: () => setTool('crop'), keys: 'C' },
      { label: 'Crop to selection', run: () => { doc.cropToSelection(); paint.fitPending = true; }, disabled: !doc.selBounds },
      'sep',
      { label: 'Rotate right', run: () => { doc.rotate(90); paint.fitPending = true; } },
      { label: 'Rotate left', run: () => { doc.rotate(270); paint.fitPending = true; } },
      { label: 'Rotate 180°', run: () => doc.rotate(180) },
      { label: 'Flip horizontally', run: () => doc.flip('h') },
      { label: 'Flip vertically', run: () => doc.flip('v') },
      'sep',
      { label: 'Look: filters & adjustments…', run: () => (paint.dialog = 'look'), disabled: !doc.active || doc.active.locked },
      { label: 'Flatten', run: () => doc.flatten(), disabled: doc.layers.length < 2 },
    ];
  };
  const selectMenu = (): MenuItem[] => {
    const doc = paint.doc!;
    return [
      { label: 'Select all', run: () => doc.selectAll(), keys: 'Ctrl+A' },
      { label: 'Deselect', run: () => doc.deselect(), keys: 'Ctrl+D', disabled: !doc.selection },
      { label: 'Invert selection', run: () => doc.invertSelection(), keys: 'Ctrl+Shift+I' },
      { label: 'Select subject', run: () => void selectSubject() },
      'sep',
      { label: 'Copy', run: () => void copySelection(false), keys: 'Ctrl+C' },
      { label: 'Cut', run: () => void copySelection(true), keys: 'Ctrl+X' },
      { label: 'Paste', run: () => pasteShortcut(), keys: 'Ctrl+V' },
      'sep',
      { label: 'Fill with the primary colour', run: () => doc.fillSelection(paint.primary), disabled: !doc.selection },
      { label: 'Delete', run: () => doc.clearSelection(), keys: 'Delete', disabled: !doc.selection },
    ];
  };
  const layerMenu = (): MenuItem[] => {
    const doc = paint.doc!;
    return [
      { label: 'New layer', run: () => doc.addLayer() },
      { label: 'Duplicate layer', run: () => doc.duplicateLayer() },
      { label: 'Merge down', run: () => doc.mergeDown(), disabled: !doc.active || doc.index(doc.active.id) <= 0 },
      { label: 'Delete layer', run: () => doc.deleteLayer(), disabled: doc.layers.length < 2 },
      'sep',
      { label: 'Add an image from the library…', run: () => pickFromLibrary('layer') },
      { label: isTauri ? 'Add an image file…' : 'Add an image from this computer…', run: () => void openFile('layer') },
      'sep',
      { label: 'Flip layer horizontally', run: () => doc.flip('h', true) },
      { label: 'Flip layer vertically', run: () => doc.flip('v', true) },
    ];
  };
  const fileMenu = (): MenuItem[] => [
    { label: 'New image…', run: () => (paint.dialog = 'new') },
    { label: 'New from clipboard', run: () => void newPaintFromClipboard() },
    { label: 'Open from the library…', run: () => pickFromLibrary('open') },
    { label: isTauri ? 'Open a file (image or .ora)…' : 'Open from this computer (image or .ora)…', run: () => void openFile('open') },
    'sep',
    { label: 'Save progress', run: () => void saveNow(), keys: 'Ctrl+S' },
    { label: 'Export image…', run: () => (paint.dialog = 'export'), keys: 'Ctrl+E' },
    'sep',
    { label: 'Close drawing', run: () => void close() },
  ];

  async function close() {
    await closePaint();
    await refresh();
  }

  /* ---------------------------------------------------------- lifecycle */

  onMount(() => {
    void refresh();
    // For automated tests and debugging: read pixels, check the state.
    (window as unknown as { tesseraPaint?: unknown }).tesseraPaint = paint;
    const key = (e: KeyboardEvent) => {
      if (app.view !== 'paint') return;
      if (handleKey(e, fit)) e.preventDefault();
    };
    const paste = (e: ClipboardEvent) => {
      if (app.view === 'paint' && !app.modals) void onPaste(e);
    };
    const leave = (e: BeforeUnloadEvent) => {
      if (paint.doc?.dirty || paint.saving === 'saving') {
        void flushPaint();
        e.preventDefault();
      }
    };
    window.addEventListener('keydown', key);
    window.addEventListener('paste', paste);
    window.addEventListener('beforeunload', leave);
    const off = palette.register('paint', () => {
      const doc = paint.doc;
      if (!doc) return [];
      return [
        { id: 'paint:export', group: 'Paint', icon: 'image', title: 'Export the drawing as an image', keywords: 'save png jpeg webp outputs', run: () => void (paint.dialog = 'export') },
        { id: 'paint:save', group: 'Paint', icon: 'file', title: 'Save progress (layers)', keywords: 'ora save', run: () => void saveNow() },
        { id: 'paint:resize', group: 'Paint', icon: 'sliders', title: 'Resize image…', keywords: 'scale size', run: () => void (paint.dialog = 'resize') },
        { id: 'paint:canvas', group: 'Paint', icon: 'square', title: 'Canvas size…', keywords: 'border extend', run: () => void (paint.dialog = 'canvas') },
        { id: 'paint:look', group: 'Paint', icon: 'sliders', title: 'Look: filters & adjustments…', keywords: 'filter adjust brightness', run: () => void (paint.dialog = 'look') },
        { id: 'paint:meme', group: 'Paint', icon: 'brush', title: 'Add meme text (top & bottom)', keywords: 'meme anton caption', run: () => void addMemeText() },
        { id: 'paint:subject', group: 'Paint', icon: 'image', title: 'Select subject', keywords: 'cut out person background', run: () => void selectSubject() },
        { id: 'paint:help', group: 'Paint', icon: 'help', title: 'Paint shortcuts', keywords: 'keys keyboard', run: () => void (paint.dialog = 'help') },
      ];
    });
    return () => {
      window.removeEventListener('keydown', key);
      window.removeEventListener('paste', paste);
      window.removeEventListener('beforeunload', leave);
      off();
    };
  });

  // The recent list refreshes when the start screen comes back.
  $effect(() => {
    if (!d) void refresh();
  });

  const zoomPct = $derived(Math.round(paint.view.zoom * 100));
  const savedLabel = $derived(paint.saving === 'saving' ? 'Saving…' : paint.saving === 'error' ? 'Not saved' : d?.dirty ? 'Edited' : paint.saving === 'saved' ? 'Saved' : '');
</script>

<input bind:this={fileInput} type="file" accept="image/*,.ora,image/openraster" hidden onchange={onFile} />

{#if !d}
  <div class="start">
    <div class="intro">
      <h1><Brush size={30} /> Paint</h1>
      <p>Draw, retouch and annotate images with layers. Your library stays untouched: drawings are saved in Tessera and exported into Outputs › Drawings.</p>
    </div>
    <div class="actions">
      <button class="action a1" onclick={() => (paint.dialog = 'new')}>
        <span class="ico"><ImagePlus size={22} /></span>
        <span class="t">New image</span>
        <span class="d">A blank canvas: screen, square, A4, phone story or your own size.</span>
      </button>
      <button class="action a2" onclick={() => pickFromLibrary('open')}>
        <span class="ico"><Images size={22} /></span>
        <span class="t">Paint on a photo</span>
        <span class="d">Pick a photo from the library; it becomes the bottom layer.</span>
      </button>
      <button class="action a3" onclick={() => pickFromLibrary('meme')}>
        <span class="ico"><Laugh size={22} /></span>
        <span class="t">Make a meme</span>
        <span class="d">A photo with classic top and bottom text, in seconds.</span>
      </button>
      <button class="action a4" onclick={() => void newPaintFromClipboard()}>
        <span class="ico"><ClipboardPaste size={22} /></span>
        <span class="t">From clipboard</span>
        <span class="d">Paste a screenshot or a copied image (or press Ctrl+V here).</span>
      </button>
    </div>
    <div class="row">
      <button class="btn" onclick={() => void openFile('open')}><FolderOpen size={16} /> {isTauri ? 'Open a file…' : 'Open from this computer…'}</button>
      <span class="hint">Images and OpenRaster (.ora) drawings from GIMP, Krita or MyPaint.</span>
    </div>

    <section>
      <h2>Your drawings</h2>
      {#if recent.length}
        <div class="recent">
          {#each recent as r (r.id)}
            <div class="proj card" role="button" tabindex="0" onclick={() => void openPaintDoc(r.id)} onkeydown={(e) => e.key === 'Enter' && void openPaintDoc(r.id)}>
              <div class="pthumb checker">{#if r.thumb}<img src={fileUrl(r.thumb, r.modified)} alt="" loading="lazy" />{/if}</div>
              <div class="pinfo">
                <span class="truncate"><b>{r.name}</b></span>
                <span class="hint">{ago(r.modified)} · {r.width}×{r.height}</span>
              </div>
              <button class="icon-btn small del" title="Delete" aria-label="Delete drawing" onclick={(e) => { e.stopPropagation(); deleting = r; }}><Trash2 size={14} /></button>
            </div>
          {/each}
        </div>
      {:else}
        <p class="hint">Drawings appear here as you work on them — they save automatically.</p>
      {/if}
    </section>
  </div>
{:else}
  <div class="paint">
    <header class="top">
      <input class="title" aria-label="Drawing name" value={d.name} onchange={(e) => { d.name = e.currentTarget.value.trim() || 'Untitled drawing'; d.touched(); }} />
      <span class="status" class:err={paint.saving === 'error'}>{savedLabel}</span>
      <Menu label="File" items={fileMenu} />
      <Menu label="Image" items={imageMenu} />
      <Menu label="Layer" items={layerMenu} />
      <Menu label="Select" items={selectMenu} />
      <button class="btn ghost mb" title="Add classic top and bottom meme text" onclick={() => void addMemeText()}><Laugh size={15} /> Meme</button>
      <span class="div"></span>
      <button class="icon-btn" title="Undo{d.histRev >= 0 && d.history.undoLabel ? `: ${d.history.undoLabel}` : ''} (Ctrl+Z)" aria-label="Undo" disabled={d.histRev < 0 || !d.history.canUndo} onclick={() => d.undo()}><Undo2 size={17} /></button>
      <button class="icon-btn" title="Redo{d.histRev >= 0 && d.history.redoLabel ? `: ${d.history.redoLabel}` : ''} (Ctrl+Shift+Z)" aria-label="Redo" disabled={d.histRev < 0 || !d.history.canRedo} onclick={() => d.redo()}><Redo2 size={17} /></button>
      <span class="spacer"></span>
      <div class="zoom">
        <button class="icon-btn small" title="Zoom out (Ctrl −)" aria-label="Zoom out" onclick={() => zoomBy(-1)}><Minus size={14} /></button>
        <button class="zpct" title="100 % (Ctrl+1)" onclick={() => zoomTo(1)}>{zoomPct}%</button>
        <button class="icon-btn small" title="Zoom in (Ctrl +)" aria-label="Zoom in" onclick={() => zoomBy(1)}><Plus size={14} /></button>
        <button class="icon-btn small" title="Fit to window (Ctrl+0)" aria-label="Fit to window" onclick={fit}><Maximize size={14} /></button>
      </div>
      <button class="icon-btn" title="Shortcuts (?)" aria-label="Shortcuts" onclick={() => (paint.dialog = 'help')}><HelpCircle size={17} /></button>
      <button class="btn" title="Save progress with layers (Ctrl+S)" onclick={() => void saveNow()}><Save size={15} /> Save</button>
      <button class="btn primary" title="Export as PNG, JPEG or WebP into Outputs › Drawings (Ctrl+E)" onclick={() => (paint.dialog = 'export')}><ImageDown size={15} /> Export</button>
      <button class="icon-btn" title="Close the drawing" aria-label="Close the drawing" onclick={() => void close()}><X size={17} /></button>
    </header>
    <ToolOptions />
    <div class="body">
      <Toolbox />
      <div class="stage">
        <PaintCanvas bind:this={canvasView} />
        {#if paint.textEdit && d.layer(paint.textEdit.layerId)}
          {#key paint.textEdit.layerId}<TextEditor />{/key}
        {/if}
        {#if paint.crop && paint.tool === 'crop'}
          <div class="crop-bar card"><span>{paint.crop.w} × {paint.crop.h}</span><button class="btn small primary" onclick={applyCrop}>Crop</button><button class="btn small ghost" onclick={() => (paint.crop = null)}>Cancel</button></div>
        {/if}
        <div class="info">{d.width} × {d.height}{d.selBounds ? ` · selection ${d.selBounds.w} × ${d.selBounds.h}` : ''}</div>
      </div>
      <aside class="side">
        <div class="sec"><h3>Colours</h3><ColorsPanel /></div>
        <div class="sec grow"><h3>Layers</h3><LayersPanel /></div>
      </aside>
    </div>
  </div>
{/if}

{#if paint.busy}
  <div class="busy" role="status"><span class="spin-dot"></span>{paint.busy}</div>
{/if}

{#if paint.dialog === 'new'}<NewImageDialog onclose={() => (paint.dialog = null)} />{/if}
{#if paint.dialog === 'export' && d}<ExportDialog onclose={() => (paint.dialog = null)} />{/if}
{#if (paint.dialog === 'resize' || paint.dialog === 'canvas') && d}<ResizeDialog mode={paint.dialog === 'resize' ? 'image' : 'canvas'} onclose={() => (paint.dialog = null)} />{/if}
{#if paint.dialog === 'look' && d?.active}<LookDialog onclose={() => (paint.dialog = null)} />{/if}
{#if paint.dialog === 'help'}<ShortcutsDialog onclose={() => (paint.dialog = null)} />{/if}
{#if picking}
  <LibraryPickDialog title={picking === 'layer' ? 'Add an image as a layer' : picking === 'meme' ? 'Pick a photo for the meme' : 'Paint on a photo'} onpick={(f) => void fromLibrary(f)} onclose={() => (picking = null)} />
{/if}
{#if paint.ask}
  {@const q = paint.ask}
  <Modal title={q.title} width={460} onclose={() => q.resolve(null)}>
    <p class="ask">{q.message}</p>
    {#snippet footer()}
      {#each q.choices as c (c.id)}
        <button class="btn" class:primary={c.primary} onclick={() => q.resolve(c.id)}>{c.label}</button>
      {/each}
    {/snippet}
  </Modal>
{/if}
{#if deleting}
  {@const r = deleting}
  <Modal title="Delete “{r.name}”?" width={420} onclose={() => (deleting = null)}>
    <p class="hint">The drawing and its layers are removed. Images you exported from it stay in Outputs.</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (deleting = null)}>Cancel</button>
      <button class="btn danger" onclick={() => void removeDrawing(r)}>Delete</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .paint {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  .top {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 50px;
    padding: 0 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .title {
    width: 200px;
    height: 32px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 7px;
    background: transparent;
    font-weight: 650;
    font-size: 14px;
  }
  .title:hover,
  .title:focus {
    border-color: var(--border-strong);
    background: var(--bg-2);
    outline: none;
  }
  .status {
    min-width: 56px;
    font-size: 11.5px;
    color: var(--text-3);
    margin-right: 6px;
  }
  .status.err {
    color: var(--danger);
  }
  .mb {
    height: 30px;
    padding: 0 9px;
  }
  .div {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 6px;
  }
  .zoom {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-right: 6px;
  }
  .zpct {
    width: 52px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
  }
  .zpct:hover {
    background: var(--hover);
  }
  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .stage {
    position: relative;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }
  .info {
    position: absolute;
    left: 10px;
    bottom: 8px;
    padding: 2px 8px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--panel) 80%, transparent);
    color: var(--text-3);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    pointer-events: none;
  }
  .crop-bar {
    position: absolute;
    left: 50%;
    bottom: 14px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px 6px 14px;
    box-shadow: var(--shadow);
    font-variant-numeric: tabular-nums;
  }
  .side {
    width: 264px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-left: 1px solid var(--border);
    overflow-y: auto;
  }
  .sec {
    padding: 12px;
    border-bottom: 1px solid var(--border);
  }
  .sec h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    margin-bottom: 10px;
  }
  .busy {
    position: fixed;
    left: 50%;
    top: 70px;
    transform: translateX(-50%);
    z-index: 120;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 16px;
    border-radius: 999px;
    background: var(--panel-3);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow);
    font-size: 13px;
  }
  .spin-dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    border-right-color: transparent;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .ask {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
  }

  /* start screen */
  .start {
    height: 100%;
    overflow-y: auto;
    padding: 36px 44px 60px;
    display: flex;
    flex-direction: column;
    gap: 24px;
  }
  .intro h1 {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 34px;
    font-weight: 800;
    letter-spacing: -0.03em;
  }
  .intro h1 :global(svg) {
    color: var(--accent);
  }
  .intro p {
    margin: 8px 0 0;
    color: var(--text-2);
    font-size: 15px;
    max-width: 760px;
  }
  .actions {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 22px;
    border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    background: var(--panel);
    text-align: left;
    cursor: pointer;
    transition: transform 0.12s, border-color 0.12s;
  }
  .action:hover {
    transform: translateY(-2px);
    border-color: var(--border-strong);
  }
  .action .ico {
    width: 42px;
    height: 42px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    color: #fff;
    margin-bottom: 6px;
  }
  .a1 .ico {
    background: linear-gradient(135deg, #ff5d8f, #ff8a5c);
  }
  .a2 .ico {
    background: linear-gradient(135deg, #8b5cf6, #4f8cff);
  }
  .a3 .ico {
    background: linear-gradient(135deg, #ffc145, #ff8a5c);
  }
  .a4 .ico {
    background: linear-gradient(135deg, #2dd4bf, #4f8cff);
  }
  .action .t {
    font-weight: 700;
    font-size: 15px;
  }
  .action .d {
    color: var(--text-2);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .start .row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  section h2 {
    font-size: 18px;
    margin-bottom: 12px;
  }
  .recent {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(190px, 1fr));
    gap: 14px;
  }
  .proj {
    position: relative;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    cursor: pointer;
    padding: 0;
  }
  .proj:hover {
    border-color: var(--border-strong);
  }
  .pthumb {
    aspect-ratio: 4 / 3;
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .pthumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .pinfo {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 9px 11px;
  }
  .del {
    position: absolute;
    top: 6px;
    right: 6px;
    background: var(--panel);
    opacity: 0;
  }
  .proj:hover .del {
    opacity: 1;
  }
  @media (max-width: 1100px) {
    .actions {
      grid-template-columns: repeat(2, 1fr);
    }
  }
</style>
