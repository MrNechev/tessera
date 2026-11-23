<script lang="ts">
  // Overview of the whole canvas with the visible area; click or drag to move there.
  import { contentRect, expand, ink, isBox, paper, union, visibleRect, type View } from '../lib/canvas';
  import type { Doc } from '../lib/types';

  let { doc, view, vw, vh, onpan }: { doc: Doc; view: View; vw: number; vh: number; onpan: (cx: number, cy: number) => void } = $props();
  const W = 220,
    H = 140;
  const vis = $derived(visibleRect(view, vw, vh));
  const world = $derived.by(() => {
    const c = contentRect(doc);
    const u = union(c ? [c, vis] : [vis])!;
    return expand(u, 0.05);
  });
  const k = $derived(Math.min(W / world.w, H / world.h));
  const ox = $derived((W - world.w * k) / 2);
  const oy = $derived((H - world.h * k) / 2);
  const sx = (x: number) => ox + (x - world.x) * k;
  const sy = (y: number) => oy + (y - world.y) * k;

  function pan(e: PointerEvent) {
    const el = e.currentTarget as SVGSVGElement;
    el.setPointerCapture(e.pointerId);
    const go = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      onpan(world.x + (ev.clientX - r.left - ox) / k, world.y + (ev.clientY - r.top - oy) / k);
    };
    go(e);
    const up = () => {
      el.removeEventListener('pointermove', go);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', go);
    el.addEventListener('pointerup', up);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<svg class="mm" width={W} height={H} onpointerdown={pan}>
  {#each doc.canvas?.items ?? [] as it (it.id)}
    {#if isBox(it)}
      {#if it.type === 'frame'}
        <rect x={sx(it.x)} y={sy(it.y)} width={it.w * k} height={it.h * k} class="fr" />
      {:else if it.type === 'note'}
        <rect x={sx(it.x)} y={sy(it.y)} width={Math.max(1, it.w * k)} height={Math.max(1, it.h * k)} fill={it.color === 'none' ? 'var(--text-3)' : paper(it.color)} />
      {:else if it.type !== 'path'}
        <rect x={sx(it.x)} y={sy(it.y)} width={it.w * k} height={it.h * k} fill="none" stroke={ink(it.color)} stroke-width="1" />
      {/if}
    {/if}
  {/each}
  {#each doc.sections as s (s.id)}
    {#if s.canvas && !s.canvas.hidden}
      <rect x={sx(s.canvas.x)} y={sy(s.canvas.y)} width={Math.max(1.5, s.canvas.w * k)} height={Math.max(1.5, s.canvas.h * k)} class="card" />
    {/if}
  {/each}
  <rect x={sx(vis.x)} y={sy(vis.y)} width={vis.w * k} height={vis.h * k} class="vp" />
</svg>

<style>
  .mm {
    display: block;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
    cursor: pointer;
    touch-action: none;
  }
  .card {
    fill: var(--text-3);
    opacity: 0.7;
  }
  .fr {
    fill: none;
    stroke: var(--line-strong);
    stroke-dasharray: 2 2;
  }
  .vp {
    fill: var(--accent-soft);
    stroke: var(--accent);
    stroke-width: 1.5;
  }
</style>
