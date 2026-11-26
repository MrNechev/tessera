<script lang="ts" module>
  export interface SizePreset {
    id: string;
    label: string;
    w: number;
    h: number;
    hint?: string;
  }
  export const SIZE_PRESETS: SizePreset[] = [
    { id: 'screen', label: 'Screen', w: 1920, h: 1080, hint: 'Full HD' },
    { id: 'square', label: 'Square', w: 1080, h: 1080, hint: 'Posts' },
    { id: 'story', label: 'Story', w: 1080, h: 1920, hint: 'Phone, upright' },
    { id: 'a4', label: 'A4 portrait', w: 1240, h: 1754, hint: '150 dpi' },
    { id: 'a4l', label: 'A4 landscape', w: 1754, h: 1240, hint: '150 dpi' },
    { id: 'photo', label: 'Photo 4:3', w: 1600, h: 1200 },
    { id: 'icon', label: 'Icon', w: 512, h: 512 },
    { id: 'pixel', label: 'Pixel art', w: 64, h: 64 },
  ];
</script>

<script lang="ts">
  import Modal from '../../components/ui/Modal.svelte';
  import ColorInput from '../../components/ui/ColorInput.svelte';
  import { newPaint, newPaintFromClipboard } from '../session.svelte';
  import { ClipboardPaste } from '@lucide/svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let w = $state(1920);
  let h = $state(1080);
  let preset = $state('screen');
  let bg = $state<'white' | 'transparent' | 'color'>('white');
  let color = $state('#ffe8ef');
  let name = $state('');

  function use(p: SizePreset) {
    preset = p.id;
    w = p.w;
    h = p.h;
    if (p.id === 'pixel' && bg === 'white') bg = 'transparent';
  }

  async function create() {
    onclose();
    await newPaint({ width: w, height: h, background: bg === 'color' ? color : bg, name: name.trim() || undefined });
  }
</script>

<Modal title="New image" width={560} {onclose}>
  <div class="presets">
    {#each SIZE_PRESETS as p (p.id)}
      <button class="pre" class:on={preset === p.id} onclick={() => use(p)}>
        <span class="shape"><span style:aspect-ratio="{p.w} / {p.h}" style:width={p.w >= p.h ? '100%' : 'auto'} style:height={p.h > p.w ? '100%' : 'auto'}></span></span>
        <b>{p.label}</b>
        <span class="hint">{p.w} × {p.h}{p.hint ? ` · ${p.hint}` : ''}</span>
      </button>
    {/each}
  </div>
  <div class="size">
    <label>Width <input class="input" type="number" min="1" max="16384" bind:value={w} oninput={() => (preset = '')} /></label>
    <span class="x">×</span>
    <label>Height <input class="input" type="number" min="1" max="16384" bind:value={h} oninput={() => (preset = '')} /></label>
    <span class="hint">pixels</span>
  </div>
  <div class="bg">
    <span class="label">Background</span>
    <div class="seg">
      <button class:on={bg === 'white'} onclick={() => (bg = 'white')}>White</button>
      <button class:on={bg === 'transparent'} onclick={() => (bg = 'transparent')}>Transparent</button>
      <button class:on={bg === 'color'} onclick={() => (bg = 'color')}>Colour</button>
    </div>
    {#if bg === 'color'}<div class="ci"><ColorInput value={color} onchange={(c) => (color = c)} /></div>{/if}
  </div>
  <label class="nm">Name <input class="input" placeholder="Untitled drawing" bind:value={name} /></label>
  {#snippet footer()}
    <button class="btn ghost" style:margin-right="auto" onclick={() => { onclose(); void newPaintFromClipboard(); }}><ClipboardPaste size={15} /> From clipboard</button>
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!(w > 0 && h > 0 && w * h <= 16384 * 16384)} onclick={() => void create()}>Create</button>
  {/snippet}
</Modal>

<style>
  .presets {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .pre {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    padding: 10px 6px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--panel-2);
    cursor: pointer;
    text-align: center;
  }
  .pre:hover {
    border-color: var(--border-strong);
  }
  .pre.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .pre .hint {
    font-size: 11px;
  }
  .shape {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    margin-bottom: 4px;
  }
  .shape span {
    display: block;
    border: 2px solid var(--text-3);
    border-radius: 3px;
  }
  .size {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    margin-top: 16px;
  }
  .size label,
  .nm {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .size input {
    width: 110px;
  }
  .x {
    padding-bottom: 8px;
    color: var(--text-3);
  }
  .size .hint {
    padding-bottom: 8px;
  }
  .bg {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
  }
  .ci {
    width: 160px;
  }
  .nm {
    margin-top: 14px;
  }
  .seg {
    display: flex;
    padding: 2px;
    gap: 2px;
    border-radius: 8px;
    background: var(--bg-2);
    border: 1px solid var(--border);
  }
  .seg button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .seg button.on {
    background: var(--panel-3);
    color: var(--text);
    font-weight: 600;
  }
</style>
