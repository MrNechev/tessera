<script lang="ts">
  // Screen-space layer over the canvas: selection boxes and handles, alignment guides,
  // the pen preview and the previews of what is being drawn.
  import { arrowPath as arrowGeom, arrowPoints, ink, smoothPath } from '../lib/canvas';
  import type { CanvasEditor } from '../lib/canvasEditor.svelte';
  import type { Rect } from '../lib/types';

  let { ed }: { ed: CanvasEditor } = $props();

  const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as const;
  const view = $derived(ed.view);

  const selScreen = $derived(
    [...ed.selected]
      .filter((id) => ed.itemMap.get(id)?.type !== 'arrow')
      .map((id) => {
        const r = ed.rectOf(id);
        const it = ed.itemMap.get(id);
        return r ? { id, r: ed.toScreen(r), rot: it?.type === 'sticker' ? (it.rotation ?? 0) : 0 } : null;
      })
      .filter(Boolean) as { id: string; r: Rect; rot: number }[],
  );

  /** screen position of a handle of a (turned) box: `h` is a compass point, 'rotate' the knob above */
  function handlePos(r: Rect, h: string, rot: number) {
    const dx = h === 'rotate' ? 0 : h.includes('w') ? -r.w / 2 : h.includes('e') ? r.w / 2 : 0;
    const dy = h === 'rotate' ? -r.h / 2 - 26 : h.includes('n') ? -r.h / 2 : h.includes('s') ? r.h / 2 : 0;
    const a = (rot * Math.PI) / 180;
    return { x: r.x + r.w / 2 + dx * Math.cos(a) - dy * Math.sin(a), y: r.y + r.h / 2 + dx * Math.sin(a) + dy * Math.cos(a) };
  }

  const arrowEnds = $derived.by(() => {
    if (!ed.singleArrow) return null;
    const { p1, p2 } = arrowPoints(ed.singleArrow, ed.tg);
    return { p1: ed.toScreenPt(p1), p2: ed.toScreenPt(p2) };
  });
</script>

<div class="overlay">
  {#each selScreen as s (s.id)}
    <div class="selbox" style="left:{s.r.x}px;top:{s.r.y}px;width:{s.r.w}px;height:{s.r.h}px;{s.rot ? `transform:rotate(${s.rot}deg)` : ''}"></div>
  {/each}
  {#if ed.singleSticker && selScreen.length === 1 && !ed.editing}
    <!-- stickers: proportional corners and a rotation knob, all turned with the sticker -->
    {#if !ed.singleSticker.locked}
      {@const s = selScreen[0]}
      {@const top = handlePos(s.r, 'n', s.rot)}
      {@const knob = handlePos(s.r, 'rotate', s.rot)}
      {#each ['nw', 'ne', 'se', 'sw'] as h (h)}
        {@const p = handlePos(s.r, h, s.rot)}
        <div class="handle h-{h}" data-handle={h} style="left:{p.x}px;top:{p.y}px"></div>
      {/each}
      <svg class="pvline"><line class="knobline" x1={top.x} y1={top.y} x2={knob.x} y2={knob.y} /></svg>
      <div class="handle rot" data-handle="rotate" title="Drag to turn (Shift: 15° steps)" style="left:{knob.x}px;top:{knob.y}px"></div>
    {/if}
  {:else if ed.single && selScreen.length === 1 && !ed.editing}
    {@const r = selScreen[0].r}
    {#each HANDLES as h (h)}
      <div
        class="handle h-{h}"
        data-handle={h}
        style="left:{r.x + (h.includes('w') ? 0 : h.includes('e') ? r.w : r.w / 2)}px;top:{r.y + (h.includes('n') ? 0 : h.includes('s') ? r.h : r.h / 2)}px"
      ></div>
    {/each}
  {/if}
  {#each ed.guides.x as gx (gx)}<div class="guide v" style="left:{gx * view.z + view.tx}px"></div>{/each}
  {#each ed.guides.y as gy (gy)}<div class="guide h" style="top:{gy * view.z + view.ty}px"></div>{/each}
  {#if ed.inkPath}
    <svg class="pvline"
      ><path d={smoothPath(ed.inkPath.map(([x, y]) => [x * view.z + view.tx, y * view.z + view.ty]))} class="inkpv" class:hl={ed.tool === 'marker'} stroke={ink(ed.inkColor)} /></svg
    >
  {/if}
  {#if ed.singleArrow && arrowEnds}
    {@const g = arrowGeom(ed.singleArrow, ed.tg)}
    <div class="handle bend" data-handle="curve" title="Drag to bend" style="left:{g.label.x * view.z + view.tx}px;top:{g.label.y * view.z + view.ty}px"></div>
  {/if}
  {#if arrowEnds}
    <div class="handle end" data-handle="from" style="left:{arrowEnds.p1.x}px;top:{arrowEnds.p1.y}px"></div>
    <div class="handle end" data-handle="to" style="left:{arrowEnds.p2.x}px;top:{arrowEnds.p2.y}px"></div>
  {/if}
  {#if ed.preview?.kind === 'rect'}
    {@const r = ed.toScreen(ed.preview.rect)}
    <div class="pv {ed.preview.shape}" style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px"></div>
  {:else if ed.preview?.kind === 'arrow'}
    <svg class="pvline">
      <line
        x1={ed.preview.p1.x * view.z + view.tx}
        y1={ed.preview.p1.y * view.z + view.ty}
        x2={ed.preview.p2.x * view.z + view.tx}
        y2={ed.preview.p2.y * view.z + view.ty}
      />
    </svg>
    {#if ed.preview.target}
      {@const tr = ed.rectOf(ed.preview.target)}
      {#if tr}
        {@const r = ed.toScreen(tr)}
        <div class="pv target" style="left:{r.x}px;top:{r.y}px;width:{r.w}px;height:{r.h}px"></div>
      {/if}
    {/if}
  {/if}
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    pointer-events: none;
    z-index: 3;
  }
  .selbox {
    position: absolute;
    border: 1.5px solid var(--accent);
    border-radius: 3px;
    margin: -1px 0 0 -1px;
  }
  .handle {
    position: absolute;
    width: 11px;
    height: 11px;
    margin: -6px 0 0 -6px;
    background: var(--bg);
    border: 2px solid var(--accent);
    border-radius: 3px;
    pointer-events: auto;
  }
  .handle.end {
    border-radius: 50%;
    width: 13px;
    height: 13px;
    margin: -7px 0 0 -7px;
    cursor: move;
  }
  .h-nw,
  .h-se {
    cursor: nwse-resize;
  }
  .h-ne,
  .h-sw {
    cursor: nesw-resize;
  }
  .h-n,
  .h-s {
    cursor: ns-resize;
  }
  .h-e,
  .h-w {
    cursor: ew-resize;
  }
  .pv {
    position: absolute;
    border: 1.5px dashed var(--accent);
    border-radius: 3px;
  }
  .pv.marquee {
    background: var(--accent-soft);
    border-style: solid;
  }
  .pv.ellipse {
    border-radius: 50%;
  }
  .pv.target {
    border: 2px solid var(--blue);
    background: var(--blue-soft);
  }
  .pvline {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .pvline line {
    stroke: var(--accent);
    stroke-width: 2.5;
    stroke-dasharray: 6 5;
  }
  .guide {
    position: absolute;
    background: #ff3dbb;
    pointer-events: none;
  }
  .guide.v {
    top: 0;
    bottom: 0;
    width: 1px;
  }
  .guide.h {
    left: 0;
    right: 0;
    height: 1px;
  }
  .knobline {
    stroke: var(--accent);
    stroke-width: 1.5;
  }
  .handle.rot {
    border-radius: 50%;
    width: 13px;
    height: 13px;
    margin: -7px 0 0 -7px;
    cursor: grab;
  }
  .inkpv {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .inkpv.hl {
    stroke-width: 16;
    opacity: 0.38;
  }
  .handle.bend {
    border-radius: 50%;
    background: var(--accent);
    cursor: grab;
  }
</style>
