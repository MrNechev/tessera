<script lang="ts">
  /** Resize the image (scales every layer) or the canvas (adds or cuts space around, by an anchor). */
  import { Link, Unlink } from '@lucide/svelte';
  import Modal from '../../components/ui/Modal.svelte';
  import ColorInput from '../../components/ui/ColorInput.svelte';
  import { paint } from '../state.svelte';

  interface Props {
    mode: 'image' | 'canvas';
    onclose: () => void;
  }
  let { mode, onclose }: Props = $props();
  const d = $derived(paint.doc!);
  const w0 = paint.doc!.width;
  const h0 = paint.doc!.height;
  let w = $state(w0);
  let h = $state(h0);
  let keep = $state(true);
  let ax = $state(1);
  let ay = $state(1);
  let fill = $state<'transparent' | 'color'>('transparent');
  let color = $state('#ffffff');

  function setW(v: number) {
    w = Math.max(1, Math.round(v));
    if (keep) h = Math.max(1, Math.round((w * h0) / w0));
  }
  function setH(v: number) {
    h = Math.max(1, Math.round(v));
    if (keep) w = Math.max(1, Math.round((h * w0) / h0));
  }
  function pct(p: number) {
    w = Math.max(1, Math.round((w0 * p) / 100));
    h = Math.max(1, Math.round((h0 * p) / 100));
  }

  function apply() {
    if (mode === 'image') d.resizeImage(w, h);
    else d.resizeCanvas(w, h, ax, ay, fill === 'color' ? color : null);
    paint.fitPending = true;
    onclose();
  }
</script>

<Modal title={mode === 'image' ? 'Resize image' : 'Canvas size'} width={440} {onclose}>
  <p class="hint">{mode === 'image' ? 'Every layer is scaled.' : 'Adds space around the picture or cuts it off; the picture itself is not scaled.'} Now {w0} × {h0}.</p>
  <div class="size">
    <label>Width <input class="input" type="number" min="1" max="16384" value={w} oninput={(e) => setW(+e.currentTarget.value)} /></label>
    <button class="icon-btn" class:active={keep} title={keep ? 'Keep proportions' : 'Free proportions'} aria-label="Keep proportions" onclick={() => (keep = !keep)}>
      {#if keep}<Link size={16} />{:else}<Unlink size={16} />{/if}
    </button>
    <label>Height <input class="input" type="number" min="1" max="16384" value={h} oninput={(e) => setH(+e.currentTarget.value)} /></label>
  </div>
  {#if mode === 'image'}
    <div class="pcts">
      {#each [25, 50, 75, 150, 200] as p (p)}<button class="btn small" onclick={() => pct(p)}>{p}%</button>{/each}
    </div>
  {:else}
    <div class="anchor-row">
      <span class="label">Anchor</span>
      <div class="anchor" role="radiogroup" aria-label="Anchor">
        {#each [0, 1, 2] as y (y)}
          {#each [0, 1, 2] as x (x)}
            <button role="radio" aria-checked={ax === x && ay === y} aria-label="Anchor {x},{y}" class:on={ax === x && ay === y} onclick={() => { ax = x; ay = y; }}></button>
          {/each}
        {/each}
      </div>
      <div class="fill">
        <span class="label">New space</span>
        <div class="seg">
          <button class:on={fill === 'transparent'} onclick={() => (fill = 'transparent')}>Transparent</button>
          <button class:on={fill === 'color'} onclick={() => (fill = 'color')}>Colour</button>
        </div>
        {#if fill === 'color'}<ColorInput value={color} onchange={(c) => (color = c)} />{/if}
      </div>
    </div>
  {/if}
  {#if w * h > 16_000_000}<p class="hint warn">That is over 16 MP — painting may get slow.</p>{/if}
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={w === w0 && h === h0} onclick={apply}>Apply</button>
  {/snippet}
</Modal>

<style>
  .size {
    display: flex;
    align-items: flex-end;
    gap: 10px;
    margin-top: 10px;
  }
  .size label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
  }
  .size input {
    width: 130px;
  }
  .pcts {
    display: flex;
    gap: 6px;
    margin-top: 12px;
  }
  .anchor-row {
    display: flex;
    gap: 18px;
    margin-top: 14px;
    align-items: flex-start;
  }
  .anchor {
    display: grid;
    grid-template-columns: repeat(3, 22px);
    gap: 3px;
    margin-top: 4px;
  }
  .anchor button {
    width: 22px;
    height: 22px;
    border-radius: 5px;
    border: 1px solid var(--border-strong);
    background: var(--panel-2);
    cursor: pointer;
  }
  .anchor button.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .fill {
    display: flex;
    flex-direction: column;
    gap: 6px;
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
    height: 26px;
    padding: 0 10px;
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
  .warn {
    color: var(--warning);
    margin-top: 10px;
  }
</style>
