<script lang="ts">
  // Still preview of a region of a document's canvas (speaker-notes window).
  import { expand, fitView } from '../lib/canvas';
  import type { Doc, Rect } from '../lib/types';
  import CanvasWorld from './CanvasWorld.svelte';

  let { doc, rect, transparent = false }: { doc: Doc; rect: Rect; transparent?: boolean } = $props();
  let w = $state(0);
  let h = $state(0);
  const view = $derived(fitView(rect, w || 1, h || 1, 0.02, 8));
</script>

<div class="thumb" class:transparent bind:clientWidth={w} bind:clientHeight={h}>
  {#if w && h}
    <CanvasWorld {doc} {view} renderRect={expand(rect, 0.1)} mountRect={null} playRect={null} lod={view.z} still present />
  {/if}
</div>

<style>
  .thumb {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--bg);
    pointer-events: none;
  }
  .thumb.transparent {
    background: transparent;
  }
</style>
