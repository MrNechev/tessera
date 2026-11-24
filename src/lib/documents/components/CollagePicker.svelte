<script lang="ts">
  // Pick the collage a section shows live: a recent collage project (and the page of a
  // multi-page one), or start a new collage from the section's photos or a blank page.
  import { onMount } from 'svelte';
  import type { ProjectSummary } from '../../api/types';
  import { pageCount } from '../../model/multipage';
  import { listProjects, loadCollage, projectThumb, type CollageChoice } from '../lib/collages.svelte';
  import type { MediaRef } from '../lib/types';
  import CollageView from './CollageView.svelte';
  import Icon from './Icon.svelte';

  let {
    photos = [],
    name = '',
    onpick,
    onclose,
  }: {
    /** the section's photos, for "New collage from this section's photos" */
    photos?: MediaRef[];
    /** name for a new collage */
    name?: string;
    onpick: (c: CollageChoice) => void;
    onclose: () => void;
  } = $props();

  /** Page thumbnails are live renders; more than this would import a whole photo book at once. */
  const MAX_PAGES = 48;

  let projects = $state<ProjectSummary[] | null>(null);
  let error = $state('');
  let q = $state('');
  /** a multi-page project waiting for its page to be chosen */
  let pagesOf = $state<{ p: ProjectSummary; count: number } | null>(null);

  onMount(async () => {
    try {
      projects = await listProjects();
    } catch (e) {
      error = String(e instanceof Error ? e.message : e);
      projects = [];
    }
  });

  const shown = $derived((projects ?? []).filter((p) => !q.trim() || p.name.toLowerCase().includes(q.trim().toLowerCase())));

  function refOf(p: ProjectSummary, page = 0) {
    return { projectId: p.id, page: page || undefined, aspect: p.width && p.height ? p.width / p.height : undefined, name: p.name };
  }

  async function choose(p: ProjectSummary) {
    let count = 1;
    try {
      count = pageCount(await loadCollage(p.id));
    } catch {
      // unreadable: link it anyway, the section shows the "missing" placeholder
    }
    if (count > 1) pagesOf = { p, count };
    else onpick({ kind: 'project', ref: refOf(p) });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog" onclick={(e) => e.stopPropagation()}>
    <header>
      {#if pagesOf}<button class="icon-btn" onclick={() => (pagesOf = null)} aria-label="Back"><Icon name="chev-left" /></button>{/if}
      <h2>{pagesOf ? `Which page of “${pagesOf.p.name}”?` : 'Show a collage'}</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body">
      {#if pagesOf}
        {@const pp = pagesOf.p}
        <div class="grid">
          {#each Array.from({ length: Math.min(pagesOf.count, MAX_PAGES) }, (_, i) => i) as i (i)}
            <button class="tile" onclick={() => onpick({ kind: 'project', ref: refOf(pp, i) })}>
              <span class="th"><CollageView ref={{ projectId: pp.id, page: i }} fill still maxSide={360} /></span>
              <span class="nm">Page {i + 1}</span>
            </button>
          {/each}
        </div>
      {:else}
        <p class="muted small">The section shows the collage itself, not a copy: edit it in the collage editor and the story follows.</p>
        <div class="new">
          {#if photos.length}
            <button class="btn" onclick={() => onpick({ kind: 'new', photos, name })}><Icon name="image" /> New collage from this section's photos</button>
          {/if}
          <button class="btn" onclick={() => onpick({ kind: 'new', photos: [], name })}><Icon name="plus" /> New blank collage</button>
        </div>
        {#if projects && projects.length > 6}
          <input class="input search" placeholder="Search collages…" bind:value={q} />
        {/if}
        {#if !projects}
          <p class="muted">Loading collages…</p>
        {:else if error}
          <p class="err">{error}</p>
        {:else if !projects.length}
          <p class="muted">No collages yet — start a new one above.</p>
        {:else}
          <div class="grid">
            {#each shown as p (p.id)}
              <button class="tile" onclick={() => void choose(p)} title={p.name}>
                <span class="th">{#if projectThumb(p)}<img src={projectThumb(p)} alt="" loading="lazy" />{:else}<Icon name="collage" size={22} />{/if}</span>
                <span class="nm">{p.name || 'Untitled'}</span>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .small {
    font-size: 12.5px;
    margin: 0 0 12px;
  }
  .new {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 14px;
  }
  .search {
    width: 100%;
    margin-bottom: 12px;
  }
  .err {
    color: var(--red);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--bg-2);
    cursor: pointer;
    text-align: left;
    min-width: 0;
  }
  .tile:hover {
    border-color: var(--accent);
  }
  .th {
    display: grid;
    place-items: center;
    aspect-ratio: 4 / 3;
    border-radius: 6px;
    overflow: hidden;
    background: var(--surface-2);
    color: var(--text-3);
  }
  .th img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .nm {
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
