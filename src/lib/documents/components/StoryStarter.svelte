<script lang="ts">
  // Story starter: pick a date range (a trip, a weekend) and optionally a folder; Tessera drafts
  // a story with a chapter per day, a section per moment (split at long pauses and when the
  // place changes), the best few photos of each, and captions with date, time and place.
  import { onMount, untrack } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { api } from '../lib/api';
  import { addDays, fmtRange, HOUR, isoDay } from '../lib/diary';
  import { countRange, draftAndOpen, type RangeQuery } from '../lib/journal';
  import { toasts } from '../lib/toast.svelte';
  import type { DirNode, Library } from '../lib/types';
  import FolderTree from './FolderTree.svelte';
  import Icon from './Icon.svelte';

  let { from: from0, to: to0, onclose, ondone }: { from?: string; to?: string; onclose: () => void; ondone?: () => void } = $props();

  const today = isoDay(new Date());
  let from = $state(untrack(() => from0) ?? addDays(today, -6));
  let to = $state(untrack(() => to0) ?? today);
  let libraries = $state<Library[]>([]);
  let library = $state('');
  let dir = $state('');
  let tree = $state<DirNode | null>(null);
  const expanded = new SvelteSet<string>();
  let perMoment = $state(4);
  let gapH = $state(2);
  let videos = $state(true);
  let title = $state('');
  let count = $state<number | null>(null);
  let busy = $state(false);

  const query = $derived<RangeQuery>({ from: from <= to ? from : to, to: from <= to ? to : from, library, dir, videos });

  onMount(() => {
    api.libraries().then((l) => (libraries = l)).catch(() => {});
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  // the folder tree of the chosen library
  $effect(() => {
    const lib = library;
    untrack(() => {
      dir = '';
      tree = null;
      expanded.clear();
      if (lib) api.tree(lib).then((t) => library === lib && (tree = t)).catch(() => {});
    });
  });

  // live count of what the draft would draw from
  $effect(() => {
    const q = query;
    if (!q.from || !q.to) return;
    const ac = new AbortController();
    const t = setTimeout(() => countRange(q, ac.signal).then((n) => (count = n)).catch(() => {}), 200);
    return () => {
      clearTimeout(t);
      ac.abort();
    };
  });

  async function draft() {
    if (busy) return;
    busy = true;
    try {
      if (await draftAndOpen(query, { perMoment, gapMs: gapH * HOUR, title })) {
        onclose();
        ondone?.();
      } else toasts.show('No photos or videos in this range', 'info');
    } catch (e) {
      toasts.error((e as Error).message);
    } finally {
      busy = false;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>New story from a trip or dates</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body">
      <p class="muted intro">Tessera drafts the story — a chapter per day, a section per moment with its best few photos, captions with date and place — and you take it from there.</p>
      <div class="row">
        <label>From <input class="input" type="date" bind:value={from} /></label>
        <label>to <input class="input" type="date" bind:value={to} /></label>
        <span class="muted">{from && to ? fmtRange(query.from, query.to) : ''}</span>
      </div>
      <div class="row">
        <label>Library
          <select class="input" bind:value={library}>
            <option value="">All libraries</option>
            {#each libraries as l (l.id)}<option value={l.id}>{l.name}</option>{/each}
          </select>
        </label>
        {#if library}<span class="muted">Folder: <b>{dir || 'all folders'}</b>{#if dir} <button class="btn sm ghost" onclick={() => (dir = '')}>All folders</button>{/if}</span>{/if}
      </div>
      {#if library && tree?.children?.length}
        <div class="tree">
          <FolderTree node={tree} selected={dir} {expanded} onselect={(p) => (dir = p)} ontoggle={(p) => (expanded.has(p) ? expanded.delete(p) : expanded.add(p))} />
        </div>
      {/if}
      <div class="row">
        <label>Photos per moment
          <select class="input" bind:value={perMoment}>
            {#each [2, 3, 4, 6, 9] as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
        <label>New moment after a pause of
          <select class="input" bind:value={gapH}>
            <option value={0.5}>30 minutes</option><option value={1}>1 hour</option><option value={2}>2 hours</option><option value={4}>4 hours</option>
          </select>
        </label>
        <label class="cb"><input type="checkbox" bind:checked={videos} /> include videos</label>
      </div>
      <label class="titlein">Title <input class="input" placeholder="Automatic (place and dates)" bind:value={title} /></label>
    </div>
    <footer>
      <span class="muted">{count === null ? 'Counting…' : `${count.toLocaleString()} photo${count === 1 ? '' : 's'}${videos ? ' and videos' : ''} to choose from`}</span>
      <span class="grow"></span>
      <button class="btn" onclick={onclose}>Cancel</button>
      <button class="btn primary" disabled={busy || !count} onclick={draft}><Icon name="chapter" size={15} /> {busy ? 'Drafting…' : 'Draft the story'}</button>
    </footer>
  </div>
</div>

<style>
  .intro {
    margin: 0 0 14px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 12px;
    margin-bottom: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  label.cb {
    flex-direction: row;
    align-items: center;
    padding-bottom: 6px;
  }
  .titlein .input {
    width: 100%;
  }
  .tree {
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 4px;
    margin-bottom: 12px;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--line);
  }
  .grow {
    flex: 1;
  }
</style>
