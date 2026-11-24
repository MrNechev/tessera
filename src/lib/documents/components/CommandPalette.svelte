<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { api } from '../lib/api';
  import { palette, score, type Command } from '../lib/palette.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { DirNode, DocMeta, FileItem, SearchHit } from '../lib/types';
  import { api as tApi } from '../../api/backend';
  import type { ProjectSummary } from '../../api/types';
  import { openProjectPath } from '../../editor/session.svelte';
  import { openInLibrary } from '../../commands';
  import Icon from './Icon.svelte';

  let q = $state('');
  let sel = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();
  let docs = $state<DocMeta[]>([]);
  let folders = $state<{ libraryId: string; libName: string; path: string; count: number }[]>([]);
  let hits = $state<SearchHit[]>([]);
  let files = $state<FileItem[]>([]);
  let projects = $state<ProjectSummary[]>([]);
  let searchTimer: ReturnType<typeof setTimeout>;

  // folders are cached for the session (a big library has thousands)
  let folderCache: typeof folders | null = null;

  function flatten(n: DirNode, libraryId: string, libName: string, out: typeof folders) {
    for (const c of n.children ?? []) {
      out.push({ libraryId, libName, path: c.path, count: c.total });
      flatten(c, libraryId, libName, out);
    }
  }

  onMount(() => {
    api.docs().then((d) => (docs = d)).catch(() => {});
    tApi.listProjects().then((p) => (projects = p)).catch(() => {});
    if (folderCache) folders = folderCache;
    else
      api.libraries().then(async (libs) => {
        const out: typeof folders = [];
        for (const l of libs) {
          try {
            flatten(await api.tree(l.id), l.id, l.name, out);
          } catch {}
        }
        folders = folderCache = out;
      });
    tick().then(() => input?.focus());
  });

  $effect(() => {
    const s = q.trim();
    clearTimeout(searchTimer);
    if (s.length < 3) {
      hits = [];
      files = [];
      return;
    }
    searchTimer = setTimeout(() => {
      api.search(s).then((h) => (hits = h.slice(0, 30))).catch(() => {});
      api.files({ q: s, limit: 8, sort: 'mtime', desc: true }).then((r) => (files = r.items)).catch(() => {});
    }, 200);
  });

  const results = $derived.by(() => {
    const s = q.trim().toLowerCase();
    const all: Command[] = palette.commands();
    for (const p of projects)
      all.push({
        id: 'proj:' + p.id,
        group: 'Collages',
        icon: 'layout',
        title: p.name,
        subtitle: `${p.width}×${p.height} · ${new Date(p.modified).toLocaleDateString()}`,
        keywords: 'collage project',
        run: () => openProjectPath(p.path, false),
      });
    for (const d of docs)
      all.push({
        id: 'doc:' + d.id,
        group: 'Stories',
        icon: 'file',
        title: d.title || 'Untitled',
        subtitle: d.tags?.length ? d.tags.join(' · ') : `${d.sections} sections`,
        keywords: d.tags?.join(' '),
        run: () => router.go(`/doc/${d.id}`),
      });
    if (s)
      for (const f of folders)
        all.push({
          id: 'dir:' + f.libraryId + f.path,
          group: 'Folders',
          icon: 'folder',
          title: f.path.split('/').pop()!,
          subtitle: `${f.libName} / ${f.path} · ${f.count}`,
          keywords: f.path,
          run: () => openInLibrary(f.libraryId, f.path),
        });
    const scored = all
      .map((c) => ({ c, sc: Math.max(score(c.title, s), score((c.keywords ?? '') + ' ' + (c.subtitle ?? ''), s) - 300) }))
      .filter((x) => x.sc > 0);
    scored.sort((a, b) => b.sc - a.sc);
    const top = scored.slice(0, s ? 60 : 40).map((x) => x.c);
    for (const f of files)
      top.push({
        id: 'file:' + f.libraryId + f.path,
        group: 'Library files',
        icon: f.kind === 'video' ? 'video' : 'image',
        title: f.name,
        subtitle: f.dir,
        run: () => openInLibrary(f.libraryId, f.dir, f.name),
      });
    for (const h of hits)
      top.push({
        id: 'hit:' + h.docId + h.section + h.field + h.snippet.slice(0, 20),
        group: 'Found in stories',
        icon: 'search',
        title: h.snippet,
        subtitle: `${h.docTitle}${h.section >= 0 ? ` · section ${h.section + 1}` : ''} · ${h.field}`,
        run: () => router.go(`/doc/${h.docId}${h.sectionId ? `?s=${h.sectionId}` : ''}`),
      });
    // keep groups together in first-appearance order
    const order: string[] = [];
    for (const c of top) if (!order.includes(c.group)) order.push(c.group);
    return order.flatMap((g) => top.filter((c) => c.group === g));
  });

  $effect(() => {
    void q;
    sel = 0;
  });

  async function run(c: Command | undefined) {
    if (!c) return;
    palette.open = false;
    try {
      await c.run();
    } catch (e) {
      toasts.error((e as Error).message);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      sel = Math.min(results.length - 1, sel + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      sel = Math.max(0, sel - 1);
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run(results[sel]);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      palette.open = false;
    } else return;
    tick().then(() => list?.querySelector('.it.on')?.scrollIntoView({ block: 'nearest' }));
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="pal-bg" onclick={() => (palette.open = false)}>
  <div class="pal" onclick={(e) => e.stopPropagation()}>
    <div class="in">
      <Icon name="search" />
      <input bind:this={input} bind:value={q} onkeydown={onKey} placeholder="Search commands, collages, stories, library files…" spellcheck="false" />
      <span class="kbd">Esc</span>
    </div>
    <div class="list" bind:this={list}>
      {#each results as c, i (c.id)}
        {#if i === 0 || results[i - 1].group !== c.group}<div class="grp">{c.group}</div>{/if}
        <button class="it" class:on={i === sel} onmousemove={() => (sel = i)} onclick={() => run(c)}>
          <Icon name={c.icon ?? 'chev-right'} size={15} />
          <span class="t">{c.title}</span>
          {#if c.subtitle}<span class="s">{c.subtitle}</span>{/if}
          {#if c.shortcut}<span class="kbd">{c.shortcut}</span>{/if}
        </button>
      {:else}
        <div class="none muted">Nothing found</div>
      {/each}
    </div>
  </div>
</div>

<style>
  .pal-bg {
    position: fixed;
    inset: 0;
    z-index: 900;
    background: #0006;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
  }
  .pal {
    width: min(680px, calc(100% - 32px));
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    box-shadow: var(--shadow);
    overflow: hidden;
  }
  .in {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
    color: var(--text-3);
  }
  .in input {
    flex: 1;
    border: none;
    background: none;
    outline: none;
    font-size: 16px;
    color: var(--text);
  }
  .list {
    overflow-y: auto;
    padding: 6px;
  }
  .grp {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-3);
    padding: 8px 8px 4px;
  }
  .it {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    background: none;
    border-radius: 8px;
    text-align: left;
    cursor: pointer;
    color: var(--text);
  }
  .it.on {
    background: var(--accent-soft);
  }
  .it .t {
    flex: none;
    max-width: 55%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .it .s {
    flex: 1;
    min-width: 0;
    color: var(--text-3);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    padding: 20px;
    text-align: center;
  }
</style>
