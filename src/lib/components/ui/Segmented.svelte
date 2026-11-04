<script lang="ts" generics="T extends string | number">
  import type { Component } from 'svelte';
  interface Option {
    value: T;
    label?: string;
    title?: string;
    icon?: Component<{ size?: number }>;
  }
  interface Props {
    options: Option[];
    value: T;
    onchange: (v: T) => void;
    full?: boolean;
  }
  let { options, value, onchange, full = true }: Props = $props();
</script>

<div class="seg" class:full role="radiogroup">
  {#each options as o (o.value)}
    <button
      type="button"
      role="radio"
      aria-checked={o.value === value}
      class:on={o.value === value}
      title={o.title ?? o.label}
      onclick={() => onchange(o.value)}
    >
      {#if o.icon}<o.icon size={15} />{/if}
      {#if o.label}<span>{o.label}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .seg {
    display: inline-flex;
    padding: 3px;
    gap: 2px;
    border-radius: var(--radius-sm);
    background: var(--bg-2);
    border: 1px solid var(--border);
  }
  .seg.full {
    display: flex;
  }
  button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    font-weight: 560;
    cursor: pointer;
    white-space: nowrap;
  }
  button:hover {
    color: var(--text);
    background: var(--hover);
  }
  button.on {
    background: var(--panel-3);
    color: var(--text);
    box-shadow: var(--shadow-sm);
  }
</style>
