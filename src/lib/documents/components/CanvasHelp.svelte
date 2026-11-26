<script lang="ts">
  // The canvas shortcut sheet (? key).
  import Icon from './Icon.svelte';

  let { onclose }: { onclose: () => void } = $props();
</script>

<div class="help">
  <header><b>Canvas shortcuts</b><button class="icon-btn" onclick={onclose}><Icon name="x" /></button></header>
  <div class="hgrid">
    <span><span class="kbd">V</span> select</span><span><span class="kbd">H</span> / <span class="kbd">Space</span>+drag / middle mouse: pan</span>
    <span><span class="kbd">C</span> section card</span><span><span class="kbd">N</span> sticky note · <span class="kbd">T</span> text</span>
    <span><span class="kbd">R</span> rectangle · <span class="kbd">O</span> ellipse · <span class="kbd">D</span> last shape picked</span><span
      ><span class="kbd">A</span> arrow · <span class="kbd">L</span> line (drag between items)</span
    >
    <span><span class="kbd">F</span> frame (presentation step)</span><span>Wheel: zoom · <span class="kbd">Shift</span>+wheel: pan</span>
    <span><span class="kbd">Shift 1</span> show all · <span class="kbd">Shift 2</span> zoom to selection</span><span><span class="kbd">Ctrl 0</span> 100% · <span class="kbd">+</span>/<span class="kbd">−</span> zoom</span>
    <span><span class="kbd">Ctrl Z</span> undo · <span class="kbd">Ctrl Shift Z</span> redo</span><span><span class="kbd">Ctrl D</span> duplicate · <span class="kbd">Del</span> delete</span>
    <span><span class="kbd">P</span> pen · <span class="kbd">M</span> highlighter · <span class="kbd">E</span> eraser (again: whole / part)</span><span>Double-click a card: edit its text in place</span>
    <span><span class="kbd">Ctrl C</span>/<span class="kbd">V</span>/<span class="kbd">X</span> copy, paste, cut (also between canvases and stories)</span><span
      ><span class="kbd">Ctrl Alt C</span>/<span class="kbd">V</span> copy / paste style</span
    >
    <span><span class="kbd">Ctrl G</span> group · <span class="kbd">Ctrl Shift G</span> ungroup</span><span><span class="kbd">Ctrl ]</span>/<span class="kbd">[</span> bring to front / send to back</span>
    <span><span class="kbd">Ctrl Shift L</span> lock / unlock</span><span>Drag the middle of a free arrow to bend it</span>
    <span>Double-click a note or shape (or <span class="kbd">F2</span>): edit text</span><span>Double-click empty space: sticky note</span>
    <span><span class="kbd">Tab</span> on a shape: add a connected child</span><span><span class="kbd">Enter</span> on a child: add a sibling below</span>
    <span>Arrow tool over a shape: blue dots fix the end to a side</span><span>Drag an arrow's label along the line</span>
    <span><span class="kbd">S</span> stickers and icons (click or drag one in)</span><span>Round handle: turn a sticker (<span class="kbd">Shift</span>: 15° steps)</span>
    <span>Alt+click: select one item of a group</span><span><span class="kbd">Alt</span>+drag: copy, connected to the original</span>
    <span>Pink lines: alignment guides</span><span><span class="kbd">Ctrl</span>+drag: no snapping or guides</span>
    <span><span class="kbd">Shift</span>+drag a corner: keep proportions</span><span>Arrow keys: nudge (<span class="kbd">Shift</span>: further)</span>
    <span><span class="kbd">F5</span> present frames</span><span>Drag a frame: moves what is inside it</span>
    <span>Paste Mermaid text (“flowchart TD …”): a diagram</span><span>Tidy up: lay connected shapes out in levels</span>
  </div>
</div>

<style>
  .help {
    position: absolute;
    left: 64px;
    top: 50%;
    transform: translateY(-50%);
    width: min(680px, calc(100% - 90px));
    max-height: calc(100% - 24px);
    overflow: auto;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 7;
    padding: 10px 14px 14px;
    font-size: 13px;
  }
  .help header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .hgrid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px 18px;
    color: var(--text-2);
  }
</style>
