<script lang="ts">
  import type { DirNode } from '../lib/types';
  import Icon from './Icon.svelte';
  import Self from './FolderTree.svelte';

  let {
    node,
    depth = 0,
    selected,
    expanded,
    onselect,
    ontoggle,
    oncontext,
  }: {
    node: DirNode;
    depth?: number;
    selected: string;
    expanded: Set<string>;
    onselect: (p: string) => void;
    ontoggle: (p: string) => void;
    oncontext?: (p: string, e: MouseEvent) => void;
  } = $props();
</script>

{#each node.children ?? [] as c (c.path)}
  {@const open = expanded.has(c.path)}
  <div class="row" class:sel={selected === c.path} style="padding-left:{6 + depth * 14}px">
    {#if c.children?.length}
      <button class="tw" onclick={() => ontoggle(c.path)} aria-label={open ? 'Collapse' : 'Expand'}>
        <Icon name={open ? 'chev-down' : 'chev-right'} size={13} />
      </button>
    {:else}
      <span class="tw"></span>
    {/if}
    <button
      class="name"
      onclick={() => onselect(c.path)}
      ondblclick={() => ontoggle(c.path)}
      oncontextmenu={(e) => { if (oncontext) { e.preventDefault(); oncontext(c.path, e); } }}
      title="{c.path} — right-click for more">
      <Icon name="folder" size={14} />
      <span class="label">{c.name}</span>
      <span class="count">{c.total}</span>
    </button>
  </div>
  {#if open && c.children?.length}
    <Self node={c} depth={depth + 1} {selected} {expanded} {onselect} {ontoggle} {oncontext} />
  {/if}
{/each}

<style>
  .row {
    display: flex;
    align-items: center;
    height: 26px;
    border-radius: 6px;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.sel {
    background: var(--accent-soft);
  }
  .row.sel .name {
    color: var(--accent);
  }
  .tw {
    width: 18px;
    height: 22px;
    display: grid;
    place-items: center;
    background: none;
    border: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 0;
    flex: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    color: var(--text-2);
    cursor: pointer;
    padding: 0 6px 0 0;
    text-align: left;
    height: 100%;
  }
  .label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
  }
</style>
