<script lang="ts">
  // Text style of canvas notes and free text: Tessera's presets (the collage Text panel's
  // list) plus the single properties — font, weight, fill, outline, shadow, caps,
  // letter-spacing and a pill behind the text. Every change hands a fresh plain object
  // (or undefined = the plain UI font) to `onchange`.
  import { FONT_CATEGORIES, FONTS } from '../../assets/fonts';
  import { lookCss, TEXT_PRESETS, themedLook, type TextLook } from '../../text/styles';
  import Icon from './Icon.svelte';

  let { look, onchange, onclose }: { look: TextLook | undefined; onchange: (look: TextLook | undefined) => void; onclose: () => void } = $props();

  // the canvas has no arched text; everything else carries over
  const presets = TEXT_PRESETS.filter((p) => !p.curve && p.id !== 'body');
  const base = (): TextLook => (look ? $state.snapshot(look) : { font: 'inter' });

  function set(patch: Partial<TextLook>) {
    const next = { ...base(), ...patch };
    for (const k of Object.keys(next) as (keyof TextLook)[]) if (next[k] === undefined || next[k] === null || next[k] === false || next[k] === 0) delete next[k];
    onchange(next);
  }

  const fontsBy = FONT_CATEGORIES.map((c) => ({ ...c, fonts: FONTS.filter((f) => f.category === c.id) }));
  const bold = $derived((look?.weight ?? 400) >= 600);
</script>

<div class="cts">
  <header>
    <b>Text style</b>
    <span class="grow"></span>
    <button class="icon-btn" title="Close" onclick={onclose}><Icon name="x" /></button>
  </header>

  <div class="presets">
    <button class="pre" class:on={!look} onclick={() => onchange(undefined)}><span class="plain">Plain</span></button>
    {#each presets as p (p.id)}
      {@const l = themedLook(p)}
      <button class="pre" title={p.label} onclick={() => onchange(l)}>
        <span class="sample" class:pill={!!l.background} style={lookCss(l, { pill: true })}>{p.label}</span>
      </button>
    {/each}
  </div>

  <div class="row">
    <span class="lbl">Font</span>
    <select class="input" value={look?.font ?? ''} onchange={(e) => set({ font: e.currentTarget.value || 'inter' })}>
      {#if !look}<option value="">Interface font</option>{/if}
      {#each fontsBy as c (c.id)}
        <optgroup label={c.name}>
          {#each c.fonts as f (f.id)}<option value={f.id}>{f.family}</option>{/each}
        </optgroup>
      {/each}
    </select>
  </div>

  <div class="row">
    <button class="icon-btn" class:on={bold} title="Bold" onclick={() => set({ weight: bold ? undefined : 700 })}><Icon name="bold" /></button>
    <button class="icon-btn" class:on={!!look?.italic} title="Italic" onclick={() => set({ italic: !look?.italic })}><Icon name="italic" /></button>
    <button class="icon-btn caps" class:on={!!look?.uppercase} title="Capitals" onclick={() => set({ uppercase: !look?.uppercase })}>AB</button>
    <span class="grow"></span>
    <span class="lbl">Colour</span>
    <input type="color" title="Text colour" value={look?.fill ?? '#1d1b26'} oninput={(e) => set({ fill: e.currentTarget.value })} />
    <button class="mini" title="Theme colour (readable in light and dark mode)" disabled={!look?.fill} onclick={() => set({ fill: undefined })}>Auto</button>
  </div>

  <div class="row">
    <label class="cb"><input type="checkbox" checked={!!look?.stroke?.width} onchange={(e) => set({ stroke: e.currentTarget.checked ? { width: 0.06, color: '#000000' } : undefined })} /> Outline</label>
    {#if look?.stroke?.width}
      {@const s = look.stroke}
      <input type="range" min="0.01" max="0.15" step="0.005" value={s.width} title="Outline width" oninput={(e) => set({ stroke: { color: s.color, width: +e.currentTarget.value } })} />
      <input type="color" title="Outline colour" value={s.color} oninput={(e) => set({ stroke: { width: s.width, color: e.currentTarget.value } })} />
    {/if}
  </div>

  <div class="row">
    <label class="cb"><input type="checkbox" checked={!!look?.shadow} onchange={(e) => set({ shadow: e.currentTarget.checked ? { x: 0.04, y: 0.06, blur: 0.12, color: '#00000088' } : undefined })} /> Shadow</label>
    {#if look?.shadow}
      {@const sh = look.shadow}
      <input type="range" min="0" max="0.4" step="0.01" value={sh.blur} title="Shadow softness" oninput={(e) => set({ shadow: { ...sh, blur: +e.currentTarget.value } })} />
      <input type="color" title="Shadow colour" value={sh.color.slice(0, 7)} oninput={(e) => set({ shadow: { ...sh, color: e.currentTarget.value + '99' } })} />
    {/if}
  </div>

  <div class="row">
    <label class="cb"><input type="checkbox" checked={!!look?.background} onchange={(e) => set({ background: e.currentTarget.checked ? { color: '#ff5d8f', padding: 0.45, radius: 1 } : undefined })} /> Pill</label>
    {#if look?.background}
      {@const b = look.background}
      <input type="color" title="Pill colour" value={b.color} oninput={(e) => set({ background: { ...b, color: e.currentTarget.value } })} />
      <span class="muted">free text only</span>
    {/if}
  </div>

  <div class="row">
    <span class="lbl">Spacing</span>
    <input class="grow" type="range" min="-0.05" max="0.4" step="0.01" value={look?.letterSpacing ?? 0} oninput={(e) => set({ letterSpacing: +e.currentTarget.value })} />
  </div>
</div>

<style>
  .cts {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
  }
  header {
    display: flex;
    align-items: center;
  }
  .grow {
    flex: 1;
  }
  .presets {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 4px;
  }
  .pre {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 40px;
    padding: 0 6px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-2);
    cursor: pointer;
    overflow: hidden;
    color: var(--text);
  }
  .pre:hover,
  .pre.on {
    border-color: var(--accent);
  }
  .sample {
    font-size: 19px;
    line-height: 1.1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sample.pill {
    font-size: 12px;
  }
  .plain {
    font: 500 14px var(--ui);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 30px;
  }
  .row select {
    flex: 1;
    min-width: 0;
  }
  .lbl {
    color: var(--text-3);
    font-size: 12px;
  }
  .cb {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 76px;
  }
  .row input[type='range'] {
    flex: 1;
    min-width: 0;
  }
  .row input[type='color'] {
    width: 28px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    background: none;
    cursor: pointer;
  }
  .caps {
    font: 700 11px var(--ui);
  }
  .mini {
    border: 1px solid var(--line-strong);
    background: none;
    border-radius: 5px;
    padding: 2px 6px;
    font-size: 11px;
    cursor: pointer;
    color: var(--text-2);
  }
  .mini:disabled {
    opacity: 0.4;
    cursor: default;
  }
</style>
