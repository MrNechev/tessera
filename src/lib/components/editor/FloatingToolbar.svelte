<script lang="ts">
  import {
    AlignCenterHorizontal,
    AlignCenterVertical,
    AlignEndHorizontal,
    AlignEndVertical,
    AlignStartHorizontal,
    AlignStartVertical,
    ArrowDown,
    ArrowUp,
    Copy,
    Crop,
    FlipHorizontal,
    ImagePlus,
    Pin,
    PinOff,
    Trash2,
    Type,
    X,
  } from '@lucide/svelte';
  import { editor } from '../../editor/editor.svelte';

  interface Props {
    rect: { x: number; y: number; w: number; h: number };
    cw: number;
    onRequestPhoto?: (id: string) => void;
  }
  let { rect, cw, onRequestPhoto }: Props = $props();

  const el = $derived(editor.single);
  const multi = $derived(editor.selection.length > 1);
  // Free elements show a rotation handle above them: keep the toolbar clear of it.
  const gap = $derived(el && !(el.kind === 'photo' && el.slot) && !el.locked ? 86 : 50);
  const above = $derived(rect.y > gap + 10);
  const left = $derived(Math.max(150, Math.min(cw - 150, rect.x + rect.w / 2)));
  const top = $derived(above ? rect.y - gap : rect.y + rect.h + 14);
</script>

<div class="bar card" style:left="{left}px" style:top="{top}px" role="toolbar" aria-label="Selection actions">
  {#if multi}
    <button class="icon-btn" title="Align left" onclick={() => editor.align('left')}><AlignStartVertical size={16} /></button>
    <button class="icon-btn" title="Align centre" onclick={() => editor.align('hcenter')}><AlignCenterVertical size={16} /></button>
    <button class="icon-btn" title="Align right" onclick={() => editor.align('right')}><AlignEndVertical size={16} /></button>
    <button class="icon-btn" title="Align top" onclick={() => editor.align('top')}><AlignStartHorizontal size={16} /></button>
    <button class="icon-btn" title="Align middle" onclick={() => editor.align('vcenter')}><AlignCenterHorizontal size={16} /></button>
    <button class="icon-btn" title="Align bottom" onclick={() => editor.align('bottom')}><AlignEndHorizontal size={16} /></button>
    <span class="sep"></span>
    <button class="icon-btn" title="Duplicate (Ctrl+D)" onclick={() => editor.duplicateSelected()}><Copy size={16} /></button>
    <button class="icon-btn danger" title="Delete (Del)" onclick={() => editor.deleteSelected()}><Trash2 size={16} /></button>
  {:else if el}
    {#if el.kind === 'photo'}
      <button class="icon-btn" title="Replace photo" onclick={() => onRequestPhoto?.(el.id)}><ImagePlus size={16} /></button>
      {#if el.asset}
        <button class="icon-btn" class:active={editor.cropTarget === el.id} title="Reposition & zoom (double-click)" onclick={() => (editor.cropTarget = editor.cropTarget === el.id ? null : el.id)}><Crop size={16} /></button>
      {/if}
      {#if el.slot}
        <button class="icon-btn" title="Detach from layout (move freely)" onclick={() => editor.detach(el.id)}><PinOff size={16} /></button>
        <span class="sep"></span>
        <button class="icon-btn danger" title="Remove from collage" onclick={() => editor.removeFromLayout(el.id)}><X size={16} /></button>
      {:else}
        {#if editor.hasLayout && el.asset}
          <button class="icon-btn" title="Put back into the layout" onclick={() => editor.patch(el.id, { slot: true } as never)}><Pin size={16} /></button>
        {/if}
        <button class="icon-btn" title="Flip horizontally" onclick={() => editor.patch(el.id, { flipX: !el.flipX })}><FlipHorizontal size={16} /></button>
      {/if}
    {:else if el.kind === 'text'}
      <button class="icon-btn" title="Edit text (double-click)" onclick={() => (editor.editingText = el.id)}><Type size={16} /></button>
    {:else}
      <button class="icon-btn" title="Flip horizontally" onclick={() => editor.patch(el.id, { flipX: !el.flipX })}><FlipHorizontal size={16} /></button>
    {/if}
    {#if !(el.kind === 'photo' && el.slot)}
      <span class="sep"></span>
      <button class="icon-btn" title="Bring forward (Ctrl+])" onclick={() => editor.arrange('forward')}><ArrowUp size={16} /></button>
      <button class="icon-btn" title="Send backward (Ctrl+[)" onclick={() => editor.arrange('backward')}><ArrowDown size={16} /></button>
      <button class="icon-btn" title="Duplicate (Ctrl+D)" onclick={() => editor.duplicateSelected()}><Copy size={16} /></button>
      <button class="icon-btn danger" title="Delete (Del)" onclick={() => editor.deleteSelected()}><Trash2 size={16} /></button>
    {/if}
  {/if}
</div>

<style>
  .bar {
    position: absolute;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px;
    border-radius: 12px;
    box-shadow: var(--shadow);
    z-index: 5;
    animation: fade-in 0.12s ease-out;
  }
  .sep {
    width: 1px;
    height: 20px;
    background: var(--border);
    margin: 0 3px;
  }
  .danger:hover {
    color: var(--danger);
  }
</style>
