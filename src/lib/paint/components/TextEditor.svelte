<script lang="ts">
  /** The editor of a live text layer: the words, style, size, colour and alignment, next to the text on the canvas. */
  import { Check, TextAlignCenter, TextAlignEnd, TextAlignStart } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import type { TextElement } from '../../model/types';
  import { ensureFont } from '../../assets/fonts';
  import { naturalTextWidth } from '../../render/text';
  import { presetElement, TEXT_PRESETS, textPreset } from '../../text/styles';
  import ColorInput from '../../components/ui/ColorInput.svelte';
  import { refitMeme } from '../actions.svelte';
  import { paint } from '../state.svelte';
  import { finishTextEdit } from '../tools.svelte';

  const d = $derived(paint.doc!);
  const id = $derived(paint.textEdit!.layerId);
  const layer = $derived(d.layer(id));
  const el = $derived(layer?.text ?? null);
  let area: HTMLTextAreaElement | undefined = $state();


  /** Keep the box hugging the words (left-aligned texts grow to the right; meme texts keep their width). */
  function update(patch: Partial<TextElement>) {
    if (!el) return;
    const next = { ...($state.snapshot(el) as TextElement), ...patch };
    if (layer?.name !== 'Top text' && layer?.name !== 'Bottom text') {
      const nat = Math.ceil(naturalTextWidth(next));
      if (next.align === 'center') next.x = Math.round(next.x + (next.w - nat) / 2);
      else if (next.align === 'right') next.x = Math.round(next.x + next.w - nat);
      next.w = nat;
    }
    d.setText(id, next);
    refitMeme(d, id);
  }

  function style(pid: string) {
    const p = textPreset(pid);
    if (!p || !el) return;
    paint.opts.textPreset = pid;
    const base = presetElement(p, el.size, Math.min(d.width, d.height));
    update({ stroke: null, background: null, shadow: null, uppercase: false, letterSpacing: 0, weight: 400, italic: false, curve: 0, ...base, text: el.text, fill: p.id === 'meme' ? '#ffffff' : el.fill });
    // re-measure once the style's font has loaded
    void ensureFont(p.look.font, p.look.weight ?? 400).then(() => update({}));
  }

  // Changing the primary colour while editing colours the text.
  let lastPrimary = paint.primary;
  $effect(() => {
    const c = paint.primary;
    if (c !== lastPrimary) {
      lastPrimary = c;
      update({ fill: c.slice(0, 7) });
    }
  });

  const pos = $derived.by(() => {
    if (!el) return { left: 0, top: 0 };
    const v = paint.view;
    const left = v.x + el.x * v.zoom;
    const below = v.y + (el.y + el.h) * v.zoom + 12;
    return { left: Math.max(8, left), top: below };
  });

  onMount(() => {
    area?.focus();
    if (paint.textEdit?.before === null) area?.select();
  });
</script>

{#if el}
  <div class="text-editor card" style:left="{pos.left}px" style:top="min({pos.top}px, calc(100% - 190px))" role="dialog" aria-label="Edit text">
    <textarea
      bind:this={area}
      class="input"
      rows="2"
      value={el.text}
      placeholder="Type your text"
      oninput={(e) => update({ text: e.currentTarget.value })}
      onkeydown={(e) => {
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          finishTextEdit();
        }
        if (e.key === 'Escape') {
          e.preventDefault();
          finishTextEdit();
        }
      }}
    ></textarea>
    <div class="row">
      <select class="select" aria-label="Style" value={paint.opts.textPreset} onchange={(e) => style(e.currentTarget.value)}>
        {#each TEXT_PRESETS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
      </select>
      <input class="input n" type="number" min="6" max="4000" aria-label="Size" title="Size (px)" value={Math.round(el.size)} onchange={(e) => {
        const s = Math.max(6, +e.currentTarget.value || el.size);
        const k = s / el.size;
        update({ size: s, stroke: el.stroke ? { ...el.stroke, width: el.stroke.width * k } : null });
        paint.opts.textSize = s;
      }} />
    </div>
    <div class="row">
      <div class="color"><ColorInput value={el.fill} label="Colour" onchange={(c) => update({ fill: c })} /></div>
      <div class="seg">
        <button class:on={el.align === 'left'} title="Left" aria-label="Align left" onclick={() => update({ align: 'left' })}><TextAlignStart size={14} /></button>
        <button class:on={el.align === 'center'} title="Centre" aria-label="Align centre" onclick={() => update({ align: 'center' })}><TextAlignCenter size={14} /></button>
        <button class:on={el.align === 'right'} title="Right" aria-label="Align right" onclick={() => update({ align: 'right' })}><TextAlignEnd size={14} /></button>
      </div>
    </div>
    <div class="row end">
      <span class="hint">Drag the text on the canvas to move it.</span>
      <button class="btn small primary" onclick={finishTextEdit}><Check size={14} /> Done</button>
    </div>
  </div>
{/if}

<style>
  .text-editor {
    position: absolute;
    z-index: 20;
    width: 280px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-shadow: var(--shadow);
  }
  textarea {
    resize: vertical;
    min-height: 52px;
    font-size: 14px;
  }
  .row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .row.end {
    justify-content: space-between;
  }
  .select {
    flex: 1;
    height: 30px;
  }
  .n {
    width: 70px;
    height: 30px;
  }
  .color {
    flex: 1;
    min-width: 0;
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
    width: 28px;
    height: 26px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
  }
  .seg button.on {
    background: var(--panel-3);
    color: var(--text);
  }
  .hint {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
