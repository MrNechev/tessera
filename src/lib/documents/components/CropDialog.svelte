<script lang="ts">
  // Crop (= zoom into) an image or video for this one use. Stored as fractions in the
  // document; the original file is never changed.
  import { onMount } from 'svelte';
  import type { FileItem, MediaRef } from '../lib/types';
  import Icon from './Icon.svelte';

  let { file, mref, onsave, onclose }: { file: FileItem; mref: MediaRef; onsave: (crop: MediaRef['crop']) => void; onclose: () => void } = $props();

  type C = { x: number; y: number; w: number; h: number };
  let c = $state<C>({ x: 0, y: 0, w: 1, h: 1 });
  let aspect = $state<number | null>(null); // width/height of the crop in pixels, null = free
  let box = $state<HTMLDivElement>();
  const W = $derived(file.width || 1600);
  const H = $derived(file.height || 900);

  onMount(() => {
    const v = mref.crop;
    if (v) c = { ...v };
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  const presets: [string, number | null][] = [
    ['Free', null],
    ['Original', -1],
    ['16:9', 16 / 9],
    ['4:3', 4 / 3],
    ['1:1', 1],
    ['3:4', 3 / 4],
    ['9:16', 9 / 16],
  ];

  function setAspect(a: number | null) {
    aspect = a === -1 ? W / H : a;
    if (!aspect) return;
    // largest centred crop with that pixel aspect inside the current one
    const cw = c.w * W,
      ch = c.h * H;
    let nw = cw,
      nh = cw / aspect;
    if (nh > ch) {
      nh = ch;
      nw = ch * aspect;
    }
    const cx = c.x + c.w / 2,
      cy = c.y + c.h / 2;
    c = clamp({ x: cx - nw / W / 2, y: cy - nh / H / 2, w: nw / W, h: nh / H });
  }

  function clamp(v: C): C {
    const w = Math.min(1, Math.max(0.02, v.w)),
      h = Math.min(1, Math.max(0.02, v.h));
    return { x: Math.min(1 - w, Math.max(0, v.x)), y: Math.min(1 - h, Math.max(0, v.y)), w, h };
  }

  function start(e: PointerEvent, mode: string) {
    e.preventDefault();
    e.stopPropagation();
    const r = box!.getBoundingClientRect();
    const s = { ...c };
    const px = e.clientX,
      py = e.clientY;
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const dx = (ev.clientX - px) / r.width,
        dy = (ev.clientY - py) / r.height;
      let n = { ...s };
      if (mode === 'move') n = { ...s, x: s.x + dx, y: s.y + dy };
      else {
        if (mode.includes('w')) {
          n.x = s.x + dx;
          n.w = s.w - dx;
        }
        if (mode.includes('e')) n.w = s.w + dx;
        if (mode.includes('n')) {
          n.y = s.y + dy;
          n.h = s.h - dy;
        }
        if (mode.includes('s')) n.h = s.h + dy;
        if (aspect) {
          // keep the pixel aspect: height follows width
          const nh = (n.w * W) / aspect / H;
          if (mode.includes('n')) n.y = s.y + s.h - nh;
          n.h = nh;
        }
      }
      if (mode === 'move') {
        n.x = Math.min(1 - n.w, Math.max(0, n.x));
        n.y = Math.min(1 - n.h, Math.max(0, n.y));
        c = n;
      } else c = clamp(n);
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  function save() {
    const full = c.w > 0.995 && c.h > 0.995;
    const r = (v: number) => Math.round(v * 10000) / 10000;
    onsave(full ? undefined : { x: r(c.x), y: r(c.y), w: r(c.w), h: r(c.h) });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog crop" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Crop</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body">
      <div class="presets">
        {#each presets as [l, a] (l)}
          <button class="seg" class:on={(a === -1 ? Math.abs((aspect ?? 0) - W / H) < 0.001 : aspect === a)} onclick={() => setAspect(a)}>{l}</button>
        {/each}
        <span class="grow"></span>
        <span class="muted">{Math.round(c.w * W)} × {Math.round(c.h * H)} px</span>
      </div>
      <div class="stage">
        <div class="imgbox" bind:this={box} style="aspect-ratio:{W}/{H}">
          <img src="/d/{file.key}/t1280.webp" alt="" draggable="false" />
          <div class="shade" style="clip-path:polygon(0 0,100% 0,100% 100%,0 100%,0 0,{c.x * 100}% {c.y * 100}%,{c.x * 100}% {(c.y + c.h) * 100}%,{(c.x + c.w) * 100}% {(c.y + c.h) * 100}%,{(c.x + c.w) * 100}% {c.y * 100}%,{c.x * 100}% {c.y * 100}%)"></div>
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="rect" style="left:{c.x * 100}%;top:{c.y * 100}%;width:{c.w * 100}%;height:{c.h * 100}%" onpointerdown={(e) => start(e, 'move')}>
            {#each ['nw', 'ne', 'se', 'sw', 'n', 'e', 's', 'w'] as h (h)}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span class="hd h-{h}" onpointerdown={(e) => start(e, h)}></span>
            {/each}
          </div>
        </div>
      </div>
      <div class="acts">
        <button class="btn" onclick={() => { c = { x: 0, y: 0, w: 1, h: 1 }; aspect = null; }}>Reset</button>
        <span class="grow"></span>
        <button class="btn" onclick={onclose}>Cancel</button>
        <button class="btn primary" onclick={save}>Apply</button>
      </div>
    </div>
  </div>
</div>

<style>
  .crop {
    width: min(1100px, 100%);
  }
  .presets,
  .acts {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .acts {
    margin-top: 12px;
  }
  .grow {
    flex: 1;
  }
  .seg {
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    background: var(--surface-2);
    font-size: 12px;
    cursor: pointer;
  }
  .seg.on {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .stage {
    margin-top: 10px;
    display: flex;
    justify-content: center;
    background: #000;
    border-radius: 8px;
    padding: 10px;
  }
  .imgbox {
    position: relative;
    max-width: 100%;
    max-height: 62vh;
    height: 62vh;
    user-select: none;
    touch-action: none;
  }
  .imgbox img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .shade {
    position: absolute;
    inset: 0;
    background: #000a;
    pointer-events: none;
  }
  .rect {
    position: absolute;
    border: 2px solid #fff;
    box-shadow: 0 0 0 1px #0008;
    cursor: move;
  }
  .hd {
    position: absolute;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    background: #fff;
    border-radius: 3px;
  }
  .h-nw { left: 0; top: 0; cursor: nwse-resize; }
  .h-ne { left: 100%; top: 0; cursor: nesw-resize; }
  .h-se { left: 100%; top: 100%; cursor: nwse-resize; }
  .h-sw { left: 0; top: 100%; cursor: nesw-resize; }
  .h-n { left: 50%; top: 0; cursor: ns-resize; }
  .h-s { left: 50%; top: 100%; cursor: ns-resize; }
  .h-e { left: 100%; top: 50%; cursor: ew-resize; }
  .h-w { left: 0; top: 50%; cursor: ew-resize; }
</style>
