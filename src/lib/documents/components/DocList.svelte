<script lang="ts">
  import { onMount } from 'svelte';
  import { api, fmtDate, thumbUrl } from '../lib/api';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { DocMeta } from '../lib/types';
  import Icon from './Icon.svelte';
  import { fromTemplate, TEMPLATES, type Template } from '../lib/templates';
  import type { SearchHit } from '../lib/types';

  let docs = $state<DocMeta[] | null>(null);
  let filter = $state('');
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  let fileInput: HTMLInputElement;
  let tag = $state<string | null>(null);
  let tplOpen = $state(false);
  let moreOpen = $state(false);
  let hits = $state<SearchHit[]>([]);
  let searchTimer: ReturnType<typeof setTimeout>;

  const allTags = $derived([...new Set((docs ?? []).flatMap((d) => d.tags ?? []))].sort((a, b) => a.localeCompare(b)));
  const shown = $derived(
    (docs ?? []).filter(
      (d) =>
        (!tag || d.tags?.includes(tag)) &&
        (!filter || d.title.toLowerCase().includes(filter.toLowerCase()) || d.tags?.some((t) => t.toLowerCase().includes(filter.toLowerCase()))),
    ),
  );

  // text inside documents (sections, captions, notes)
  $effect(() => {
    const q = filter.trim();
    clearTimeout(searchTimer);
    if (q.length < 3) {
      hits = [];
      return;
    }
    searchTimer = setTimeout(() => api.search(q).then((h) => (hits = h.filter((x) => x.field !== 'title' && x.field !== 'tag').slice(0, 20))).catch(() => {}), 250);
  });

  async function createFrom(t: Template) {
    tplOpen = false;
    const d = await api.importDoc(fromTemplate(t));
    router.go(`/doc/${d.id}${t.canvas ? '/canvas' : ''}`);
  }

  async function load() {
    try {
      docs = await api.docs();
      resolver.prefetch(
        docs.filter((d) => d.coverPath).map((d) => ({ libraryId: d.coverLibraryId!, path: d.coverPath! })),
      );
    } catch (e) {
      toasts.error((e as Error).message);
    }
  }

  onMount(load);

  async function create() {
    const d = await api.createDoc('Untitled');
    router.go(`/doc/${d.id}`);
  }

  async function duplicate(d: DocMeta) {
    await api.duplicateDoc(d.id);
    toasts.show(`Duplicated “${d.title}”`, 'ok');
    load();
  }

  async function remove(d: DocMeta) {
    if (!confirm(`Delete “${d.title}”? It is moved to data/documents/.trash and can be recovered from there.`)) return;
    await api.deleteDoc(d.id);
    load();
  }

  function startRename(d: DocMeta) {
    renaming = d.id;
    renameValue = d.title;
  }

  async function commitRename(d: DocMeta) {
    const title = renameValue.trim();
    renaming = null;
    if (!title || title === d.title) return;
    const full = await api.getDoc(d.id);
    full.title = title;
    await api.saveDoc(d.id, full.version, full);
    load();
  }

  async function importFile(e: Event) {
    const f = (e.target as HTMLInputElement).files?.[0];
    if (!f) return;
    try {
      const doc = JSON.parse(await f.text());
      const d = await api.importDoc(doc);
      toasts.show(`Imported “${d.title}”`, 'ok');
      load();
    } catch (err) {
      toasts.error('Import failed: ' + (err as Error).message);
    } finally {
      fileInput.value = '';
    }
  }

  async function stress() {
    try {
      const d = await api.stressTest();
      router.go(`/doc/${d.id}`);
    } catch (e) {
      toasts.error((e as Error).message);
    }
  }

  function cover(d: DocMeta) {
    if (!d.coverPath) return '';
    const r = resolver.cache.get(d.coverLibraryId + '\u0000' + d.coverPath);
    return r?.found ? thumbUrl(r.derived, 1280) : '';
  }
</script>

<div class="page">
  <main>
    <div class="head">
      <div class="titles">
        <h1>Stories</h1>
        <p class="muted">Diaries, trips and chronicles — told with your photos and videos, presented full-screen.</p>
      </div>
      <input class="input search" type="search" placeholder="Search titles, tags and text…" bind:value={filter} />
      <div class="menuwrap">
        <button class="icon-btn" title="More" aria-label="More" onclick={() => (moreOpen = !moreOpen)}><Icon name="more" /></button>
        {#if moreOpen}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="menu-backdrop" onclick={() => (moreOpen = false)}></div>
          <div class="tpls more-menu">
            <button onclick={() => { moreOpen = false; fileInput.click(); }}><Icon name="upload" /><span><b>Import a story</b><small>A .json file exported from Tessera or Letopis</small></span></button>
            <a href="/api/export" onclick={() => (moreOpen = false)}><Icon name="download" /><span><b>Back up every story</b><small>One zip with every story and its history</small></span></a>
            <button onclick={() => { moreOpen = false; void stress(); }}><Icon name="activity" /><span><b>Create a stress-test story</b><small>Random library media, to check scrolling performance</small></span></button>
          </div>
        {/if}
      </div>
      <input type="file" accept=".json,application/json" hidden bind:this={fileInput} onchange={importFile} />
      <div class="split">
        <button class="btn primary" onclick={create}><Icon name="plus" /> New story</button>
        <button class="btn primary more" title="Start from a template" onclick={() => (tplOpen = !tplOpen)}><Icon name="chev-down" /></button>
        {#if tplOpen}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="menu-backdrop" onclick={() => (tplOpen = false)}></div>
          <div class="tpls">
            <div class="th">Start from a template</div>
            {#each TEMPLATES as t (t.id)}
              <button onclick={() => createFrom(t)}><Icon name={t.icon} /><span><b>{t.name}</b><small>{t.description}</small></span></button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    {#if allTags.length}
      <div class="tagbar">
        <button class="chip" class:on={!tag} onclick={() => (tag = null)}>All</button>
        {#each allTags as t (t)}<button class="chip" class:on={tag === t} onclick={() => (tag = tag === t ? null : t)}><Icon name="tag" size={11} /> {t}</button>{/each}
      </div>
    {/if}
    {#if hits.length}
      <div class="hits">
        <div class="muted small">Found inside stories</div>
        {#each hits as h, i (i)}
          <a class="hit" href="#/doc/{h.docId}{h.sectionId ? `?s=${h.sectionId}` : ''}"><b>{h.docTitle}</b> <span class="muted">· {h.field}</span> — {h.snippet}</a>
        {/each}
      </div>
    {/if}

    {#if docs === null}
      <p class="muted">Loading…</p>
    {:else if docs.length === 0}
      <div class="empty">
        <h2>Start your first chronicle</h2>
        <p class="muted">
          A story is a list of sections. Each section combines images or videos from your library with text. Reorder
          them freely, then present them full-screen.
        </p>
        <div class="row">
          <button class="btn primary" onclick={create}><Icon name="plus" /> New story</button>
          <button class="btn" onclick={stress} title="Builds a story from random library videos, GIFs and images">Create a stress-test story</button>
        </div>
      </div>
    {:else}
      <div class="grid">
        {#each shown as d (d.id)}
          {@const c = cover(d)}
          <article class="card">
            <a class="thumb" href="#/doc/{d.id}" aria-label="Open {d.title}">
              {#if c}<img src={c} alt="" loading="lazy" decoding="async" />{:else}<div class="ph"><Icon name="file" size={28} /></div>{/if}
            </a>
            <div class="meta">
              {#if renaming === d.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="input"
                  bind:value={renameValue}
                  autofocus
                  onkeydown={(e) => {
                    if (e.key === 'Enter') commitRename(d);
                    if (e.key === 'Escape') renaming = null;
                  }}
                  onblur={() => commitRename(d)}
                />
              {:else}
                <a class="title" href="#/doc/{d.id}">{d.title}</a>
              {/if}
              <div class="sub muted">
                {d.sections} section{d.sections === 1 ? '' : 's'} · {d.media} media · {fmtDate(d.updatedAt)}
              </div>
              {#if d.tags?.length}<div class="dtags">{#each d.tags as t (t)}<button class="chip" onclick={() => (tag = t)}>{t}</button>{/each}</div>{/if}
            </div>
            <div class="actions">
              <button class="icon-btn" title="Present" onclick={() => router.go(`/present/${d.id}`)}><Icon name="present" /></button>
              <button class="icon-btn" title="Rename" onclick={() => startRename(d)}><Icon name="notes" /></button>
              <button class="icon-btn" title="Duplicate" onclick={() => duplicate(d)}><Icon name="copy" /></button>
              <a class="icon-btn" title="Export JSON" href="/api/documents/{d.id}/export"><Icon name="download" /></a>
              <button class="icon-btn danger" title="Delete" onclick={() => remove(d)}><Icon name="trash" /></button>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </main>
</div>

<style>
  .split {
    position: relative;
    display: inline-flex;
  }
  .split .btn:first-child {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .split .more {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    padding: 0 8px;
    border-left: 1px solid #0003;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
  }
  .tpls {
    position: absolute;
    right: 0;
    top: 38px;
    z-index: 51;
    width: 360px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: var(--shadow);
  }
  .tpls .th {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    padding: 6px 8px;
  }
  .tpls button {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    width: 100%;
    padding: 8px;
    border: none;
    background: none;
    border-radius: 8px;
    text-align: left;
    cursor: pointer;
  }
  .tpls button:hover {
    background: var(--surface-2);
  }
  .tpls span {
    display: flex;
    flex-direction: column;
  }
  .tpls small {
    color: var(--text-3);
  }
  .tagbar,
  .dtags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-bottom: 14px;
  }
  .dtags {
    margin: 4px 0 0;
  }
  .tagbar .chip,
  .dtags .chip {
    border: none;
    cursor: pointer;
  }
  .chip.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .hits {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 16px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--surface);
  }
  .hit {
    color: var(--text);
    text-decoration: none;
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit:hover {
    color: var(--accent);
  }
  .small {
    font-size: 12px;
  }
  .page {
    height: 100%;
    overflow: auto;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .titles p {
    margin: 4px 0 0;
  }
  .search {
    width: 280px;
  }
  .menuwrap {
    position: relative;
  }
  .more-menu a {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 8px;
    border-radius: 8px;
    color: var(--text);
    text-decoration: none;
  }
  .more-menu a:hover {
    background: var(--surface-2);
  }
  main {
    padding: 36px 44px 60px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 24px;
  }
  h1 {
    font-size: 30px;
    font-weight: 800;
    letter-spacing: -0.03em;
    margin: 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 18px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    transition:
      transform 0.15s,
      border-color 0.15s;
  }
  .card:hover {
    border-color: var(--line-strong);
    transform: translateY(-2px);
  }
  .thumb {
    aspect-ratio: 16 / 9;
    background: var(--bg-2);
    display: block;
    overflow: hidden;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .ph {
    height: 100%;
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .meta {
    padding: 12px 14px 4px;
  }
  .title {
    font-weight: 650;
    font-size: 15px;
    color: var(--text);
    text-decoration: none;
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sub {
    font-size: 12px;
    margin-top: 2px;
  }
  .actions {
    display: flex;
    gap: 2px;
    padding: 4px 8px 8px;
  }
  .actions .danger:hover {
    color: var(--red);
  }
  .empty {
    max-width: 560px;
    padding: 40px;
    border: 1px dashed var(--line-strong);
    border-radius: 16px;
  }
  .empty h2 {
    font-size: 20px;
    font-weight: 700;
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    gap: 10px;
    margin-top: 16px;
  }
</style>
