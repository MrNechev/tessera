<script lang="ts">
  import type { Snippet } from 'svelte';
  import { ChevronDown } from '@lucide/svelte';
  interface Props {
    title: string;
    open?: boolean;
    actions?: Snippet;
    children: Snippet;
  }
  let { title, open = true, actions, children }: Props = $props();
  let isOpen = $state(true);
  $effect.pre(() => {
    isOpen = open;
  });
</script>

<section class="section">
  <header>
    <button type="button" class="toggle" onclick={() => (isOpen = !isOpen)} aria-expanded={isOpen}>
      <span class="chev" class:closed={!isOpen}><ChevronDown size={14} /></span>
      <span>{title}</span>
    </button>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </header>
  {#if isOpen}
    <div class="body">{@render children()}</div>
  {/if}
</section>

<style>
  .section {
    border-bottom: 1px solid var(--border);
    padding: 10px 14px 12px;
  }
  header {
    display: flex;
    align-items: center;
    min-height: 24px;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
    border: 0;
    background: none;
    padding: 0;
    cursor: pointer;
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
    text-align: left;
  }
  .chev {
    display: inline-flex;
    transition: transform 0.15s;
    color: var(--text-3);
  }
  .chev.closed {
    transform: rotate(-90deg);
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 10px;
  }
</style>
