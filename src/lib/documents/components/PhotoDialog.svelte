<script lang="ts">
  /**
   * "Edit photo": Tessera's photo toolkit in one dialog, used everywhere —
   * - story media (`mode="story"`): look, crop & turn, frame & shape, hidden areas;
   * - collage photos (`mode="collage"`): look and hidden areas (the collage editor
   *   crops, turns and frames photos on the page itself);
   * - "Edit a copy" in the Library (`mode="copy"`): look, crop & turn, hidden areas,
   *   saved as a new file in Outputs.
   * Edits are stored as settings; originals are never changed.
   */
  import { onMount } from 'svelte';
  import type { MaskShape, PhotoAdjust } from '../../model/types';
  import { MASK_SHAPES } from '../../render/shapes';
  import LookPanel from '../../look/LookPanel.svelte';
  import { bakeLook, fullAdjust, lookFilter, type Crop, type FrameStyle, type Look } from '../../look/looks.svelte';
  import LookBox, { framedAspect } from '../../look/LookBox.svelte';
  import Icon from './Icon.svelte';

  export type PhotoEdit = Look & { crop?: Crop };

  interface Props {
    /** A ~1280 px rendition of the source. */
    src: string;
    /** Source size in pixels. */
    width: number;
    height: number;
    value: PhotoEdit;
    mode: 'story' | 'collage' | 'copy';
    kind?: 'image' | 'video';
    saveLabel?: string;
    busy?: boolean;
    onsave: (v: PhotoEdit) => void;
    onclose: () => void;
  }
  let { src, width, height, value, mode, kind = 'image', saveLabel = 'Apply', busy = false, onsave, onclose }: Props = $props();

  type Tab = 'look' | 'crop' | 'frame' | 'hide';
  const tabs = $derived<[Tab, string][]>(
    (
      [
        ['look', 'Look'],
        ['crop', 'Crop & turn'],
        ['frame', 'Frame & shape'],
        ['hide', 'Hide areas'],
      ] as [Tab, string][]
    ).filter(([t]) => (mode === 'collage' ? t === 'look' || t === 'hide' : mode === 'copy' ? t !== 'frame' : true) && !(t === 'hide' && kind === 'video')),
  );
  let tab = $state<Tab>('look');

  let e = $state<PhotoEdit>({});
  let c = $state<Crop>({ x: 0, y: 0, w: 1, h: 1 });
  let aspectLock = $state<number | null>(null);
  let box = $state<HTMLDivElement>();
  let hideMode = $state<'blur' | 'pixelate'>('blur');
  let drawing = $state<{ x: number; y: number; w: number; h: number } | null>(null);
  const W = $derived(width || 1600);
  const H = $derived(height || 900);

  onMount(() => {
    e = structuredClone($state.snapshot(value) as PhotoEdit);
    if (value.crop) c = { ...value.crop };
    const k = (ev: KeyboardEvent) => ev.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  /* ---------------------------------------------------------- preview */

  // The result, rendered exactly like exports do (debounced while sliders move).
  let preview = $state('');
  let img: HTMLImageElement | null = null;
  let pending = false;
  let again = false;
  $effect(() => {
    void JSON.stringify(e);
    void c.x, c.y, c.w, c.h;
    if (tab === 'crop' || tab === 'hide') return;
    void render();
  });
  async function render() {
    if (pending) {
      again = true;
      return;
    }
    pending = true;
    try {
      if (!img) {
        img = new Image();
        img.src = src;
        await img.decode();
      }
      await new Promise((r) => requestAnimationFrame(r));
      const full = c.w > 0.995 && c.h > 0.995;
      // frames and masks are drawn around the result by LookBox, so leave them out here
      const out = bakeLook(img, full || mode === 'collage' ? undefined : c, { ...$state.snapshot(e), frame: undefined, mask: undefined }, 1100);
      preview = out.toDataURL('image/jpeg', 0.9);
    } catch {
      /* the source failed to load; the stage shows the plain image */
    } finally {
      pending = false;
      if (again) {
        again = false;
        void render();
      }
    }
  }
  const shownW = $derived((e.rotate === 90 || e.rotate === 270 ? H * c.h : W * c.w) || 1);
  const shownH = $derived((e.rotate === 90 || e.rotate === 270 ? W * c.w : H * c.h) || 1);

  /* ---------------------------------------------------------- crop */

  const presets: [string, number | null][] = [
    ['Free', null],
    ['Original', -1],
    ['16:9', 16 / 9],
    ['4:3', 4 / 3],
    ['1:1', 1],
    ['4:5', 4 / 5],
    ['9:16', 9 / 16],
  ];

  function setAspect(a: number | null) {
    aspectLock = a === -1 ? W / H : a;
    if (!aspectLock) return;
    const cw = c.w * W,
      ch = c.h * H;
    let nw = cw,
      nh = cw / aspectLock;
    if (nh > ch) {
      nh = ch;
      nw = ch * aspectLock;
    }
    const cx = c.x + c.w / 2,
      cy = c.y + c.h / 2;
    c = clamp({ x: cx - nw / W / 2, y: cy - nh / H / 2, w: nw / W, h: nh / H });
  }

  function clamp(v: Crop): Crop {
    const w = Math.min(1, Math.max(0.02, v.w)),
      h = Math.min(1, Math.max(0.02, v.h));
    return { x: Math.min(1 - w, Math.max(0, v.x)), y: Math.min(1 - h, Math.max(0, v.y)), w, h };
  }

  function start(ev: PointerEvent, hmode: string) {
    ev.preventDefault();
    ev.stopPropagation();
    const r = box!.getBoundingClientRect();
    const s0 = { ...c };
    const px = ev.clientX,
      py = ev.clientY;
    const el = ev.currentTarget as HTMLElement;
    el.setPointerCapture(ev.pointerId);
    const move = (mv: PointerEvent) => {
      const dx = (mv.clientX - px) / r.width,
        dy = (mv.clientY - py) / r.height;
      let n = { ...s0 };
      if (hmode === 'move') n = { ...s0, x: s0.x + dx, y: s0.y + dy };
      else {
        if (hmode.includes('w')) {
          n.x = s0.x + dx;
          n.w = s0.w - dx;
        }
        if (hmode.includes('e')) n.w = s0.w + dx;
        if (hmode.includes('n')) {
          n.y = s0.y + dy;
          n.h = s0.h - dy;
        }
        if (hmode.includes('s')) n.h = s0.h + dy;
        if (aspectLock) {
          const nh = (n.w * W) / aspectLock / H;
          if (hmode.includes('n')) n.y = s0.y + s0.h - nh;
          n.h = nh;
        }
      }
      if (hmode === 'move') {
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

  function turn(d: 90 | -90) {
    e.rotate = ((((e.rotate ?? 0) + d) % 360) + 360) % 360 as PhotoEdit['rotate'];
  }

  /* ---------------------------------------------------------- hidden areas */

  function drawStart(ev: PointerEvent) {
    if (tab !== 'hide' || !box) return;
    ev.preventDefault();
    const r = box.getBoundingClientRect();
    const x0 = (ev.clientX - r.left) / r.width;
    const y0 = (ev.clientY - r.top) / r.height;
    const el = ev.currentTarget as HTMLElement;
    el.setPointerCapture(ev.pointerId);
    drawing = { x: x0, y: y0, w: 0, h: 0 };
    const move = (mv: PointerEvent) => {
      const x1 = Math.min(1, Math.max(0, (mv.clientX - r.left) / r.width));
      const y1 = Math.min(1, Math.max(0, (mv.clientY - r.top) / r.height));
      drawing = { x: Math.min(x0, x1), y: Math.min(y0, y1), w: Math.abs(x1 - x0), h: Math.abs(y1 - y0) };
    };
    const up = () => {
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      if (drawing && drawing.w > 0.01 && drawing.h > 0.01) e.hideAreas = [...(e.hideAreas ?? []), { ...drawing, mode: hideMode }];
      drawing = null;
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
  }

  /* ---------------------------------------------------------- save */

  function save() {
    const r4 = (v: number) => Math.round(v * 10000) / 10000;
    const full = c.w > 0.995 && c.h > 0.995;
    const out: PhotoEdit = { ...$state.snapshot(e) };
    out.crop = full || mode === 'collage' ? undefined : { x: r4(c.x), y: r4(c.y), w: r4(c.w), h: r4(c.h) };
    if (out.adjust) {
      const a = out.adjust;
      const neutral = JSON.stringify(fullAdjust(undefined));
      if (JSON.stringify(fullAdjust(a)) === neutral) {
        out.adjust = undefined;
        out.filter = undefined;
      }
    }
    for (const k of ['flipX', 'flipY', 'shadow'] as const) if (!out[k]) delete out[k];
    if (!out.rotate) delete out.rotate;
    if (!out.straighten) delete out.straighten;
    if (!out.hideAreas?.length) delete out.hideAreas;
    if (!out.mask || out.mask === 'rect') delete out.mask;
    if (!out.frame) {
      delete out.frame;
      delete out.frameColor;
    }
    if (!out.border?.width) delete out.border;
    onsave(out);
  }

  function resetAll() {
    c = { x: 0, y: 0, w: 1, h: 1 };
    aspectLock = null;
    e = {};
  }

  const FRAMES: [FrameStyle | '', string][] = [
    ['', 'None'],
    ['polaroid', 'Polaroid'],
    ['instant', 'Instant'],
    ['mat', 'Mat'],
    ['film', 'Film'],
    ['tape', 'Tape'],
  ];
  const resultAspect = $derived(shownW / shownH);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="lt">
  <div class="backdrop" onclick={onclose}>
    <div class="dialog photo" onclick={(ev) => ev.stopPropagation()}>
      <header>
        <h2>{mode === 'copy' ? 'Edit a copy' : kind === 'video' ? 'Edit video' : 'Edit photo'}</h2>
        <div class="tabs">
          {#each tabs as [t, label] (t)}<button class:on={tab === t} onclick={() => (tab = t)}>{label}</button>{/each}
        </div>
        <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
      </header>
      <div class="body split">
        <div class="left">
          {#if tab === 'crop' || tab === 'hide'}
            <!-- the source as it is (crop and hidden areas are in its coordinates) -->
            <div class="stage">
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="imgbox" class:drawing={tab === 'hide'} bind:this={box} style="aspect-ratio:{W}/{H};width:min(100%, calc(62vh * {W / H}))" onpointerdown={drawStart}>
                <img {src} alt="" draggable="false" style:filter={lookFilter(e.adjust) || undefined} />
                {#if tab === 'crop'}
                  <div class="shade" style="clip-path:polygon(0 0,100% 0,100% 100%,0 100%,0 0,{c.x * 100}% {c.y * 100}%,{c.x * 100}% {(c.y + c.h) * 100}%,{(c.x + c.w) * 100}% {(c.y + c.h) * 100}%,{(c.x + c.w) * 100}% {c.y * 100}%,{c.x * 100}% {c.y * 100}%)"></div>
                  <!-- svelte-ignore a11y_no_static_element_interactions -->
                  <div class="rect" style="left:{c.x * 100}%;top:{c.y * 100}%;width:{c.w * 100}%;height:{c.h * 100}%" onpointerdown={(ev) => start(ev, 'move')}>
                    {#each ['nw', 'ne', 'se', 'sw', 'n', 'e', 's', 'w'] as h (h)}
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <span class="hd h-{h}" onpointerdown={(ev) => start(ev, h)}></span>
                    {/each}
                  </div>
                {:else}
                  {#each e.hideAreas ?? [] as a, i (i)}
                    <div class="area" style="left:{a.x * 100}%;top:{a.y * 100}%;width:{a.w * 100}%;height:{a.h * 100}%">
                      <span>{a.mode}</span>
                      <button class="x" title="Remove" onpointerdown={(ev) => ev.stopPropagation()} onclick={() => (e.hideAreas = e.hideAreas!.filter((_, j) => j !== i))}><Icon name="x" size={12} /></button>
                    </div>
                  {/each}
                  {#if drawing}<div class="area new" style="left:{drawing.x * 100}%;top:{drawing.y * 100}%;width:{drawing.w * 100}%;height:{drawing.h * 100}%"></div>{/if}
                {/if}
              </div>
            </div>
          {:else}
            <!-- the result: exactly what the story, the collage or the saved copy shows -->
            <div class="stage result">
              <div class="resultbox" style="width:min(100%, calc(62vh * {framedAspect(resultAspect, tab === 'frame' || mode === 'story' ? e.frame : undefined)}))">
                {#if preview}
                  <LookBox look={{ ...e, crop: undefined, rotate: 0, straighten: 0, flipX: false, flipY: false, frame: mode === 'story' ? e.frame : undefined, mask: mode === 'story' ? e.mask : undefined, border: mode === 'story' ? e.border : undefined, shadow: mode === 'story' ? e.shadow : undefined }} aspect={resultAspect}>
                    {#snippet media(place: string)}<img src={preview} alt="" draggable="false" style={place} />{/snippet}
                  </LookBox>
                {:else}
                  <div class="loading muted">Rendering…</div>
                {/if}
              </div>
            </div>
          {/if}
        </div>

        <aside class="side">
          {#if tab === 'look'}
            <LookPanel adjust={e.adjust} filter={e.filter} thumb={src} video={kind === 'video'} onchange={(a: PhotoAdjust | null, f: string | null) => { e.adjust = a ?? undefined; e.filter = f ?? undefined; }} />
          {:else if tab === 'crop'}
            <div class="group">Crop</div>
            <div class="chips">
              {#each presets as [l, a] (l)}
                <button class="seg" class:on={a === -1 ? Math.abs((aspectLock ?? 0) - W / H) < 0.001 : aspectLock === a} onclick={() => setAspect(a)}>{l}</button>
              {/each}
            </div>
            <span class="muted small">{Math.round(c.w * W)} × {Math.round(c.h * H)} px</span>
            <div class="group">Turn & flip</div>
            <div class="chips">
              <button class="seg" onclick={() => turn(-90)} title="Turn left"><Icon name="undo" size={14} /> Left</button>
              <button class="seg" onclick={() => turn(90)} title="Turn right"><Icon name="redo" size={14} /> Right</button>
              <button class="seg" class:on={e.flipX} onclick={() => (e.flipX = !e.flipX)}><Icon name="arrow-both" size={14} /> Mirror</button>
              <button class="seg" class:on={e.flipY} onclick={() => (e.flipY = !e.flipY)}>Upside down</button>
            </div>
            <label class="sl">
              <span>Straighten <span class="muted">{(e.straighten ?? 0).toFixed(1)}°</span></span>
              <input type="range" min="-45" max="45" step="0.1" value={e.straighten ?? 0} oninput={(ev) => (e.straighten = Number(ev.currentTarget.value))} ondblclick={() => (e.straighten = 0)} />
            </label>
            <p class="muted small">Turns and straightening show on the Look tab. Double-click the slider to reset it.</p>
          {:else if tab === 'frame'}
            <div class="group">Shape</div>
            <div class="chips">
              {#each MASK_SHAPES as s (s.id)}
                <button class="seg" class:on={(e.mask ?? 'rect') === s.id} onclick={() => (e.mask = s.id as MaskShape)}>{s.name}</button>
              {/each}
            </div>
            <div class="group">Frame</div>
            <div class="chips">
              {#each FRAMES as [id, name] (id)}
                <button class="seg" class:on={(e.frame ?? '') === id} onclick={() => (e.frame = id || undefined)}>{name}</button>
              {/each}
            </div>
            {#if e.frame}
              <label class="row">Frame colour <input type="color" value={e.frameColor ?? (e.frame === 'film' ? '#141218' : e.frame === 'tape' ? '#f7a8c4' : '#ffffff')} oninput={(ev) => (e.frameColor = ev.currentTarget.value)} /></label>
            {/if}
            <div class="group">Border & shadow</div>
            <label class="sl">
              <span>Border <span class="muted">{Math.round((e.border?.width ?? 0) * 1000) / 10}%</span></span>
              <input type="range" min="0" max="0.06" step="0.001" value={e.border?.width ?? 0} oninput={(ev) => (e.border = { width: Number(ev.currentTarget.value), color: e.border?.color ?? '#ffffff' })} />
            </label>
            {#if e.border?.width}
              <label class="row">Border colour <input type="color" value={e.border.color} oninput={(ev) => (e.border = { ...e.border!, color: ev.currentTarget.value })} /></label>
            {/if}
            <label class="row"><input type="checkbox" checked={!!e.shadow} onchange={(ev) => (e.shadow = ev.currentTarget.checked)} /> Drop shadow</label>
          {:else}
            <div class="group">Hide faces, plates, screens…</div>
            <p class="muted small">Drag rectangles over what should not be seen. The original file stays as it is.</p>
            <div class="chips">
              <button class="seg" class:on={hideMode === 'blur'} onclick={() => (hideMode = 'blur')}>Blur</button>
              <button class="seg" class:on={hideMode === 'pixelate'} onclick={() => (hideMode = 'pixelate')}>Pixelate</button>
            </div>
            <span class="muted small">{e.hideAreas?.length ?? 0} area{e.hideAreas?.length === 1 ? '' : 's'}</span>
            {#if e.hideAreas?.length}<button class="btn sm" onclick={() => (e.hideAreas = [])}>Remove all</button>{/if}
          {/if}
        </aside>
      </div>
      <div class="acts">
        <button class="btn" onclick={resetAll}>Reset all</button>
        <span class="grow"></span>
        <button class="btn" onclick={onclose}>Cancel</button>
        <button class="btn primary" disabled={busy} onclick={save}>{busy ? 'Saving…' : saveLabel}</button>
      </div>
    </div>
  </div>
</div>

<style>
  .photo {
    width: min(1280px, 100%);
  }
  .photo header {
    gap: 12px;
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin-right: auto;
    padding: 2px;
    border-radius: 8px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }
  .tabs button {
    padding: 4px 12px;
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
  .split {
    display: flex;
    gap: 14px;
  }
  .left {
    flex: 1;
    min-width: 0;
  }
  .side {
    width: 300px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: calc(62vh + 24px);
    overflow-y: auto;
    padding-right: 4px;
  }
  .group {
    margin-top: 6px;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .seg {
    display: inline-flex;
    align-items: center;
    gap: 4px;
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
  .sl {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12.5px;
  }
  .sl > span {
    display: flex;
    justify-content: space-between;
  }
  .sl input {
    width: 100%;
    accent-color: var(--accent);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }
  .small {
    font-size: 11.5px;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 16px 14px;
  }
  .grow {
    flex: 1;
  }
  .stage {
    display: flex;
    justify-content: center;
    align-items: center;
    background: #000;
    border-radius: 8px;
    padding: 10px;
    min-height: 40vh;
  }
  .stage.result {
    background: repeating-conic-gradient(#2a2a2a 0 25%, #333 0 50%) 0 0 / 20px 20px;
  }
  .resultbox {
    position: relative;
    flex: none;
  }
  .loading {
    padding: 40px;
    text-align: center;
  }
  .imgbox {
    position: relative;
    flex: none;
    user-select: none;
    touch-action: none;
  }
  .imgbox.drawing {
    cursor: crosshair;
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
  .area {
    position: absolute;
    border: 2px dashed #fff;
    background: rgba(255, 255, 255, 0.18);
    backdrop-filter: blur(6px);
    color: #fff;
    font-size: 11px;
    text-shadow: 0 1px 2px #000;
  }
  .area span {
    position: absolute;
    left: 4px;
    top: 2px;
  }
  .area.new {
    border-color: var(--accent);
  }
  .area .x {
    position: absolute;
    right: -10px;
    top: -10px;
    width: 20px;
    height: 20px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 50%;
    background: #000c;
    color: #fff;
    cursor: pointer;
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
