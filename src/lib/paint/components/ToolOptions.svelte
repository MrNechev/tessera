<script lang="ts">
  /** The options of the current tool (size, opacity, tolerance, shape style…), above the canvas. */
  import { Check, Scan, X } from '@lucide/svelte';
  import { TEXT_PRESETS } from '../../text/styles';
  import { applyCrop, selectSubject, setTool } from '../actions.svelte';
  import { paint, type ShapeKind, type ShapeStyle } from '../state.svelte';
  import { SIZED } from '../tools.svelte';

  const o = $derived(paint.opts);
  const d = $derived(paint.doc);

  function num(label: string, value: number, min: number, max: number, set: (v: number) => void, unit = '', step = 1) {
    return { label, value, min, max, set, unit, step };
  }

  const sizeCtl = $derived(SIZED.includes(paint.tool) ? num('Size', paint.size, 1, paint.tool === 'pencil' ? 64 : 500, (v) => (paint.size = v), ' px') : null);

  function persist() {
    paint.persist();
  }

  const SHAPES: { id: ShapeKind; label: string }[] = [
    { id: 'rect', label: 'Rectangle' },
    { id: 'ellipse', label: 'Ellipse' },
    { id: 'line', label: 'Line' },
    { id: 'arrow', label: 'Arrow' },
  ];
  const STYLES: { id: ShapeStyle; label: string; title: string }[] = [
    { id: 'outline', label: 'Outline', title: 'Outline in the primary colour' },
    { id: 'fill', label: 'Filled', title: 'Filled with the primary colour' },
    { id: 'both', label: 'Both', title: 'Primary outline, secondary fill' },
  ];
  const MODES = [
    { id: 'replace', label: 'New' },
    { id: 'add', label: 'Add', title: 'Shift' },
    { id: 'subtract', label: 'Subtract', title: 'Alt' },
    { id: 'intersect', label: 'Intersect', title: 'Shift+Alt' },
  ] as const;
</script>

<div class="opts" role="toolbar" aria-label="Tool options">
  {#if sizeCtl}
    <label class="ctl">
      <span>{sizeCtl.label}</span>
      <input type="range" min={sizeCtl.min} max={sizeCtl.max} value={sizeCtl.value} style:--fill="{((sizeCtl.value - sizeCtl.min) / (sizeCtl.max - sizeCtl.min)) * 100}%" oninput={(e) => sizeCtl.set(+e.currentTarget.value)} />
      <input class="n" type="number" min={sizeCtl.min} max={sizeCtl.max} value={sizeCtl.value} onchange={(e) => sizeCtl.set(+e.currentTarget.value)} />
    </label>
  {/if}

  {#if ['brush', 'pencil', 'eraser', 'fill', 'shape', 'clone'].includes(paint.tool)}
    <label class="ctl">
      <span>Opacity</span>
      <input type="range" min="0.05" max="1" step="0.01" value={o.opacity} style:--fill="{o.opacity * 100}%" oninput={(e) => (paint.opts.opacity = +e.currentTarget.value)} onchange={persist} />
      <span class="v">{Math.round(o.opacity * 100)}%</span>
    </label>
  {/if}

  {#if paint.tool === 'brush' || paint.tool === 'eraser'}
    <label class="ctl">
      <span>Smoothing</span>
      <input type="range" min="0" max="1" step="0.05" value={o.smoothing} style:--fill="{o.smoothing * 100}%" oninput={(e) => (paint.opts.smoothing = +e.currentTarget.value)} onchange={persist} />
    </label>
    <span class="hint">Pen pressure is used when you draw with a pen.</span>
  {/if}

  {#if paint.tool === 'blur' || paint.tool === 'smudge'}
    <div class="seg">
      <button class:on={paint.tool === 'blur'} onclick={() => setTool('blur')}>Blur</button>
      <button class:on={paint.tool === 'smudge'} onclick={() => setTool('smudge')}>Smudge</button>
    </div>
    <label class="ctl">
      <span>Strength</span>
      <input type="range" min="0.05" max="1" step="0.05" value={o.strength} style:--fill="{o.strength * 100}%" oninput={(e) => (paint.opts.strength = +e.currentTarget.value)} onchange={persist} />
    </label>
  {/if}
  {#if paint.tool === 'clone'}
    <span class="hint"><b>Alt+click</b> the spot to copy from, then paint.</span>
  {/if}

  {#if paint.tool === 'fill' || paint.tool === 'wand'}
    <label class="ctl">
      <span>Tolerance</span>
      <input type="range" min="0" max="100" value={o.tolerance} style:--fill="{o.tolerance}%" oninput={(e) => (paint.opts.tolerance = +e.currentTarget.value)} onchange={persist} />
      <span class="v">{o.tolerance}%</span>
    </label>
    <label class="chk"><input type="checkbox" checked={o.contiguous} onchange={(e) => { paint.opts.contiguous = e.currentTarget.checked; persist(); }} /> Contiguous</label>
    <label class="chk" title="Look at every visible layer, not just the current one"><input type="checkbox" checked={o.sampleAll} onchange={(e) => { paint.opts.sampleAll = e.currentTarget.checked; persist(); }} /> All layers</label>
  {/if}

  {#if paint.tool === 'picker'}
    <label class="chk" title="Pick from every visible layer, not just the current one"><input type="checkbox" checked={o.sampleAll} onchange={(e) => { paint.opts.sampleAll = e.currentTarget.checked; persist(); }} /> All layers</label>
    <span class="hint">Click: primary colour · right-click: secondary. Alt+click picks with the brush too.</span>
  {/if}

  {#if paint.tool === 'shape'}
    <div class="seg">
      {#each SHAPES as s (s.id)}
        <button class:on={o.shape === s.id} onclick={() => { paint.opts.shape = s.id; persist(); }}>{s.label}</button>
      {/each}
    </div>
    {#if o.shape === 'rect' || o.shape === 'ellipse'}
      <div class="seg">
        {#each STYLES as s (s.id)}
          <button class:on={o.shapeStyle === s.id} title={s.title} onclick={() => { paint.opts.shapeStyle = s.id; persist(); }}>{s.label}</button>
        {/each}
      </div>
    {/if}
    <label class="ctl">
      <span>Width</span>
      <input type="range" min="1" max="80" value={o.shapeWidth} style:--fill="{(o.shapeWidth / 80) * 100}%" oninput={(e) => (paint.opts.shapeWidth = +e.currentTarget.value)} onchange={persist} />
      <span class="v">{o.shapeWidth}px</span>
    </label>
    <span class="hint">Shift: straight angles, squares and circles.</span>
  {/if}

  {#if paint.tool === 'select' || paint.tool === 'lasso' || paint.tool === 'wand'}
    {#if paint.tool === 'select'}
      <div class="seg">
        <button class:on={o.selectShape === 'rect'} onclick={() => { paint.opts.selectShape = 'rect'; persist(); }}>Rectangle</button>
        <button class:on={o.selectShape === 'ellipse'} onclick={() => { paint.opts.selectShape = 'ellipse'; persist(); }}>Ellipse</button>
      </div>
    {/if}
    <div class="seg">
      {#each MODES as m (m.id)}
        <button class:on={o.selectMode === m.id} title={'title' in m ? m.title : undefined} onclick={() => { paint.opts.selectMode = m.id; persist(); }}>{m.label}</button>
      {/each}
    </div>
    <button class="btn small" disabled={!!paint.busy} onclick={() => void selectSubject()} title="Select the main subject (on this device)"><Scan size={14} /> Select subject</button>
    {#if d?.selection}
      <button class="btn small ghost" onclick={() => d.deselect()}>Deselect</button>
    {/if}
  {/if}

  {#if paint.tool === 'move'}
    <span class="hint">{d?.floating ? 'Drag to move · arrows nudge · Enter drops it · Esc puts it back' : d?.selection ? 'Drag to move the selected pixels.' : 'Drag to move the whole layer.'}</span>
    {#if d?.floating}<button class="btn small primary" onclick={() => d.commitFloating()}><Check size={14} /> Done</button>{/if}
  {/if}

  {#if paint.tool === 'text'}
    <select class="select" value={o.textPreset} onchange={(e) => { paint.opts.textPreset = e.currentTarget.value; persist(); }} aria-label="Text style">
      {#each TEXT_PRESETS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
    </select>
    <label class="ctl">
      <span>Size</span>
      <input class="n" type="number" min="6" max="2000" value={o.textSize} onchange={(e) => { paint.opts.textSize = Math.max(6, +e.currentTarget.value || 72); persist(); }} />
    </label>
    <span class="hint">Click to add text, click a text to edit or drag it. It stays editable until you paint on its layer.</span>
  {/if}

  {#if paint.tool === 'crop'}
    {#if paint.crop}
      <span class="v">{paint.crop.w} × {paint.crop.h}</span>
      <button class="btn small primary" onclick={applyCrop}><Check size={14} /> Crop</button>
      <button class="btn small ghost" onclick={() => (paint.crop = null)}><X size={14} /> Cancel</button>
    {:else}
      <span class="hint">Drag over the part to keep (Shift: square).</span>
      {#if d?.selBounds}<button class="btn small" onclick={() => (paint.crop = { ...d.selBounds! })}>Use the selection</button>{/if}
    {/if}
  {/if}

  {#if paint.tool === 'hand'}
    <span class="hint">Drag to move around. Tip: hold Space with any tool, or use the middle mouse button.</span>
  {/if}
</div>

<style>
  .opts {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 42px;
    padding: 4px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    scrollbar-width: thin;
    white-space: nowrap;
  }
  .ctl {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    font-size: 12px;
  }
  .ctl input[type='range'] {
    width: 110px;
  }
  .n {
    width: 56px;
    height: 26px;
    padding: 0 6px;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    font-size: 12px;
  }
  .v {
    color: var(--text-3);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    min-width: 34px;
  }
  .chk {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-2);
  }
  .hint {
    color: var(--text-3);
    font-size: 12px;
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
    height: 24px;
    padding: 0 9px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
  }
  .seg button.on {
    background: var(--panel-3);
    color: var(--text);
    font-weight: 600;
  }
  .select {
    height: 28px;
    width: auto;
  }
</style>
