<script lang="ts">
  // A live collage: one page of a collage project, drawn by the collage editor's own
  // renderer into a canvas at the size it is shown (story, slide, canvas card). `still`
  // renders it once into an <img> instead (print, PDF, presenter previews), which the
  // browser prints like any photo. A deleted project shows a placeholder, never an error.
  import { onFontsLoaded } from '../../assets/fonts';
  import type { CollageDoc } from '../../model/types';
  import { AdjustCache } from '../../render/adjust';
  import { renderDocument } from '../../render/renderer';
  import { library } from '../../stores/library.svelte';
  import { stickers } from '../../stores/stickers.svelte';
  import { collageImage, collagePage, collageResources, collages, pageExtras, type CollageImage } from '../lib/collages.svelte';
  import type { CollageRef } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    ref,
    fill = false,
    zoom = 1,
    still = false,
    maxSide = 1600,
  }: {
    ref: CollageRef | undefined;
    /** fit inside the parent's box (slides, cards) instead of taking the full width */
    fill?: boolean;
    /** extra resolution factor: the canvas view scales cards with a transform */
    zoom?: number;
    still?: boolean;
    /** long side of the `still` image, in pixels */
    maxSide?: number;
  } = $props();

  /** Longest side of the live canvas in device pixels (memory stays bounded on 4K screens). */
  const MAX_DEVICE_PX = 3200;

  let box: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let doc = $state.raw<CollageDoc | null>(null);
  let image = $state.raw<CollageImage | null>(null);
  let missing = $state(false);
  let boxW = $state(0);
  let boxH = $state(0);
  let fontGen = $state(0);
  const resources = collageResources(new AdjustCache(64));

  const aspect = $derived(doc ? doc.page.width / doc.page.height : image ? image.width / image.height : ref?.aspect || 4 / 3);
  const cssW = $derived(fill ? Math.min(boxW, boxH * aspect) : boxW);
  const cssH = $derived(cssW / aspect);

  // (Re)load whenever the reference changes or the project was saved.
  $effect(() => {
    const r = ref;
    if (!r?.projectId) {
      missing = true;
      return;
    }
    const target: CollageRef = { projectId: r.projectId, page: r.page };
    void collages.rev[r.projectId];
    let cancelled = false;
    missing = false;
    if (still) {
      void collageImage(target, maxSide).then((img) => {
        if (cancelled) return;
        image = img;
        missing = !img;
      });
    } else {
      collagePage(target).then(
        (d) => {
          if (cancelled) return;
          doc = d;
          void pageExtras(d).then(() => fontGen++);
        },
        () => {
          if (cancelled) return;
          doc = null;
          missing = true;
        },
      );
    }
    return () => {
      cancelled = true;
    };
  });

  $effect(() => onFontsLoaded(() => fontGen++));

  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(([e]) => {
      boxW = e.contentRect.width;
      boxH = e.contentRect.height;
    });
    ro.observe(box);
    return () => ro.disconnect();
  });

  // Draw on the next frame; bitmaps arriving one by one coalesce into few redraws.
  $effect(() => {
    const d = doc;
    const c = canvas;
    if (!d || !c || cssW < 1 || cssH < 1) return;
    void library.version;
    void stickers.version;
    void fontGen;
    const w = cssW;
    const h = cssH;
    const k = zoom;
    const frame = requestAnimationFrame(() => {
      const dpr = window.devicePixelRatio || 1;
      const s = Math.min(dpr * k, MAX_DEVICE_PX / Math.max(w, h));
      const dw = Math.max(1, Math.round(w * s));
      const dh = Math.max(1, Math.round(h * s));
      if (c.width !== dw || c.height !== dh) {
        c.width = dw;
        c.height = dh;
      }
      const ctx = c.getContext('2d');
      if (!ctx) return;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, dw, dh);
      renderDocument(ctx, d, { scale: dw / d.page.width, resources, placeholders: false });
    });
    return () => cancelAnimationFrame(frame);
  });
</script>

<div class="collage-view" class:fill bind:this={box} style:aspect-ratio={fill ? undefined : aspect}>
  {#if missing}
    <div class="missing">
      <Icon name="collage" size={22} />
      <b>{ref?.name ? `The collage “${ref.name}” is missing` : 'This collage is missing'}</b>
      <span>It was deleted, or this story came from another Tessera.</span>
    </div>
  {:else if still}
    {#if image}<img src={image.url} alt={ref?.name ?? 'Collage'} draggable="false" />{/if}
  {:else}
    <canvas bind:this={canvas} style:width="{cssW}px" style:height="{cssH}px"></canvas>
  {/if}
</div>

<style>
  .collage-view {
    position: relative;
    width: 100%;
    min-width: 0;
  }
  .collage-view.fill {
    height: 100%;
    display: grid;
    place-items: center;
  }
  canvas,
  img {
    display: block;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .missing {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 16px;
    text-align: center;
    font-size: 13px;
    color: var(--text-3, #999);
    border: 1.5px dashed var(--line-strong, #8886);
    border-radius: 10px;
  }
  .missing b {
    color: var(--text-2, #bbb);
  }
</style>
