<script lang="ts">
  /** A small live picture of one layer (redrawn when its pixels change). */
  import type { PaintDoc } from '../doc.svelte';

  interface Props {
    doc: PaintDoc;
    id: string;
    rev: number;
  }
  let { doc, id, rev }: Props = $props();
  let cv: HTMLCanvasElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | null = null;

  function draw() {
    timer = null;
    const src = doc.canvases.get(id);
    if (!cv || !src) return;
    const s = Math.min(44 / doc.width, 34 / doc.height);
    const w = Math.max(1, Math.round(doc.width * s));
    const h = Math.max(1, Math.round(doc.height * s));
    const dpr = window.devicePixelRatio || 1;
    cv.width = Math.round(w * dpr);
    cv.height = Math.round(h * dpr);
    cv.style.width = `${w}px`;
    cv.style.height = `${h}px`;
    const c = cv.getContext('2d')!;
    c.imageSmoothingQuality = 'high';
    c.clearRect(0, 0, cv.width, cv.height);
    c.drawImage(src, 0, 0, cv.width, cv.height);
  }

  $effect(() => {
    void rev;
    void doc.width;
    void doc.height;
    // throttled: strokes bump the revision often
    if (!timer) timer = setTimeout(draw, 120);
    return () => {
      if (timer) clearTimeout(timer);
      timer = null;
    };
  });
</script>

<span class="lthumb checker"><canvas bind:this={cv}></canvas></span>

<style>
  .lthumb {
    width: 46px;
    height: 36px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 5px;
    overflow: hidden;
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  canvas {
    display: block;
  }
</style>
