<script lang="ts" module>
  export const ICON_MIME = 'application/x-tessera-icon';
</script>

<script lang="ts">
  // Diagram icons for the canvas: search, then click (or drag) one onto the board.
  import { ICON_GROUPS, ICONS, searchIcons } from '../lib/diagram/icons';
  import Icon from './Icon.svelte';

  let { onpick }: { onpick: (id: string) => void } = $props();
  let q = $state('');
  const found = $derived(q.trim() ? searchIcons(q) : ICONS);
  const groups = $derived(ICON_GROUPS.map((g) => ({ g, icons: found.filter((i) => i.group === g) })).filter((x) => x.icons.length));
</script>

<div class="ic">
  <label class="search">
    <Icon name="search" size={15} />
    <!-- svelte-ignore a11y_autofocus -->
    <input class="input" placeholder="Search icons (server, user, cloud…)" bind:value={q} autofocus />
  </label>
  {#each groups as { g, icons } (g)}
    <div class="gh">{g}</div>
    <div class="grid">
      {#each icons as i (i.id)}
        <button
          class="cell"
          title={i.id.replace(/-/g, ' ')}
          draggable="true"
          ondragstart={(e) => {
            e.dataTransfer?.setData(ICON_MIME, i.id);
            if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy';
          }}
          onclick={() => onpick(i.id)}
        >
          <svg width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={i.d} /></svg>
        </button>
      {/each}
    </div>
  {:else}
    <p class="muted">No icon called “{q}”.</p>
  {/each}
</div>

<style>
  .search {
    position: relative;
    display: block;
    margin-bottom: 6px;
    color: var(--text-3);
  }
  .search :global(svg) {
    position: absolute;
    left: 9px;
    top: 50%;
    transform: translateY(-50%);
  }
  .search input {
    width: 100%;
    padding-left: 30px;
  }
  .gh {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    margin: 8px 2px 4px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(40px, 1fr));
    gap: 3px;
  }
  .cell {
    height: 40px;
    display: grid;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
  }
  .cell:hover {
    border-color: var(--accent);
    color: var(--accent);
  }
</style>
