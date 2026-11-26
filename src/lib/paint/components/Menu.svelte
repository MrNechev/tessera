<script lang="ts" module>
  export type MenuItem = { label: string; run: () => void; disabled?: boolean; keys?: string } | 'sep';
</script>

<script lang="ts">
  /** A small dropdown menu for Paint's top bar. */
  import { ChevronDown } from '@lucide/svelte';
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    items: () => MenuItem[];
    icon?: Snippet;
  }
  let { label, items, icon }: Props = $props();
  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();
  const list = $derived(open ? items() : []);
</script>

<svelte:window
  onpointerdown={(e) => {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }}
  onkeydown={(e) => {
    if (open && e.key === 'Escape') {
      e.stopPropagation();
      open = false;
    }
  }}
/>

<div class="menu" bind:this={root}>
  <button class="btn ghost mb" class:open aria-haspopup="menu" aria-expanded={open} onclick={() => (open = !open)}>
    {#if icon}{@render icon()}{/if}{label}<ChevronDown size={13} />
  </button>
  {#if open}
    <div class="pop card" role="menu">
      {#each list as it, i (i)}
        {#if it === 'sep'}
          <div class="sep"></div>
        {:else}
          <button
            role="menuitem"
            class="mi"
            disabled={it.disabled}
            onclick={() => {
              open = false;
              it.run();
            }}
          >
            <span>{it.label}</span>
            {#if it.keys}<span class="keys">{it.keys}</span>{/if}
          </button>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .menu {
    position: relative;
  }
  .mb {
    height: 30px;
    padding: 0 9px;
    gap: 5px;
  }
  .mb.open {
    background: var(--active);
  }
  .pop {
    position: absolute;
    z-index: 60;
    top: 34px;
    left: 0;
    min-width: 230px;
    padding: 5px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .mi {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
  }
  .mi:hover:not(:disabled) {
    background: var(--hover);
  }
  .mi:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .keys {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
</style>
