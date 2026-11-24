<script lang="ts">
  // Edit a photo (or video) for this one use: crop, filter, colour adjustments, flips —
  // Tessera's photo toolkit, shared with the collage editor. Stored in the document;
  // the original file is never changed.
  import { onMount } from 'svelte';
  import type { FileItem, MediaRef } from '../lib/types';
  import Icon from './Icon.svelte';
  import type { PhotoAdjust } from '../../model/types';
  import { FILTER_PRESETS, presetAdjust } from '../../render/adjust';
  import { fullAdjust, hasLook, lookFilter, lookStyle } from '../../look/looks.svelte';

  export type PhotoEdit = Pick<MediaRef, 'crop' | 'adjust' | 'filter' | 'flipX' | 'flipY'>;

  let { file, mref, onsave, onclose }: { file: FileItem; mref: MediaRef; onsave: (edit: PhotoEdit) => void; onclose: () => void } = $props();

  let adjust = $state<PhotoAdjust>(fullAdjust(undefined));
  let filter = $state<string>('none');
  let flipX = $state(false);
  let flipY = $state(false);
  let tab = $state<'look' | 'crop'>('look');

  const SLIDERS: [keyof PhotoAdjust, string, number, number][] = [
    ['brightness', 'Brightness', -1, 1],
    ['contrast', 'Contrast', -1, 1],
    ['saturation', 'Saturation', -1, 1],
    ['warmth', 'Warmth', -1, 1],
    ['fade', 'Fade', 0, 1],
    ['vignette', 'Vignette', 0, 1],
    ['grayscale', 'Black & white', 0, 1],
    ['sepia', 'Sepia', 0, 1],
  ];

  function pickPreset(id: string) {
    filter = id;
    adjust = presetAdjust(id);
  }

  type C = { x: number; y: number; w: number; h: number };
  let c = $state<C>({ x: 0, y: 0, w: 1, h: 1 });
  let aspect = $state<number | null>(null); // width/height of the crop in pixels, null = free
  let box = $state<HTMLDivElement>();
  const W = $derived(file.width || 1600);
  const H = $derived(file.height || 900);

  onMount(() => {
    const v = mref.crop;
    if (v) c = { ...v };
    adjust = fullAdjust(mref.adjust);
    filter = mref.filter ?? 'none';
    flipX = !!mref.flipX;
    flipY = !!mref.flipY;
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
      // the box may be mirrored (flips): crop coordinates stay in the source's orientation
      const dx = ((ev.clientX - px) / r.width) * (flipX ? -1 : 1),
        dy = ((ev.clientY - py) / r.height) * (flipY ? -1 : 1);
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
    const a = Object.fromEntries(Object.entries(adjust).map(([k, v]) => [k, r(v)])) as unknown as PhotoAdjust;
    const look = hasLook({ adjust: a });
    onsave({
      crop: full ? undefined : { x: r(c.x), y: r(c.y), w: r(c.w), h: r(c.h) },
      adjust: look ? a : undefined,
      filter: look && filter !== 'none' ? filter : undefined,
      flipX: flipX || undefined,
      flipY: flipY || undefined,
    });
  }

  function resetAll() {
    c = { x: 0, y: 0, w: 1, h: 1 };
    aspect = null;
    adjust = fullAdjust(undefined);
    filter = 'none';
    flipX = flipY = false;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog crop" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Edit {file.kind === 'video' ? 'video' : 'photo'}</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body split">
      <div class="left">
      <div class="presets">
        {#each presets as [l, a] (l)}
          <button class="seg" class:on={(a === -1 ? Math.abs((aspect ?? 0) - W / H) < 0.001 : aspect === a)} onclick={() => setAspect(a)}>{l}</button>
        {/each}
        <span class="grow"></span>
        <span class="muted">{Math.round(c.w * W)} × {Math.round(c.h * H)} px</span>
      </div>
      <div class="stage">
        <div class="imgbox" bind:this={box} style="aspect-ratio:{W}/{H};width:min(100%, calc(62vh * {W / H}))" style:transform={flipX || flipY ? `scale(${flipX ? -1 : 1},${flipY ? -1 : 1})` : undefined}>
          <img src="/d/{file.key}/t1280.webp" alt="" draggable="false" style:filter={lookFilter(adjust) || undefined} />
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
      </div>
      <aside class="side">
        <div class="flips">
          <button class="seg" class:on={flipX} onclick={() => (flipX = !flipX)} title="Mirror left–right"><Icon name="arrow-both" size={14} /> Flip horizontal</button>
          <button class="seg" class:on={flipY} onclick={() => (flipY = !flipY)} title="Mirror top–bottom"><Icon name="arrow-both" size={14} /> Flip vertical</button>
        </div>
        <div class="tabs">
          <button class:on={tab === 'look'} onclick={() => (tab = 'look')}>Filters</button>
          <button class:on={tab === 'crop'} onclick={() => (tab = 'crop')}>Adjust</button>
        </div>
        {#if tab === 'look'}
          <div class="filters">
            {#each FILTER_PRESETS as p (p.id)}
              <button class="flt" class:on={filter === p.id} onclick={() => pickPreset(p.id)}>
                <img src="/d/{file.key}/t320.webp" alt="" draggable="false" style={lookStyle({ adjust: presetAdjust(p.id) }) || undefined} />
                <span>{p.name}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="sliders">
            {#each SLIDERS as [k, label, min, max] (k)}
              <label>
                <span class="sl"><span>{label}</span><span class="muted">{Math.round(adjust[k] * 100)}</span></span>
                <input type="range" {min} {max} step="0.01" value={adjust[k]} oninput={(e) => (adjust = { ...adjust, [k]: Number(e.currentTarget.value) })} ondblclick={() => (adjust = { ...adjust, [k]: 0 })} />
              </label>
            {/each}
            <p class="muted small">Double-click a slider to reset it. The same adjustments as in the collage editor.</p>
          </div>
        {/if}
      </aside>
    </div>
    <div class="acts">
      <button class="btn" onclick={resetAll}>Reset all</button>
      <span class="grow"></span>
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" onclick={save}>Apply</button>
    </div>
  </div>
</div>

<style>
  .crop {
    width: min(1280px, 100%);
  }
  .split {
    display: flex;
    gap: 14px;
  }
  .left {
    flex: 1;
    min-width: 0;
  }
  .side {
    width: 280px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
    max-height: calc(62vh + 56px);
    overflow-y: auto;
  }
  .flips {
    display: flex;
    gap: 4px;
  }
  .flips .seg {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
  }
  .tabs {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .tabs button {
    flex: 1;
    padding: 4px 0;
    border: 0;
    border-radius: 6px;
    background: none;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-2);
    cursor: pointer;
  }
  .tabs button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .filters {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .flt {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0;
    border: 0;
    background: none;
    font-size: 11.5px;
    color: var(--text-2);
    cursor: pointer;
  }
  .flt img {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 7px;
    outline: 2px solid transparent;
    outline-offset: 1px;
  }
  .flt.on img {
    outline-color: var(--accent);
  }
  .flt.on {
    color: var(--text);
  }
  .sliders {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sliders label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12.5px;
  }
  .sl {
    display: flex;
    justify-content: space-between;
  }
  .sliders input {
    width: 100%;
    accent-color: var(--accent);
  }
  .small {
    font-size: 11.5px;
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
    flex: none;
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
