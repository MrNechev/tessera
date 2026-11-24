<script lang="ts">
  /**
   * "Cut out": removes a photo's background on this machine (U²-Netp in a
   * Web Worker), shows the original next to the cut-out on a checkerboard,
   * lets you refine the edge (threshold, softness, erase/restore brush) and
   * saves a transparent PNG into Outputs/Cut-outs. Shared by the Library,
   * the collage editor and stories; what happens after saving is up to the
   * caller (`actions` / `onsaved`).
   */
  import { onMount } from 'svelte';
  import { Brush, Eraser, LoaderCircle, MousePointer2, Scissors, Undo2 } from '@lucide/svelte';
  import { call } from '../api/http';
  import { api, isDesktop } from '../api/backend';
  import Modal from '../components/ui/Modal.svelte';
  import Segmented from '../components/ui/Segmented.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import type { AnyCanvas } from '../render/patterns';
  import { canvasOf, loadPhoto, saveCutout, scaleTo, segment } from './engine';
  import { alphaBounds, buildAlpha, composite, DEFAULT_MASK, paintDabs, type Dab } from './mask';
  import { MODEL_SIZE } from './model';
  import { cutoutDialog, type CutoutRequest } from './open.svelte';

  let { request, onclose }: { request: CutoutRequest; onclose: () => void } = $props();

  /** Long side of the on-screen previews (≈ 2× the pane for sharp HiDPI). */
  const PREVIEW = 900;

  const actions = $derived(request.actions?.length ? request.actions : [{ id: 'save', label: 'Save cut-out' }]);
  const name = $derived(request.source.name);

  let stage = $state('Loading the photo');
  let fraction = $state(0);
  let error = $state<string | null>(null);
  let saving = $state(false);

  let threshold = $state(DEFAULT_MASK.threshold);
  let softness = $state(DEFAULT_MASK.softness);
  let tool = $state<'off' | 'erase' | 'restore'>('off');
  let brush = $state(0.03);
  /** Brush strokes, each a list of dabs (undo removes the last stroke). */
  let strokes = $state.raw<Dab[][]>([]);

  let photo: AnyCanvas | null = null;
  let preview = $state.raw<ImageData | null>(null);
  let mask = $state.raw<Float32Array | null>(null);

  let beforeEl: HTMLCanvasElement | undefined = $state();
  let afterEl: HTMLCanvasElement | undefined = $state();
  let cursor = $state<{ x: number; y: number; d: number } | null>(null);

  onMount(() => {
    let cancelled = false;
    void (async () => {
      try {
        const s = request.source;
        const path = 'libraryId' in s ? await call<string>('resolve', { libraryId: s.libraryId, path: s.path }) : s.path;
        const img = await loadPhoto(path);
        if (cancelled) return;
        photo = canvasOf(img);
        const k = Math.min(1, PREVIEW / Math.max(img.width, img.height));
        const pw = Math.max(1, Math.round(img.width * k));
        const ph = Math.max(1, Math.round(img.height * k));
        const small = scaleTo(photo, pw, ph);
        preview = (small.getContext('2d') as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D).getImageData(0, 0, pw, ph);
        fraction = 0.1;
        mask = await segment(photo, (st, f) => {
          stage = st;
          fraction = 0.1 + 0.9 * f;
        });
      } catch (e) {
        if (!cancelled) error = e instanceof Error ? e.message : String(e);
      }
    })();
    return () => (cancelled = true);
  });

  // The original, once.
  $effect(() => {
    if (!beforeEl || !preview) return;
    beforeEl.width = preview.width;
    beforeEl.height = preview.height;
    beforeEl.getContext('2d')?.putImageData(preview, 0, 0);
  });

  // The edge settings only change on slider moves; brush dabs are painted on
  // top of a copy, so drawing stays fast.
  const base = $derived(preview && mask ? buildAlpha(mask, MODEL_SIZE, MODEL_SIZE, preview.width, preview.height, { threshold, softness, feather: DEFAULT_MASK.feather }) : null);

  let frame = 0;
  $effect(() => {
    if (!afterEl || !preview || !base) return;
    const dabs = strokes.flat();
    const p = preview;
    const b = base;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      if (!afterEl) return;
      let alpha = b;
      if (dabs.length) {
        alpha = b.slice();
        paintDabs(alpha, p.width, p.height, dabs);
      }
      afterEl.width = p.width;
      afterEl.height = p.height;
      const out = composite(p.data, alpha, p.width, { x: 0, y: 0, w: p.width, h: p.height });
      afterEl.getContext('2d')?.putImageData(new ImageData(out, p.width, p.height), 0, 0);
    });
  });

  /* ------------------------------------------------------------- brush */

  let drawing = false;
  let last: { x: number; y: number } | null = null;

  function pos(e: PointerEvent): { x: number; y: number } {
    const r = afterEl!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / r.width, y: (e.clientY - r.top) / r.height };
  }

  function moveCursor(e: PointerEvent) {
    if (tool === 'off' || !afterEl) return (cursor = null);
    const r = afterEl.getBoundingClientRect();
    cursor = { x: e.clientX - r.left, y: e.clientY - r.top, d: 2 * brush * Math.max(r.width, r.height) };
  }

  function addDabs(to: { x: number; y: number }) {
    if (tool === 'off' || !preview) return;
    const mode = tool;
    const aspect = preview.width / preview.height;
    // Space dabs a third of the radius apart so fast strokes stay continuous.
    const from = last ?? to;
    const dx = (to.x - from.x) * (aspect >= 1 ? 1 : aspect);
    const dy = (to.y - from.y) * (aspect >= 1 ? 1 / aspect : 1);
    const steps = Math.max(1, Math.ceil(Math.hypot(dx, dy) / (brush / 3)));
    const dabs: Dab[] = [];
    for (let i = last ? 1 : 0; i <= steps; i++) {
      const t = i / steps;
      dabs.push({ x: from.x + (to.x - from.x) * t, y: from.y + (to.y - from.y) * t, r: brush, mode });
    }
    last = to;
    const cur = strokes[strokes.length - 1];
    strokes = [...strokes.slice(0, -1), [...cur, ...dabs]];
  }

  function down(e: PointerEvent) {
    if (tool === 'off' || e.button !== 0 || !base) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    drawing = true;
    last = null;
    strokes = [...strokes, []];
    addDabs(pos(e));
  }

  function move(e: PointerEvent) {
    moveCursor(e);
    if (drawing) addDabs(pos(e));
  }

  function up() {
    drawing = false;
    last = null;
    if (strokes.length && !strokes[strokes.length - 1].length) strokes = strokes.slice(0, -1);
  }

  /* -------------------------------------------------------------- save */

  async function save(action: string) {
    if (!photo || !mask || saving) return;
    saving = true;
    try {
      const w = photo.width;
      const h = photo.height;
      const full = (photo.getContext('2d') as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D).getImageData(0, 0, w, h);
      const alpha = buildAlpha(mask, MODEL_SIZE, MODEL_SIZE, w, h, { threshold, softness, feather: DEFAULT_MASK.feather, dabs: strokes.flat() });
      const rect = alphaBounds(alpha, w, h);
      if (!rect) throw new Error('Nothing is left after the cut-out — lower the threshold or restore some of the subject.');
      const rgba = composite(full.data, alpha, w, rect);
      const res = await saveCutout(rgba, rect.w, rect.h, name);
      cutoutDialog.saved++;
      toasts.push({
        kind: 'success',
        title: 'Cut-out saved',
        message: `Outputs › ${res.ref.path}`,
        action: { label: isDesktop ? 'Show in folder' : 'Download', run: () => void api.revealPath(res.path) },
      });
      onclose();
      request.onsaved?.(res, action);
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not save the cut-out', message: e instanceof Error ? e.message : String(e) });
    } finally {
      saving = false;
    }
  }

  const ready = $derived(!!mask && !error);
</script>

<Modal title="Cut out · {name}" width={920} {onclose}>
  <div class="panes">
    <figure>
      <div class="stage">
        {#if preview}<canvas bind:this={beforeEl} aria-label="Original"></canvas>{:else}<LoaderCircle size={22} class="spin" />{/if}
      </div>
      <figcaption>Original</figcaption>
    </figure>
    <figure>
      <div class="stage checker">
        {#if ready && preview}
          <canvas
            bind:this={afterEl}
            class:painting={tool !== 'off'}
            aria-label="Cut-out"
            onpointerdown={down}
            onpointermove={move}
            onpointerup={up}
            onpointercancel={up}
            onpointerleave={() => (cursor = null)}
          ></canvas>
          {#if cursor}<span class="cursor" style:left="{cursor.x}px" style:top="{cursor.y}px" style:width="{cursor.d}px" style:height="{cursor.d}px"></span>{/if}
        {:else if error}
          <p class="err">{error}</p>
        {:else}
          <div class="working">
            <Scissors size={22} />
            <span>{stage}…</span>
            <div class="bar"><span style:width="{Math.round(fraction * 100)}%"></span></div>
            <span class="hint">Runs on this device — the photo never leaves Tessera.</span>
          </div>
        {/if}
      </div>
      <figcaption>Cut-out</figcaption>
    </figure>
  </div>

  <div class="controls">
    <Slider label="Threshold" value={threshold} min={0.05} max={0.95} step={0.01} scale={100} unit="%" disabled={!ready} onchange={(v) => (threshold = v)} />
    <Slider label="Edge softness" value={softness} min={0} max={1} step={0.01} scale={100} unit="%" disabled={!ready} onchange={(v) => (softness = v)} />
    <div class="brush">
      <span class="label">Touch up</span>
      <Segmented
        full={false}
        value={tool}
        onchange={(v) => (tool = v)}
        options={[
          { value: 'off', label: 'Off', icon: MousePointer2 },
          { value: 'erase', label: 'Erase', icon: Eraser },
          { value: 'restore', label: 'Restore', icon: Brush },
        ]}
      />
      <button class="icon-btn" title="Undo the last stroke" disabled={!strokes.length} onclick={() => (strokes = strokes.slice(0, -1))}><Undo2 size={16} /></button>
    </div>
    <Slider label="Brush size" value={brush} min={0.005} max={0.12} step={0.005} scale={100} unit="%" disabled={!ready || tool === 'off'} onchange={(v) => (brush = v)} />
  </div>

  {#snippet footer()}
    <span class="hint where">Saved as a transparent PNG in Outputs › Cut-outs</span>
    <button class="btn" onclick={onclose}>Cancel</button>
    {#each actions as a, i (a.id)}
      <button class={i === actions.length - 1 ? 'btn primary' : 'btn'} disabled={!ready || saving} onclick={() => void save(a.id)}>
        {#if saving && i === actions.length - 1}<LoaderCircle size={15} class="spin" />{/if}
        {a.label}
      </button>
    {/each}
  {/snippet}
</Modal>

<style>
  .panes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  figure {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  figcaption {
    font-size: 12px;
    color: var(--text-3);
    text-align: center;
  }
  .stage {
    position: relative;
    height: min(52vh, 420px);
    display: grid;
    place-items: center;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--panel-2);
    overflow: hidden;
  }
  canvas {
    max-width: 100%;
    max-height: 100%;
    display: block;
    touch-action: none;
  }
  canvas.painting {
    cursor: none;
  }
  .cursor {
    position: absolute;
    pointer-events: none;
    border-radius: 50%;
    border: 1.5px solid #fff;
    box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.6);
    transform: translate(-50%, -50%);
  }
  .working {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 18px;
    border-radius: 12px;
    background: var(--panel);
    color: var(--text-2);
    text-align: center;
    max-width: 80%;
  }
  .bar {
    width: 200px;
    height: 6px;
    border-radius: 99px;
    background: var(--panel-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .err {
    padding: 18px;
    color: var(--danger);
    background: var(--panel);
    border-radius: 10px;
    max-width: 85%;
  }
  .controls {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 24px;
    margin-top: 16px;
  }
  .brush {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .where {
    margin-right: auto;
    align-self: center;
  }
</style>
