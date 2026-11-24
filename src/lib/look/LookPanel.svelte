<script lang="ts">
  /**
   * The photo look controls, shared by the collage Inspector, the story
   * "Edit photo" dialog and "Edit a copy" in the Library: one-click Auto,
   * filter presets (previewed on the photo itself), and every adjustment of
   * the shared pipeline (render/look.ts).
   */
  import { Sparkles } from '@lucide/svelte';
  import type { PhotoAdjust } from '../model/types';
  import { FILTER_PRESETS, presetAdjust } from '../render/adjust';
  import { autoAdjust } from '../render/look';
  import Slider from '../components/ui/Slider.svelte';
  import ColorInput from '../components/ui/ColorInput.svelte';
  import { bakeLook, fullAdjust } from './looks.svelte';

  interface Props {
    adjust: PhotoAdjust | null | undefined;
    filter: string | null | undefined;
    /** A small rendition of the photo, for preset previews and Auto. */
    thumb?: string;
    /** Videos only get the stages an SVG filter can do. */
    video?: boolean;
    /** `key` names the control, so a slider drag can be one undo step. */
    onchange: (adjust: PhotoAdjust | null, filter: string | null, key?: string) => void;
  }
  let { adjust, filter, thumb, video = false, onchange }: Props = $props();

  const a = $derived(fullAdjust(adjust ?? undefined));
  let previews = $state<Record<string, string>>({});
  let open = $state<Record<string, boolean>>({ light: true, color: true });

  const PIXEL_ONLY = new Set(['vibrance', 'clarity', 'sharpen', 'blur', 'grain']);
  type K = Exclude<keyof PhotoAdjust, 'duotone' | 'pop'>;
  const GROUPS: { id: string; name: string; sliders: [K, string, number, number][] }[] = [
    { id: 'light', name: 'Light', sliders: [['exposure', 'Exposure', -1, 1], ['brightness', 'Brightness', -1, 1], ['contrast', 'Contrast', -1, 1], ['highlights', 'Highlights', -1, 1], ['shadows', 'Shadows', -1, 1]] },
    { id: 'color', name: 'Colour', sliders: [['saturation', 'Saturation', -1, 1], ['vibrance', 'Vibrance', -1, 1], ['warmth', 'Warmth', -1, 1], ['tint', 'Tint', -1, 1]] },
    { id: 'effects', name: 'Effects', sliders: [['fade', 'Fade', 0, 1], ['vignette', 'Vignette', 0, 1], ['grain', 'Grain', 0, 1], ['grayscale', 'Black & white', 0, 1], ['sepia', 'Sepia', 0, 1]] },
    { id: 'detail', name: 'Detail', sliders: [['clarity', 'Clarity', -1, 1], ['sharpen', 'Sharpen', 0, 1], ['blur', 'Blur', 0, 1]] },
  ];

  const presets = $derived(video ? FILTER_PRESETS.filter((p) => !Object.keys(p.adjust).some((k) => PIXEL_ONLY.has(k) || k === 'pop')) : FILTER_PRESETS);

  // Preview every preset on the photo itself (tiny renditions, made once).
  $effect(() => {
    const src = thumb;
    if (!src) return;
    let cancelled = false;
    const img = new Image();
    img.decoding = 'async';
    img.src = src;
    void img
      .decode()
      .then(async () => {
        const out: Record<string, string> = {};
        for (const p of FILTER_PRESETS) {
          if (cancelled) return;
          out[p.id] = bakeLook(img, undefined, { adjust: presetAdjust(p.id) }, 140).toDataURL('image/jpeg', 0.8);
          await new Promise((r) => setTimeout(r, 0));
        }
        if (!cancelled) previews = out;
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  });

  function set(k: K, v: number) {
    onchange({ ...a, [k]: v }, null, 'adj-' + k);
  }

  function pick(id: string) {
    onchange(id === 'none' ? null : presetAdjust(id), id === 'none' ? null : id);
  }

  async function auto() {
    if (!thumb) return;
    const img = new Image();
    img.src = thumb;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = Math.min(256, img.naturalWidth);
    c.height = Math.max(1, Math.round((c.width * img.naturalHeight) / img.naturalWidth));
    const g = c.getContext('2d', { willReadFrequently: true })!;
    g.drawImage(img, 0, 0, c.width, c.height);
    const fix = autoAdjust(g.getImageData(0, 0, c.width, c.height).data);
    if (video) delete fix.vibrance;
    onchange({ ...a, ...fix }, null);
  }

  const DUO = [
    ['#2b1055', '#ff9a8b'],
    ['#08233f', '#7de2d1'],
    ['#1b1b1b', '#f5d76e'],
    ['#3a0ca3', '#f72585'],
    ['#0b3d2e', '#f1faee'],
  ];
  const POP_HUES: [number, string][] = [
    [0, 'Red'],
    [30, 'Orange'],
    [55, 'Yellow'],
    [120, 'Green'],
    [200, 'Blue'],
    [300, 'Pink'],
  ];
</script>

<div class="look-panel">
  <div class="row">
    <button class="lp-btn accent" onclick={() => void auto()} disabled={!thumb} title="Fix exposure, contrast and colour cast automatically"><Sparkles size={14} /> Auto</button>
    <button class="lp-btn" onclick={() => onchange(null, null)} disabled={!adjust}>Reset</button>
  </div>

  <div class="presets">
    {#each presets as p (p.id)}
      <button class="preset" class:on={(filter ?? (adjust ? '' : 'none')) === p.id} onclick={() => pick(p.id)} title={p.name}>
        {#if previews[p.id]}<img src={previews[p.id]} alt="" draggable="false" />{:else}<span class="ph"></span>{/if}
        <span>{p.name}</span>
      </button>
    {/each}
  </div>

  {#each GROUPS as g (g.id)}
    {@const sliders = g.sliders.filter(([k]) => !video || !PIXEL_ONLY.has(k))}
    {#if sliders.length}
      <button class="group" onclick={() => (open[g.id] = !open[g.id])}>{open[g.id] ? '▾' : '▸'} {g.name}</button>
      {#if open[g.id]}
        {#each sliders as [k, label, min, max] (k)}
          <Slider {label} value={(a[k] as number) ?? 0} {min} {max} step={0.01} scale={100} unit="" onchange={(v) => set(k, v)} />
        {/each}
      {/if}
    {/if}
  {/each}

  <button class="group" onclick={() => (open.duo = !open.duo)}>{open.duo ? '▾' : '▸'} Duotone</button>
  {#if open.duo}
    <div class="swatches">
      {#each DUO as [d, l] (d + l)}
        <button class="duo" style:background="linear-gradient(90deg, {d}, {l})" title="Duotone" onclick={() => onchange({ ...a, duotone: { dark: d, light: l, amount: a.duotone?.amount || 1 } }, null)}></button>
      {/each}
    </div>
    {#if a.duotone}
      <ColorInput label="Shadows" value={a.duotone.dark} onchange={(c) => onchange({ ...a, duotone: { ...a.duotone!, dark: c } }, null, 'duo-dark')} />
      <ColorInput label="Highlights" value={a.duotone.light} onchange={(c) => onchange({ ...a, duotone: { ...a.duotone!, light: c } }, null, 'duo-light')} />
      <Slider label="Strength" value={a.duotone.amount} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => onchange({ ...a, duotone: { ...a.duotone!, amount: v } }, null, 'duo-amount')} />
      <button class="lp-btn" onclick={() => onchange({ ...a, duotone: null }, null)}>Remove duotone</button>
    {/if}
  {/if}

  {#if !video}
    <button class="group" onclick={() => (open.pop = !open.pop)}>{open.pop ? '▾' : '▸'} Colour pop</button>
    {#if open.pop}
      <p class="note">Keep one colour, the rest turns black & white.</p>
      <div class="swatches">
        {#each POP_HUES as [h, name] (h)}
          <button class="hue" class:on={a.pop?.hue === h && (a.pop?.amount ?? 0) > 0} style:background="hsl({h} 85% 55%)" title={name} onclick={() => onchange({ ...a, pop: { hue: h, range: a.pop?.range ?? 40, amount: 1 } }, null)}></button>
        {/each}
      </div>
      {#if a.pop && a.pop.amount > 0}
        <Slider label="Hue" value={a.pop.hue} min={0} max={359} step={1} unit="°" onchange={(v) => onchange({ ...a, pop: { ...a.pop!, hue: Math.round(v) } }, null, 'pop-hue')} />
        <Slider label="Range" value={a.pop.range} min={10} max={120} step={1} unit="°" onchange={(v) => onchange({ ...a, pop: { ...a.pop!, range: Math.round(v) } }, null, 'pop-range')} />
        <Slider label="Strength" value={a.pop.amount} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => onchange({ ...a, pop: { ...a.pop!, amount: v } }, null, 'pop-amount')} />
      {/if}
    {/if}
  {:else}
    <p class="note">Videos get the colour and light adjustments; detail, grain and colour pop are for photos.</p>
  {/if}
</div>

<style>
  .look-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .lp-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .lp-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .lp-btn.accent {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
  }
  .preset {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
    min-width: 0;
  }
  .preset span:last-child {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .preset img,
  .preset .ph {
    width: 100%;
    aspect-ratio: 1;
    object-fit: cover;
    border-radius: 6px;
    background: var(--panel-3);
    outline: 2px solid transparent;
    outline-offset: 1px;
  }
  .preset.on img,
  .preset.on .ph {
    outline-color: var(--accent);
  }
  .preset.on {
    color: var(--text);
  }
  .group {
    margin-top: 4px;
    padding: 4px 0;
    border: 0;
    border-top: 1px solid var(--border);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    text-align: left;
    cursor: pointer;
  }
  .swatches {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .duo {
    width: 40px;
    height: 22px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    cursor: pointer;
  }
  .hue {
    width: 24px;
    height: 24px;
    border: 2px solid transparent;
    border-radius: 50%;
    cursor: pointer;
  }
  .hue.on {
    border-color: var(--text);
  }
  .note {
    margin: 0;
    color: var(--text-3);
    font-size: 11.5px;
  }
</style>
