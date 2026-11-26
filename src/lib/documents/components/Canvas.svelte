<script lang="ts">
  // Canvas (whiteboard) view of the open document: section cards plus notes, text,
  // shapes, arrows and frames on an infinite pan/zoom surface. Shares the DocState with
  // the document view (autosave, conflicts, history) and adds its own undo stack.
  //
  // State and commands live in CanvasEditor (lib/canvasEditor.svelte.ts); pointer and
  // keyboard handling in lib/canvasPointer.ts and lib/canvasKeys.ts; the toolbars and the
  // selection overlay are their own components.
  import { onMount, untrack } from 'svelte';
  import { paper } from '../lib/canvas';
  import { CanvasEditor } from '../lib/canvasEditor.svelte';
  import { canvasKeyDown, canvasKeyUp } from '../lib/canvasKeys';
  import { CanvasPointer } from '../lib/canvasPointer';
  import StickerGrid from '../../components/StickerGrid.svelte';
  import { lookCss } from '../../text/styles';
  import CanvasTextStyle from './CanvasTextStyle.svelte';
  import type { DocState } from '../lib/docstate.svelte';
  import { dnd } from '../lib/dnd.svelte';
  import { router } from '../lib/router.svelte';
  import type { MediaRef, NoteItem } from '../lib/types';
  import CanvasPanel from './CanvasPanel.svelte';
  import CanvasWorld from './CanvasWorld.svelte';
  import CanvasMinimap from './CanvasMinimap.svelte';
  import CanvasTools from './CanvasTools.svelte';
  import CanvasZoombar from './CanvasZoombar.svelte';
  import CanvasContext from './CanvasContext.svelte';
  import CanvasOverlay from './CanvasOverlay.svelte';
  import CanvasHelp from './CanvasHelp.svelte';
  import Icon from './Icon.svelte';
  import SectionView from './SectionView.svelte';

  let {
    ds,
    showPanel,
    onrelink,
    onpresentdoc,
  }: {
    ds: DocState;
    showPanel: boolean;
    onrelink: (sectionId: string, mediaIndex: number) => void;
    onpresentdoc: (sectionIndex: number) => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  const ed = new CanvasEditor(ds);
  const ptr = new CanvasPointer(ed);

  const doc = $derived(ed.doc);
  const view = $derived(ed.view);

  $effect(() => {
    // viewport resized
    void ed.vw;
    void ed.vh;
    if (ed.ready) untrack(() => ed.settle());
  });

  onMount(() => {
    void ed.start();
    const wheel = (e: WheelEvent) => ptr.wheel(e);
    const vp = ed.vp!;
    vp.addEventListener('wheel', wheel, { passive: false });
    return () => {
      vp.removeEventListener('wheel', wheel);
      ed.stop();
    };
  });

  // sections created elsewhere (drawer: "add section below", duplicate) get a place
  $effect(() => {
    if (!ed.ready) return;
    const un = doc.sections.filter((s) => !s.canvas);
    if (!un.length) return;
    untrack(() => ed.placeNewSections(un));
  });

  // a whole new document (history restore, "load their version"): forget undo + selection
  $effect(() => {
    void ds.revision;
    untrack(() => ed.reset());
  });

  $effect(() => {
    if (ed.ready && ed.pendingFocus) {
      const id = ed.pendingFocus;
      ed.pendingFocus = null;
      untrack(() => ed.focusCard(id));
    }
  });

  /** Library → canvas: a new card with the media, at a point (default: the view centre). */
  export function addMediaCard(refs: MediaRef[], at?: { x: number; y: number }) {
    ed.addMediaCard(refs, at);
  }
  /** select a card / item and bring it into view (command palette, links with ?s=) */
  export function focusCard(id: string) {
    ed.focusCard(id);
  }
  export function present(fromSelection = false) {
    return ed.present(fromSelection);
  }

  const editItem = $derived(ed.editItem);
  const firstNote = $derived(ed.selItems.find((i) => i.type === 'note') as NoteItem | undefined);
  const onlyNotes = $derived(ed.selCards.length === 0 && ed.selItems.length > 0 && ed.selItems.every((i) => i.type === 'note'));

  const bgSize = $derived.by(() => {
    let s = 24 * view.z;
    while (s < 12) s *= 4;
    while (s > 96) s /= 4;
    return s;
  });

  const hiddenCount = $derived(doc.sections.filter((s) => s.canvas?.hidden).length);
  const drawerSection = $derived(ed.drawer ? ed.secMap.get(ed.drawer) : undefined);
  const cursor = $derived(ed.spaceDown || ed.tool === 'hand' ? (ed.panning ? 'grabbing' : 'grab') : ed.tool === 'select' ? 'default' : 'crosshair');
</script>

<svelte:window onkeydown={(e) => canvasKeyDown(ed, e)} onkeyup={(e) => canvasKeyUp(ed, e)} onblur={() => (ed.spaceDown = false)} onpaste={(e) => ed.onPaste(e)} />

<div class="cv" class:with-panel={showPanel}>
  <div class="stage">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="viewport"
      class:drop={ed.dropHint && dnd.dragging}
      bind:this={ed.vp}
      bind:clientWidth={ed.vw}
      bind:clientHeight={ed.vh}
      style="cursor:{cursor}"
      onpointerdown={(e) => ptr.down(e)}
      onpointermove={(e) => ptr.move(e)}
      onpointerup={(e) => ptr.up(e)}
      onpointercancel={(e) => ptr.up(e)}
      ondblclick={(e) => ptr.dblclick(e)}
      ondragover={(e) => ptr.dragOver(e)}
      ondragleave={() => (ed.dropHint = false)}
      ondrop={(e) => ptr.drop(e)}
      oncontextmenu={(e) => e.preventDefault()}
    >
      {#if ed.gridOn}
        <!-- dot grid: a slightly oversized layer moved by transform (no repaint while panning) -->
        <div
          class="gridlayer"
          style="background-size:{bgSize}px {bgSize}px;transform:translate({((view.tx % bgSize) + bgSize) % bgSize}px,{((view.ty % bgSize) + bgSize) % bgSize}px)"
        ></div>
      {/if}
      {#if ed.ready}
        <CanvasWorld
          {doc}
          {view}
          renderRect={ed.renderRect}
          mountRect={ed.mountRect}
          playRect={ed.playRect}
          lod={ed.lod}
          moving={ed.moving}
          selected={ed.selected}
          editing={ed.editing}
          editor
          editingCard={ed.editingCard}
          oncardtext={(id, f, v) => ed.onCardText(id, f, v)}
        />
      {/if}

      {#if editItem}
        <div class="editlayer" style="transform:translate({view.tx}px,{view.ty}px) scale({view.z})">
          <textarea
            class="editbox {editItem.type}"
            class:free={editItem.type === 'note' && editItem.color === 'none'}
            bind:this={ed.editEl}
            bind:value={editItem.text}
            oninput={() => ed.growEdit()}
            onkeydown={(e) => ed.onEditKey(e)}
            onblur={() => ed.commitEdit()}
            spellcheck="true"
            placeholder={editItem.type === 'note' ? (editItem.color === 'none' ? 'Text' : 'Note') : 'Label'}
            style="left:{editItem.x}px;top:{editItem.y}px;width:{editItem.w}px;height:{editItem.h}px;font-size:{editItem.size || 24}px;{editItem.type === 'note'
              ? `background:${paper(editItem.color)};text-align:${editItem.align || 'left'};${lookCss(editItem.look, { pill: editItem.color === 'none' })}`
              : ''}"
          ></textarea>
        </div>
      {/if}

      <CanvasOverlay {ed} />

      {#if ed.ready && !doc.sections.some((s) => s.canvas && !s.canvas.hidden) && !ed.items.length}
        <div class="empty">
          <h3>An empty canvas</h3>
          <p>Drag media from the library here, or pick a tool on the left. Double-click anywhere for a sticky note.</p>
          <p class="muted">Sections of the story show up here as cards. Frames become the steps of a canvas presentation.</p>
        </div>
      {/if}
    </div>

    <CanvasTools {ed} />
    <CanvasZoombar {ed} />

    {#if ed.showMinimap && ed.ready && ed.vw}
      <div class="minimap">
        <CanvasMinimap {doc} {view} vw={ed.vw} vh={ed.vh} onpan={(cx, cy) => ed.setView({ ...view, tx: ed.vw / 2 - cx * view.z, ty: ed.vh / 2 - cy * view.z }, true)} />
      </div>
    {/if}

    <CanvasContext {ed} {onpresentdoc} />

    {#if ed.stickerPicker}
      <div class="sidepop stickers">
        <header>
          <b>Stickers</b>
          <span class="muted">click, or drag onto the canvas</span>
          <button class="icon-btn" title="Close (Esc)" onclick={() => (ed.stickerPicker = false)}><Icon name="x" /></button>
        </header>
        <StickerGrid onpick={(s) => ed.addSticker(s)} draggable autofocus />
      </div>
    {/if}

    {#if ed.styleMenu && firstNote && onlyNotes}
      <div class="sidepop textstyle">
        <CanvasTextStyle look={firstNote.look} onchange={(l) => ed.setLook(l)} onclose={() => (ed.styleMenu = false)} />
      </div>
    {/if}

    {#if ed.help}
      <CanvasHelp onclose={() => (ed.help = false)} />
    {/if}

    {#if drawerSection}
      <aside class="drawer" data-scroll-root>
        <header>
          <b>Edit section</b>
          <span class="grow"></span>
          <button class="icon-btn" title="Show on the canvas" onclick={() => drawerSection.canvas && ed.zoomToRect(drawerSection.canvas)}><Icon name="maximize" /></button>
          <button class="icon-btn" title="Close (Esc)" onclick={() => (ed.drawer = null)}><Icon name="x" /></button>
        </header>
        <div class="dbody">
          {#key drawerSection.id}
            <SectionView
              section={drawerSection}
              index={ds.sectionIndex(drawerSection.id)}
              total={doc.sections.length}
              {ds}
              {onrelink}
              onpresent={(i) => onpresentdoc(i)}
            />
          {/key}
        </div>
      </aside>
    {/if}
  </div>

  {#if showPanel}
    <CanvasPanel
      {ds}
      selected={ed.selected}
      {hiddenCount}
      onbefore={() => ed.before()}
      onzoom={(r) => ed.zoomToRect(r, 0.05, 2)}
      onselect={(id) => ed.select([id])}
      onpresent={(i) => router.go(`/present/${doc.id}?canvas=1&s=${i}`)}
      onaddframe={() => ed.addFrameAroundView()}
      oneditcard={(id) => ed.openDrawer(id)}
    />
  {/if}
</div>

<style>
  .cv {
    position: relative;
    height: 100%;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
  }
  .cv.with-panel {
    grid-template-columns: minmax(0, 1fr) 260px;
  }
  .stage {
    position: relative;
    min-height: 0;
    overflow: hidden;
  }
  .viewport {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background-color: var(--bg);
    touch-action: none;
    user-select: none;
    outline: none;
  }
  .gridlayer {
    position: absolute;
    left: -100px;
    top: -100px;
    right: 0;
    bottom: 0;
    background-image: radial-gradient(circle, var(--line-strong) 1px, transparent 1.2px);
    background-position: 0 0;
    will-change: transform;
    pointer-events: none;
  }
  .viewport.drop {
    box-shadow: inset 0 0 0 3px var(--accent);
  }
  .editlayer {
    position: absolute;
    left: 0;
    top: 0;
    width: 0;
    height: 0;
    transform-origin: 0 0;
    z-index: 2;
  }
  .editbox {
    position: absolute;
    margin: 0;
    border: none;
    outline: 2px solid var(--accent);
    border-radius: 4px;
    resize: none;
    overflow: hidden;
    line-height: 1.3;
    font-family: var(--ui);
    padding: 0.55em 0.65em;
    color: #1d1b22;
    background: transparent;
  }
  .editbox.free {
    color: var(--text);
    padding: 0.1em 0.15em;
  }
  .editbox.rect,
  .editbox.ellipse {
    color: var(--text);
    text-align: center;
    padding: 8%;
    line-height: 1.25;
    background: color-mix(in srgb, var(--bg) 70%, transparent);
  }
  /* side panels over the canvas: sticker picker (left, next to the tools) and text style (right) */
  .sidepop {
    position: absolute;
    top: 12px;
    max-height: calc(100% - 24px);
    overflow: auto;
    padding: 10px 12px 12px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 7;
  }
  .sidepop.stickers {
    left: 64px;
    width: min(320px, calc(100% - 80px));
  }
  .sidepop.textstyle {
    right: 12px;
    width: min(290px, calc(100% - 80px));
  }
  .sidepop header {
    display: flex;
    align-items: center;
    margin-bottom: 8px;
  }
  .sidepop header .muted {
    flex: 1;
    margin-left: 8px;
    font-size: 12px;
  }
  .minimap {
    position: absolute;
    left: 64px;
    bottom: 12px;
    z-index: 5;
  }
  .empty {
    position: absolute;
    left: 50%;
    top: 42%;
    transform: translate(-50%, -50%);
    text-align: center;
    max-width: 460px;
    pointer-events: none;
    color: var(--text-2);
  }
  .empty h3 {
    font: 650 22px var(--serif);
    color: var(--text);
    margin: 0 0 8px;
  }
  .drawer {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(620px, 92%);
    background: var(--bg);
    border-left: 1px solid var(--line-strong);
    box-shadow: var(--shadow);
    z-index: 8;
    display: flex;
    flex-direction: column;
    overflow: auto;
  }
  .drawer header {
    position: sticky;
    top: 0;
    z-index: 6;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    background: var(--bg);
    border-bottom: 1px solid var(--line);
  }
  .grow {
    flex: 1;
  }
  .dbody {
    padding: 14px 16px 80px;
  }

  /* shared by the canvas toolbars (CanvasTools, CanvasZoombar, CanvasContext, style panel) */
  .cv :global(.hsep) {
    height: 1px;
    background: var(--line-strong);
    margin: 3px 4px;
  }
  .cv :global(.vsep) {
    width: 1px;
    height: 20px;
    background: var(--line-strong);
    margin: 0 4px;
    flex: none;
  }
  .cv :global(.sw) {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid transparent;
    padding: 0;
    cursor: pointer;
    flex: none;
    box-shadow: inset 0 0 0 1px #0002;
  }
  .cv :global(.sw.on) {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--bg) inset;
  }
  .cv :global(.sw.none) {
    background: repeating-linear-gradient(45deg, var(--surface-3) 0 4px, var(--surface) 4px 8px);
  }
  .cv :global(.seg) {
    height: 26px;
    min-width: 28px;
    padding: 0 6px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface-2);
    font-size: 12px;
    cursor: pointer;
  }
  .cv :global(.seg.on) {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .cv :global(.zmenu) {
    position: relative;
  }
  .cv :global(.zpop) {
    position: absolute;
    bottom: 40px;
    right: 0;
    min-width: 240px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .cv :global(.zpop .zh) {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    padding: 6px 8px 4px;
  }
  .cv :global(.zpop button) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 8px;
    border: none;
    background: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    font-size: 13px;
  }
  .cv :global(.zpop button:hover:not(:disabled)) {
    background: var(--surface-2);
  }
  .cv :global(.zpop button:disabled) {
    opacity: 0.4;
  }
</style>
