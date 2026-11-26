<script lang="ts">
  /** Apply a photo look (Tessera's filters and adjustments) to the current layer, with a live preview. */
  import Modal from '../../components/ui/Modal.svelte';
  import LookPanel from '../../look/LookPanel.svelte';
  import type { PhotoAdjust } from '../../model/types';
  import { fullAdjust } from '../../look/looks.svelte';
  import { applyLook, lookPixels } from '../actions.svelte';
  import { ctxOf, newCanvas } from '../doc.svelte';
  import { thumbCanvas } from '../io';
  import { paint } from '../state.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();
  const d = paint.doc!;
  const layer = d.active!;
  const small = thumbCanvas(d.canvas(layer.id), 560);
  const thumb = thumbCanvas(small, 256).toDataURL('image/png');
  let adjust = $state<PhotoAdjust | null>(null);
  let filter = $state<string | null>(null);
  let preview = $state('');

  $effect(() => {
    const a = adjust ? ($state.snapshot(adjust) as PhotoAdjust) : null;
    const t = setTimeout(() => {
      const c = newCanvas(small.width, small.height);
      const g = ctxOf(c);
      g.drawImage(small, 0, 0);
      if (a) {
        const img = g.getImageData(0, 0, c.width, c.height);
        lookPixels(img.data, c.width, c.height, fullAdjust(a));
        g.putImageData(img, 0, 0);
      }
      preview = c.toDataURL('image/png');
    }, 60);
    return () => clearTimeout(t);
  });

  function apply() {
    if (adjust) applyLook(fullAdjust($state.snapshot(adjust) as PhotoAdjust));
    onclose();
  }
</script>

<Modal title="Look: filters and adjustments" width={860} {onclose}>
  <div class="wrap">
    <div class="pv">{#if preview}<img class="checker" src={preview} alt="Preview" />{/if}</div>
    <div class="side">
      <p class="hint">Applies to <b>{layer.name}</b>{d.selection ? ', inside the selection' : ''}.</p>
      <LookPanel {adjust} {filter} {thumb} onchange={(a, f) => { adjust = a; filter = f; }} />
    </div>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!adjust || layer.locked} onclick={apply}>Apply</button>
  {/snippet}
</Modal>

<style>
  .wrap {
    display: flex;
    gap: 16px;
    min-height: 420px;
  }
  .pv {
    flex: 1;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
    background: var(--canvas-bg);
    overflow: hidden;
    min-width: 0;
  }
  .pv img {
    max-width: 100%;
    max-height: 60vh;
    display: block;
  }
  .side {
    width: 300px;
    max-height: 64vh;
    overflow-y: auto;
    flex: none;
  }
</style>
