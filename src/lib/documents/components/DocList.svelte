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
  import ThemeToggle from './ThemeToggle.svelte';

  let { authEnabled }: { authEnabled: boolean } = $props();
  let docs = $state<DocMeta[] | null>(null);
  let filter = $state('');
  let renaming = $state<string | null>(null);
  let renameValue = $state('');
  let fileInput: HTMLInputElement;
  let tag = $state<string | null>(null);
  let tplOpen = $state(false);
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

  async function logout() {
    await api.logout();
    location.reload();
  }

  function cover(d: DocMeta) {
    if (!d.coverPath) return '';
    const r = resolver.cache.get(d.coverLibraryId + '\u0000' + d.coverPath);
    return r?.found ? thumbUrl(r.derived, 1280) : '';
  }
</script>

<div class="page">
  <header class="top">
    <div class="brand">
      <img src="/tessera.svg" alt="" width="28" height="28" />
      <span class="name">Documents</span>
    </div>
    <input class="input search" placeholder="Search documents (titles, tags, text)…" bind:value={filter} />
    <div class="grow"></div>
    <button class="btn" onclick={() => fileInput.click()} title="Import a document (.json)"><Icon name="upload" /> Import</button>
    <a class="btn" href="/api/export" title="Download every document (with history) as a zip"><Icon name="download" /> Backup</a>
    <button class="icon-btn" title="System status" onclick={() => router.go('/system')}><Icon name="activity" /></button>
    <ThemeToggle />
    {#if authEnabled}<button class="icon-btn" title="Sign out" onclick={logout}><Icon name="logout" /></button>{/if}
    <input type="file" accept=".json,application/json" hidden bind:this={fileInput} onchange={importFile} />
  </header>

  <main>
    <div class="head">
      <h1>Documents</h1>
      <div class="split">
        <button class="btn primary" onclick={create}><Icon name="plus" /> New document</button>
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
        <div class="muted small">Found inside documents</div>
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
          A document is a list of sections. Each section combines images or videos from your library with text. Reorder
          them freely, then present them full-screen.
        </p>
        <div class="row">
          <button class="btn primary" onclick={create}><Icon name="plus" /> New document</button>
          <button class="btn" onclick={stress} title="Builds a document from random library videos, GIFs and images">Create a stress-test document</button>
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
      <p class="foot muted">
        <button class="linkish" onclick={stress}>Create a stress-test document</button> from random library media to check
        scrolling performance.
      </p>
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
  .top {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 20px;
    background: color-mix(in srgb, var(--bg) 85%, transparent);
    backdrop-filter: blur(10px);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-right: 14px;
  }
  .name {
    font: 650 18px var(--serif);
  }
  .search {
    width: 260px;
  }
  .grow {
    flex: 1;
  }
  a.btn {
    text-decoration: none;
  }
  main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 28px 20px 60px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }
  h1 {
    font: 650 30px var(--serif);
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
    font: 600 16px var(--serif);
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
    font: 600 22px var(--serif);
    margin: 0 0 8px;
  }
  .row {
    display: flex;
    gap: 10px;
    margin-top: 16px;
  }
  .foot {
    margin-top: 28px;
    font-size: 13px;
  }
  .linkish {
    background: none;
    border: none;
    color: var(--blue);
    padding: 0;
    cursor: pointer;
    text-decoration: underline;
  }
</style>
