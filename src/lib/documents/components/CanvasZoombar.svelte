<script lang="ts">
  // The canvas zoom bar (bottom right): zoom, minimap, arrange and export menus, grid
  // snapping, wheel mode and the presentation button.
  import { steps } from '../lib/canvas';
  import { setPref, type CanvasEditor } from '../lib/canvasEditor.svelte';
  import Icon from './Icon.svelte';

  let { ed }: { ed: CanvasEditor } = $props();
  const stepCount = $derived(steps(ed.doc).length);
</script>

<div class="zoombar">
  <button class="icon-btn" title="Zoom out (−)" onclick={() => ed.zoomBy(0.8)}><Icon name="zoom-out" /></button>
  <button class="zval" title="Zoom to 100% (Ctrl+0)" onclick={() => ed.zoom100()}>{Math.round(ed.view.z * 100)}%</button>
  <button class="icon-btn" title="Zoom in (+)" onclick={() => ed.zoomBy(1.25)}><Icon name="zoom-in" /></button>
  <button class="icon-btn" title="Show everything (Shift+1)" onclick={() => ed.fitAll()}><Icon name="maximize" /></button>
  <span class="vsep"></span>
  <button
    class="icon-btn"
    class:on={ed.showMinimap}
    title="Minimap"
    onclick={() => {
      ed.showMinimap = !ed.showMinimap;
      setPref('minimap', ed.showMinimap ? '1' : '0');
    }}><Icon name="map" /></button
  >
  <div class="zmenu">
    <button class="icon-btn" title="Arrange cards…" onclick={() => (ed.arrangeMenu = !ed.arrangeMenu)}><Icon name="arrange" /></button>
    {#if ed.arrangeMenu}
      <div class="zpop">
        <div class="zh">Arrange {ed.selCards.length >= 2 ? `${ed.selCards.length} selected cards` : 'all cards'}</div>
        <button onclick={() => ed.arrange('grid')}><Icon name="grid" size={14} /> Grid (story order)</button>
        <button onclick={() => ed.arrange('timeline')}><Icon name="calendar" size={14} /> Timeline (date taken)</button>
        <button onclick={() => ed.arrange('folder')}><Icon name="folder" size={14} /> By folder (with frames)</button>
      </div>
    {/if}
  </div>
  <div class="zmenu">
    <button class="icon-btn" title="Export…" onclick={() => (ed.exportMenu = !ed.exportMenu)}><Icon name="download" /></button>
    {#if ed.exportMenu}
      <div class="zpop">
        <div class="zh">Export</div>
        <button onclick={() => ed.doExport('all')}><Icon name="image2" size={14} /> PNG of the whole canvas</button>
        <button onclick={() => ed.doExport('view')}><Icon name="image2" size={14} /> PNG of the current view</button>
        <button disabled={!ed.selected.size} onclick={() => ed.doExport('sel')}><Icon name="image2" size={14} /> PNG of the selection</button>
        <button onclick={() => ed.doExport(ed.selected.size ? 'sel' : 'all', true)}
          ><Icon name="image2" size={14} /> PNG with a see-through background{ed.selected.size ? ' (selection)' : ''}</button
        >
        <button onclick={() => ed.exportSvg('all')}><Icon name="code2" size={14} /> SVG of the whole canvas</button>
        <button disabled={!ed.selected.size} onclick={() => ed.exportSvg('sel')}><Icon name="code2" size={14} /> SVG of the selection</button>
        <button
          onclick={() => {
            ed.exportMenu = false;
            window.open(`#/print/${ed.doc.id}?mode=canvas`, '_blank');
          }}><Icon name="file-pdf" size={14} /> PDF (one page per frame)</button
        >
      </div>
    {/if}
  </div>
  <button
    class="icon-btn"
    class:on={ed.gridOn}
    title="Snap to grid (hold Ctrl to move freely)"
    onclick={() => {
      ed.gridOn = !ed.gridOn;
      setPref('grid', ed.gridOn ? '1' : '0');
    }}><Icon name="dots" /></button
  >
  <button
    class="icon-btn"
    class:on={ed.wheelZoom}
    title={ed.wheelZoom ? 'Mouse wheel zooms (Shift+wheel pans). Click to make the wheel pan.' : 'Mouse wheel pans (Ctrl+wheel zooms). Click to make the wheel zoom.'}
    onclick={() => {
      ed.wheelZoom = !ed.wheelZoom;
      setPref('wheel', ed.wheelZoom ? 'zoom' : 'pan');
    }}><Icon name="mouse" /></button
  >
  <span class="vsep"></span>
  <button class="btn sm ghost" title="Present the frames (F5) · Shift+F5 from the selected frame" onclick={() => ed.present(true)} disabled={!stepCount}>
    <Icon name="present" size={15} />
    {stepCount} step{stepCount === 1 ? '' : 's'}
  </button>
</div>

<style>
  .zoombar {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 5;
  }
  .zval {
    min-width: 54px;
    height: 30px;
    border: none;
    background: none;
    font: 12px var(--mono);
    color: var(--text-2);
    cursor: pointer;
    border-radius: 6px;
  }
  .zval:hover {
    background: var(--surface-2);
  }
</style>
