<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { app } from '../../stores/app.svelte';
  interface Props {
    title: string;
    width?: number;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  }
  let { title, width = 520, onclose, children, footer }: Props = $props();

  onMount(() => {
    app.modals++;
    return () => app.modals--;
  });
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />

<div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="modal card" role="dialog" aria-modal="true" aria-label={title} style:width="{width}px">
    <header>
      <h3>{title}</h3>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><X size={18} /></button>
    </header>
    <div class="content">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(8, 6, 14, 0.55);
    backdrop-filter: blur(3px);
    display: grid;
    place-items: center;
    animation: fade-in 0.12s ease-out;
  }
  .modal {
    max-width: calc(100vw - 40px);
    max-height: calc(100vh - 60px);
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    padding: 14px 14px 10px 20px;
  }
  h3 {
    flex: 1;
    font-size: 16px;
  }
  .content {
    padding: 6px 20px 18px;
    overflow: auto;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
    background: var(--panel-2);
  }
</style>
