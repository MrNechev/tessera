<script lang="ts" module>
  export const PALETTE = [
    '#ffffff', '#f4f1ea', '#e9e4f0', '#c9c3d6', '#8a8299', '#4d4660', '#2b2340', '#000000',
    '#ff5d8f', '#ff4f81', '#d81b60', '#b23a48', '#ff8a5c', '#ff6b4a', '#ffc145', '#f3d27a',
    '#c9a227', '#8a3b12', '#34d399', '#1b9e77', '#1b4d32', '#2dd4bf', '#7cc4ff', '#3d7bff',
    '#1e3a5f', '#0a1230', '#8b5cf6', '#7b2ff7', '#c084fc', '#ffd6e5', '#d9f1ff', '#fff4d6',
  ];
  let recent = $state<string[]>([]);
</script>

<script lang="ts">
  interface Props {
    value: string;
    label?: string;
    alpha?: boolean;
    onchange: (v: string) => void;
  }
  let { value, label, alpha = false, onchange }: Props = $props();
  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();

  function parse(v: string): { hex: string; a: number } {
    const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(v);
    if (m) return { hex: `#${m[1]}`, a: m[2] ? parseInt(m[2], 16) / 255 : 1 };
    const r = /^rgba?\(([^)]+)\)$/i.exec(v);
    if (r) {
      const [rr, gg, bb, aa] = r[1].split(',').map((s) => parseFloat(s));
      const h = (n: number) => Math.round(n).toString(16).padStart(2, '0');
      return { hex: `#${h(rr)}${h(gg)}${h(bb)}`, a: Number.isFinite(aa) ? aa : 1 };
    }
    return { hex: '#000000', a: 1 };
  }
  const parsed = $derived(parse(value));
  let hexText = $state('');
  $effect.pre(() => {
    hexText = parsed.hex;
  });

  function emit(hex: string, a = parsed.a) {
    const out = alpha && a < 1 ? `${hex}${Math.round(a * 255).toString(16).padStart(2, '0')}` : hex;
    onchange(out);
  }

  function pick(hex: string) {
    emit(hex);
    recent = [hex, ...recent.filter((c) => c !== hex)].slice(0, 8);
  }

  function onDocDown(e: MouseEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onmousedown={onDocDown} />

<div class="color" bind:this={root}>
  <button type="button" class="swatch-btn" onclick={() => (open = !open)} title={label ?? 'Colour'}>
    <span class="swatch checker"><span style:background={value}></span></span>
    {#if label}<span class="lbl">{label}</span>{/if}
    <span class="hex">{parsed.hex.toUpperCase()}{alpha && parsed.a < 1 ? ` · ${Math.round(parsed.a * 100)}%` : ''}</span>
  </button>
  {#if open}
    <div class="pop card">
      <div class="grid">
        {#each PALETTE as c (c)}
          <button type="button" class="chip" class:sel={c === parsed.hex} style:background={c} title={c} onclick={() => pick(c)} aria-label={c}></button>
        {/each}
      </div>
      {#if recent.length}
        <div class="hint">Recent</div>
        <div class="grid">
          {#each recent as c (c)}
            <button type="button" class="chip" style:background={c} title={c} onclick={() => pick(c)} aria-label={c}></button>
          {/each}
        </div>
      {/if}
      <div class="row">
        <input
          class="input grow"
          value={hexText}
          oninput={(e) => {
            hexText = (e.target as HTMLInputElement).value;
            if (/^#[0-9a-f]{6}$/i.test(hexText)) emit(hexText.toLowerCase());
          }}
        />
        <label class="native" title="Custom colour">
          <input type="color" value={parsed.hex} oninput={(e) => emit((e.target as HTMLInputElement).value)} onchange={(e) => pick((e.target as HTMLInputElement).value)} />
          <span style:background={parsed.hex}></span>
        </label>
      </div>
      {#if alpha}
        <div class="row">
          <span class="hint">Opacity</span>
          <input
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={parsed.a}
            style:--fill="{parsed.a * 100}%"
            oninput={(e) => emit(parsed.hex, parseFloat((e.target as HTMLInputElement).value))}
          />
        </div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .color {
    position: relative;
  }
  .swatch-btn {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 32px;
    padding: 0 8px 0 4px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    cursor: pointer;
  }
  .swatch-btn:hover {
    border-color: var(--text-3);
  }
  .swatch {
    width: 24px;
    height: 24px;
    border-radius: 5px;
    overflow: hidden;
    flex: none;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
  }
  .swatch span {
    display: block;
    width: 100%;
    height: 100%;
  }
  .lbl {
    flex: 1;
    text-align: left;
    color: var(--text-2);
    font-size: 12px;
  }
  .hex {
    font-size: 11.5px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
    margin-left: auto;
  }
  .pop {
    position: absolute;
    z-index: 50;
    top: 36px;
    left: 0;
    width: 236px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: var(--shadow);
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
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.18);
  }
  .chip:hover {
    transform: scale(1.12);
  }
  .chip.sel {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--accent);
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
</style>
