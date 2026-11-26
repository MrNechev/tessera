<script lang="ts">
  /** Colours: primary/secondary (click to choose which one the palette sets), hex, opacity, palette and recent colours. */
  import { ArrowLeftRight, Pipette } from '@lucide/svelte';
  import { PALETTE } from '../../components/ui/ColorInput.svelte';
  import { setTool } from '../actions.svelte';
  import { paint, rgbaOf, hexOf } from '../state.svelte';

  let target = $state<'primary' | 'secondary'>('primary');
  const value = $derived(target === 'primary' ? paint.primary : paint.secondary);
  const rgba = $derived(rgbaOf(value));
  const hex6 = $derived(value.slice(0, 7));
  let hexText = $state('');
  $effect.pre(() => {
    hexText = hex6.toUpperCase();
  });

  function set(c: string, remember = true) {
    if (target === 'primary') {
      if (remember) paint.setPrimary(c);
      else paint.primary = c;
    } else if (remember) paint.setSecondary(c);
    else paint.secondary = c;
  }

  function pick(hex: string) {
    const a = rgba[3];
    const [r, g, b] = rgbaOf(hex);
    set(hexOf(r, g, b, a));
  }

  function alpha(v: number) {
    const [r, g, b] = rgba;
    set(hexOf(r, g, b, Math.round(v * 255)), false);
  }
</script>

<div class="paint-colors">
  <div class="top">
    <button class="big checker" class:on={target === 'primary'} title="Primary colour (left button)" aria-label="Edit the primary colour" onclick={() => (target = 'primary')}><span style:background={paint.primary}></span></button>
    <button class="icon-btn small" title="Swap (X)" aria-label="Swap colours" onclick={() => paint.swap()}><ArrowLeftRight size={14} /></button>
    <button class="big checker" class:on={target === 'secondary'} title="Secondary colour (right button)" aria-label="Edit the secondary colour" onclick={() => (target = 'secondary')}><span style:background={paint.secondary}></span></button>
    <span class="spacer"></span>
    <button class="icon-btn small" title="Eyedropper (I)" aria-label="Eyedropper" onclick={() => setTool('picker')}><Pipette size={15} /></button>
  </div>
  <div class="row">
    <input
      class="input hex"
      aria-label="Hex colour"
      value={hexText}
      maxlength="7"
      oninput={(e) => {
        hexText = e.currentTarget.value;
        const t = hexText.startsWith('#') ? hexText : `#${hexText}`;
        if (/^#[0-9a-f]{6}$/i.test(t)) pick(t.toLowerCase());
      }}
    />
    <label class="native" title="Any colour">
      <input type="color" value={hex6} oninput={(e) => pick(e.currentTarget.value)} />
      <span style:background={hex6}></span>
    </label>
  </div>
  <label class="op">
    <span>Opacity</span>
    <input type="range" min="0" max="1" step="0.01" value={rgba[3] / 255} style:--fill="{(rgba[3] / 255) * 100}%" oninput={(e) => alpha(+e.currentTarget.value)} onchange={() => paint.persist()} />
    <span class="v">{Math.round((rgba[3] / 255) * 100)}%</span>
  </label>
  <div class="grid">
    {#each PALETTE as c (c)}
      <button class="chip" class:sel={c === hex6} style:background={c} title={c} aria-label={c} onclick={() => pick(c)} oncontextmenu={(e) => { e.preventDefault(); const t = target; target = 'secondary'; pick(c); target = t; }}></button>
    {/each}
  </div>
  {#if paint.recent.length}
    <div class="lbl">Recent</div>
    <div class="grid">
      {#each paint.recent as c (c)}
        <button class="chip" style:background={c} title={c} aria-label={c} onclick={() => pick(c)}></button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .paint-colors {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .big {
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: 8px;
    border: 2px solid var(--panel);
    box-shadow: 0 0 0 1px var(--border-strong);
    overflow: hidden;
    cursor: pointer;
  }
  .big.on {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .big span {
    display: block;
    width: 100%;
    height: 100%;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .hex {
    flex: 1;
    min-width: 0;
    font-variant-numeric: tabular-nums;
    text-transform: uppercase;
  }
  .native {
    position: relative;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-sm);
    overflow: hidden;
    cursor: pointer;
    flex: none;
    border: 1px solid var(--border-strong);
    background: conic-gradient(red, yellow, lime, cyan, blue, magenta, red);
  }
  .native span {
    position: absolute;
    inset: 7px;
    border-radius: 4px;
    box-shadow: 0 0 0 2px #fff;
  }
  .native input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  .op {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--text-2);
  }
  .op input {
    flex: 1;
    min-width: 0;
  }
  .v {
    width: 36px;
    text-align: right;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 4px;
  }
  .chip {
    aspect-ratio: 1;
    border-radius: 5px;
    border: 0;
    padding: 0;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.18);
  }
  .chip:hover {
    transform: scale(1.12);
  }
  .chip.sel {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--accent);
  }
  .lbl {
    font-size: 11px;
    color: var(--text-3);
    font-weight: 600;
  }
</style>
