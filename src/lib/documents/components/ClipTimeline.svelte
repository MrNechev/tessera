<script lang="ts">
  // Visual clip trimming: a filmstrip from the video's sprite sheet with start/end handles.
  // Drag a handle (or click the strip) and the preview jumps there; ▶ plays the clip.
  import { fmtDuration, spriteLayout } from '../lib/api';
  import type { FileItem } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    file,
    src,
    startAt,
    endAt,
    onchange,
  }: {
    file: FileItem;
    /** playable URL for the preview */
    src: string;
    startAt?: number;
    endAt?: number;
    onchange: (startAt: number | undefined, endAt: number | undefined) => void;
  } = $props();

  const dur = $derived(file.duration || 1);
  const L = $derived(spriteLayout(file));
  const rowsN = $derived(Math.ceil(L.n / L.cols));
  const FRAMES = 12;
  let a = $state(0);
  let b = $state(1);
  let strip = $state<HTMLDivElement>();
  let video = $state<HTMLVideoElement>();
  let playing = $state(false);
  let head = $state<number | null>(null);

  $effect(() => {
    a = (startAt ?? 0) / dur;
    b = endAt ? endAt / dur : 1;
  });

  function frameStyle(k: number) {
    const fi = Math.min(L.n - 1, Math.round((k * (L.n - 1)) / (FRAMES - 1)));
    const x = L.cols > 1 ? ((fi % L.cols) / (L.cols - 1)) * 100 : 0;
    const y = rowsN > 1 ? (Math.floor(fi / L.cols) / (rowsN - 1)) * 100 : 0;
    return `background-image:url(/d/${file.key}/sprite.jpg);background-size:${L.cols * 100}% ${rowsN * 100}%;background-position:${x}% ${y}%`;
  }

  function seek(t: number) {
    if (video) video.currentTime = t * dur;
  }

  function frac(e: PointerEvent) {
    const r = strip!.getBoundingClientRect();
    return Math.min(1, Math.max(0, (e.clientX - r.left) / r.width));
  }

  function drag(which: 'a' | 'b', e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const f = frac(ev);
      if (which === 'a') a = Math.min(f, b - 0.5 / dur);
      else b = Math.max(f, a + 0.5 / dur);
      seek(which === 'a' ? a : b);
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      commit();
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  function commit() {
    const s = Math.round(a * dur * 10) / 10;
    const e = Math.round(b * dur * 10) / 10;
    onchange(s > 0.05 ? s : undefined, e < dur - 0.05 ? e : undefined);
  }

  function clickStrip(e: PointerEvent) {
    seek(frac(e));
  }

  function play() {
    if (!video) return;
    if (playing) {
      video.pause();
      return;
    }
    video.currentTime = a * dur;
    video.play().catch(() => {});
  }

  function onTime() {
    if (!video) return;
    head = video.currentTime / dur;
    if (playing && video.currentTime >= b * dur) {
      video.pause();
      video.currentTime = a * dur;
    }
  }

  function reset() {
    a = 0;
    b = 1;
    commit();
  }
</script>

<div class="ct">
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={video} {src} preload="metadata" muted playsinline poster="/d/{file.key}/t320.webp" ontimeupdate={onTime} onplay={() => (playing = true)} onpause={() => (playing = false)}></video>
  <div class="right">
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="strip" bind:this={strip} onpointerdown={clickStrip} style="aspect-ratio:{(FRAMES * L.w) / L.h}">
      {#each Array(FRAMES) as _, k (k)}<span class="fr" style={frameStyle(k)}></span>{/each}
      <span class="dim" style="left:0;width:{a * 100}%"></span>
      <span class="dim" style="left:{b * 100}%;right:0"></span>
      <span class="sel" style="left:{a * 100}%;width:{(b - a) * 100}%"></span>
      {#if head !== null}<span class="head" style="left:{head * 100}%"></span>{/if}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span class="h hs" style="left:{a * 100}%" onpointerdown={(e) => drag('a', e)} title="Start"></span>
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span class="h he" style="left:{b * 100}%" onpointerdown={(e) => drag('b', e)} title="End"></span>
    </div>
    <div class="row">
      <button class="btn sm" onclick={play}><Icon name={playing ? 'pause' : 'play'} size={13} /> {playing ? 'Pause' : 'Play clip'}</button>
      <span class="muted">{fmtDuration(a * dur) || '0:00'} → {fmtDuration(b * dur)} · {Math.round((b - a) * dur * 10) / 10} s of {fmtDuration(dur)}</span>
      {#if a > 0 || b < 1}<button class="btn sm ghost" onclick={reset}>Whole video</button>{/if}
    </div>
  </div>
</div>

<style>
  .ct {
    display: flex;
    gap: 10px;
    grid-column: 1 / -1;
    align-items: flex-start;
  }
  video {
    width: 150px;
    aspect-ratio: 16 / 9;
    background: #000;
    border-radius: 6px;
    object-fit: contain;
    flex: none;
  }
  .right {
    flex: 1;
    min-width: 0;
  }
  .strip {
    position: relative;
    display: flex;
    width: 100%;
    max-height: 64px;
    border-radius: 6px;
    overflow: visible;
    cursor: pointer;
    touch-action: none;
    user-select: none;
  }
  .fr {
    flex: 1;
    background-color: #000;
    background-repeat: no-repeat;
  }
  .dim {
    position: absolute;
    top: 0;
    bottom: 0;
    background: #000a;
    pointer-events: none;
  }
  .sel {
    position: absolute;
    top: -2px;
    bottom: -2px;
    border: 2px solid var(--accent);
    border-radius: 4px;
    pointer-events: none;
  }
  .head {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    background: #fff;
    pointer-events: none;
  }
  .h {
    position: absolute;
    top: -6px;
    bottom: -6px;
    width: 12px;
    margin-left: -6px;
    background: var(--accent);
    border-radius: 4px;
    cursor: ew-resize;
    touch-action: none;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 12px;
  }
</style>
