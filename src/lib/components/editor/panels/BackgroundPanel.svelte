<script lang="ts">
  import { X } from '@lucide/svelte';
  import { fileUrl } from '../../../api/backend';
  import { editor } from '../../../editor/editor.svelte';
  import type { Background, Fill, PageFrameStyle, PatternId, TextureId } from '../../../model/types';
  import { cssForFill } from '../../../render/fills';
  import { PAGE_FRAMES } from '../../../render/pageFrames';
  import { PATTERNS, TEXTURES, patternTile } from '../../../render/patterns';
  import { library } from '../../../stores/library.svelte';
  import ColorInput from '../../ui/ColorInput.svelte';
  import Section from '../../ui/Section.svelte';
  import Slider from '../../ui/Slider.svelte';

  const bg = $derived(editor.doc.background);
  const frame = $derived(editor.doc.frame);

  const SOLIDS = ['#ffffff', '#f6efe6', '#fbf3ea', '#ffe3ec', '#ffd6e5', '#fff4d6', '#e7f7d4', '#d9f1ff', '#e9d8ff', '#1d1b26', '#141218', '#0a1230', '#1b4d32', '#8e1414', '#3d0a1b', '#c99a6b'];

  const GRADIENTS: Fill[] = [
    { kind: 'linear', angle: 135, stops: [{ offset: 0, color: '#ffe3ec' }, { offset: 1, color: '#ffc2d4' }] },
    { kind: 'linear', angle: 160, stops: [{ offset: 0, color: '#ff5f7e' }, { offset: 1, color: '#d81b60' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#ffd36e' }, { offset: 1, color: '#ff7a59' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#7fd3ff' }, { offset: 1, color: '#ffe9b3' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#e8f6ff' }, { offset: 1, color: '#a9d1f2' }] },
    { kind: 'linear', angle: 135, stops: [{ offset: 0, color: '#e9d8ff' }, { offset: 1, color: '#ffd6f0' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#eef7f3' }, { offset: 1, color: '#dff0ff' }] },
    { kind: 'linear', angle: 135, stops: [{ offset: 0, color: '#f7e2c4' }, { offset: 1, color: '#efc58f' }] },
    { kind: 'radial', stops: [{ offset: 0, color: '#c62828' }, { offset: 1, color: '#8e1414' }] },
    { kind: 'radial', stops: [{ offset: 0, color: '#1d2b5c' }, { offset: 1, color: '#0a1230' }] },
    { kind: 'radial', stops: [{ offset: 0, color: '#6b1530' }, { offset: 1, color: '#3d0a1b' }] },
    { kind: 'radial', stops: [{ offset: 0, color: '#3a1d4f' }, { offset: 1, color: '#120a1a' }] },
    { kind: 'linear', angle: 45, stops: [{ offset: 0, color: '#8b5cf6' }, { offset: 0.5, color: '#ff5d8f' }, { offset: 1, color: '#ffc145' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#1b4d32' }, { offset: 1, color: '#123824' }] },
    { kind: 'linear', angle: 120, stops: [{ offset: 0, color: '#232526' }, { offset: 1, color: '#414345' }] },
    { kind: 'linear', angle: 90, stops: [{ offset: 0, color: '#fffbe6' }, { offset: 1, color: '#e7f7d4' }] },
  ];

  function set(fn: (b: Background) => void, key?: string) {
    editor.update((d) => fn(d.background as Background), { key });
  }

  const solidColor = $derived(bg.fill.kind === 'solid' ? bg.fill.color : (bg.fill.stops[0]?.color ?? '#ffffff'));

  const patternPreviews = $derived.by(() => {
    const color = bg.pattern?.color ?? '#8a8299';
    const out: Record<string, string> = {};
    for (const p of PATTERNS) {
      const tile = patternTile(p.id, color === 'multi' ? 'multi' : color, 48);
      const c = document.createElement('canvas');
      c.width = 48;
      c.height = 48;
      c.getContext('2d')!.drawImage(tile as CanvasImageSource, 0, 0);
      out[p.id] = c.toDataURL();
    }
    return out;
  });

  function setPattern(id: PatternId | null) {
    set((b) => {
      b.pattern = id ? { id, color: b.pattern?.color ?? '#ffffff', opacity: b.pattern?.opacity ?? 0.35, scale: b.pattern?.scale ?? 0.07 } : null;
    });
  }

  function setTexture(id: TextureId | null) {
    set((b) => {
      b.texture = id ? { id, intensity: b.texture?.intensity ?? 0.8 } : null;
    });
  }

  function setFrame(style: PageFrameStyle) {
    const def = PAGE_FRAMES.find((f) => f.id === style)!;
    editor.update((d) => {
      d.frame = style === 'none' ? null : { style, color: def.color, width: def.width, inset: def.inset };
    });
  }
</script>

<div class="head">
  <h3>Background</h3>
</div>

<Section title="Colour">
  <div class="swatches">
    {#each SOLIDS as c (c)}
      <button class="sw" class:on={bg.fill.kind === 'solid' && bg.fill.color === c} style:background={c} aria-label={c} onclick={() => set((b) => (b.fill = { kind: 'solid', color: c }))}></button>
    {/each}
  </div>
  <ColorInput label="Custom colour" value={solidColor} onchange={(c) => set((b) => (b.fill = { kind: 'solid', color: c }), 'bg-color')} />
</Section>

<Section title="Gradient">
  <div class="swatches big">
    {#each GRADIENTS as g, i (i)}
      <button class="sw" style:background={cssForFill(g)} aria-label="Gradient {i + 1}" onclick={() => set((b) => (b.fill = structuredClone(g)))}></button>
    {/each}
  </div>
  {#if bg.fill.kind !== 'solid'}
    <div class="row">
      {#each bg.fill.stops as s, i (i)}
        <div class="grow">
          <ColorInput value={s.color} onchange={(c) => set((b) => {
            if (b.fill.kind !== 'solid') b.fill.stops[i].color = c;
          }, `bg-stop-${i}`)} />
        </div>
      {/each}
    </div>
    {#if bg.fill.kind === 'linear'}
      <Slider label="Angle" value={bg.fill.angle} min={0} max={360} unit="°" onchange={(v) => set((b) => {
        if (b.fill.kind === 'linear') b.fill.angle = v;
      }, 'bg-angle')} />
    {/if}
  {/if}
</Section>

<Section title="Pattern">
  <div class="tiles">
    <button class="tile none" class:on={!bg.pattern} onclick={() => setPattern(null)} title="No pattern"><X size={16} /></button>
    {#each PATTERNS as p (p.id)}
      <button class="tile" class:on={bg.pattern?.id === p.id} title={p.name} onclick={() => setPattern(p.id)} style:background-image="url({patternPreviews[p.id]})"></button>
    {/each}
  </div>
  {#if bg.pattern}
    <ColorInput label="Pattern colour" value={bg.pattern.color === 'multi' ? '#ff5d8f' : bg.pattern.color} onchange={(c) => set((b) => {
      if (b.pattern) b.pattern.color = c;
    }, 'bg-pattern-color')} />
    {#if bg.pattern.id === 'confetti'}
      <button class="btn small" onclick={() => set((b) => {
        if (b.pattern) b.pattern.color = 'multi';
      })}>Multicolour confetti</button>
    {/if}
    <Slider label="Opacity" value={bg.pattern.opacity} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.pattern) b.pattern.opacity = v;
    }, 'bg-pattern-op')} />
    <Slider label="Size" value={bg.pattern.scale} min={0.02} max={0.25} step={0.002} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.pattern) b.pattern.scale = v;
    }, 'bg-pattern-scale')} />
  {/if}
</Section>

<Section title="Texture" open={false}>
  <div class="tex">
    <button class="btn small" class:active={!bg.texture} onclick={() => setTexture(null)}>None</button>
    {#each TEXTURES as t (t.id)}
      <button class="btn small" class:active={bg.texture?.id === t.id} onclick={() => setTexture(t.id)}>{t.name}</button>
    {/each}
  </div>
  {#if bg.texture}
    <Slider label="Strength" value={bg.texture.intensity} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.texture) b.texture.intensity = v;
    }, 'bg-tex')} />
  {/if}
  <Slider label="Vignette" value={bg.vignette} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => (b.vignette = v), 'bg-vignette')} />
</Section>

<Section title="Photo background" open={!!bg.image}>
  {#if library.items.length}
    <div class="photos">
      {#each library.items.slice(0, 24) as item (item.id)}
        <button
          class="ph"
          class:on={bg.image?.asset === item.id}
          title={item.name}
          onclick={() => editor.update((d) => {
            editor.ensureAsset(d, item.id);
            d.background.image = { asset: item.id, blur: d.background.image?.blur ?? 0.3, dim: d.background.image?.dim ?? 0.1, opacity: 1 };
          })}
        ><img src={fileUrl(item.thumb)} alt="" loading="lazy" /></button>
      {/each}
    </div>
  {:else}
    <p class="hint">Import photos first to use one as the background.</p>
  {/if}
  {#if bg.image}
    <Slider label="Blur" value={bg.image.blur} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.image) b.image.blur = v;
    }, 'bg-img-blur')} />
    <Slider label="Darken" value={bg.image.dim} min={0} max={0.8} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.image) b.image.dim = v;
    }, 'bg-img-dim')} />
    <Slider label="Opacity" value={bg.image.opacity} min={0} max={1} step={0.01} scale={100} unit="%" onchange={(v) => set((b) => {
      if (b.image) b.image.opacity = v;
    }, 'bg-img-op')} />
    <button class="btn small" onclick={() => set((b) => (b.image = null))}>Remove photo background</button>
  {/if}
</Section>

<Section title="Page border">
  <div class="frames">
    {#each PAGE_FRAMES as f (f.id)}
      <button class="btn small" class:active={(frame?.style ?? 'none') === f.id} onclick={() => setFrame(f.id)}>{f.name}</button>
    {/each}
  </div>
  {#if frame}
    <ColorInput label="Border colour" value={frame.color} onchange={(c) => editor.update((d) => {
      if (d.frame) d.frame.color = c;
    }, { key: 'frame-color' })} />
    <Slider label="Thickness" value={frame.width} min={0.001} max={0.04} step={0.0005} scale={100} decimals={1} unit="%" onchange={(v) => editor.update((d) => {
      if (d.frame) d.frame.width = v;
    }, { key: 'frame-width' })} />
    <Slider label="Distance from edge" value={frame.inset} min={0} max={0.1} step={0.001} scale={100} decimals={1} unit="%" onchange={(v) => editor.update((d) => {
      if (d.frame) d.frame.inset = v;
    }, { key: 'frame-inset' })} />
  {/if}
</Section>

<style>
  .head {
    padding: 16px 14px 4px;
  }
  h3 {
    font-size: 15px;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 5px;
  }
  .swatches.big {
    grid-template-columns: repeat(4, 1fr);
  }
  .sw {
    aspect-ratio: 1;
    border-radius: 7px;
    border: 0;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.18);
  }
  .swatches.big .sw {
    aspect-ratio: 1.4;
  }
  .sw:hover {
    transform: scale(1.06);
  }
  .sw.on {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--accent);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 5px;
  }
  .tile {
    aspect-ratio: 1;
    border-radius: 7px;
    border: 1px solid var(--border-strong);
    background-color: var(--panel-3);
    background-size: 24px 24px;
    cursor: pointer;
    color: var(--text-3);
    display: grid;
    place-items: center;
  }
  .tile.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  .tex,
  .frames {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  :global(.btn.active) {
    border-color: var(--accent);
    color: var(--accent);
    background: var(--accent-soft);
  }
  .photos {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 5px;
  }
  .ph {
    aspect-ratio: 1;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 7px;
    overflow: hidden;
    cursor: pointer;
    background: var(--panel-3);
  }
  .ph.on {
    border-color: var(--accent);
  }
  .ph img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
</style>
