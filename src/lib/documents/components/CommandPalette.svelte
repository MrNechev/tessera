<script lang="ts">
  import { app } from '../../stores/app.svelte';
  import { onMount, tick } from 'svelte';
  import { api } from '../lib/api';
  import { palette, score, type Command } from '../lib/palette.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { DirNode, DocMeta, SearchHit } from '../lib/types';
  import Icon from './Icon.svelte';

  let q = $state('');
  let sel = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();
  let docs = $state<DocMeta[]>([]);
  let folders = $state<{ libraryId: string; libName: string; path: string; count: number }[]>([]);
  let hits = $state<SearchHit[]>([]);
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

  function themeToggle() {
    const cur = document.documentElement.dataset.theme === 'light' ? 'light' : 'dark';
    void app.saveSettings({ theme: cur === 'dark' ? 'light' : 'dark' });
  }

  const base: Command[] = [
    { id: 'new-doc', group: 'Actions', icon: 'plus', title: 'New document', run: async () => router.go(`/doc/${(await api.createDoc('Untitled')).id}`) },
    { id: 'home', group: 'Actions', icon: 'home', title: 'All documents', run: () => router.go('/') },
    { id: 'system', group: 'Actions', icon: 'activity', title: 'System page', keywords: 'status queues gpu cache', run: () => router.go('/system') },
    { id: 'theme', group: 'Actions', icon: 'moon', title: 'Switch light / dark theme', run: themeToggle },
  ];

  $effect(() => {
    const s = q.trim();
    clearTimeout(searchTimer);
    if (s.length < 3) {
      hits = [];
      return;
    }
    searchTimer = setTimeout(() => api.search(s).then((h) => (hits = h.slice(0, 30))).catch(() => {}), 200);
  });

  const results = $derived.by(() => {
    const s = q.trim().toLowerCase();
    const all: Command[] = [...palette.commands(), ...base];
    for (const d of docs)
      all.push({
        id: 'doc:' + d.id,
        group: 'Documents',
        icon: 'file',
        title: d.title || 'Untitled',
        subtitle: d.tags?.length ? d.tags.join(' · ') : `${d.sections} sections`,
        keywords: d.tags?.join(' '),
        run: () => router.go(`/doc/${d.id}`),
      });
    if (palette.openFolder && s)
      for (const f of folders)
        all.push({
          id: 'dir:' + f.libraryId + f.path,
          group: 'Folders',
          icon: 'folder',
          title: f.path.split('/').pop()!,
          subtitle: `${f.libName} / ${f.path} · ${f.count}`,
          keywords: f.path,
          run: () => palette.openFolder?.(f.libraryId, f.path),
        });
    const scored = all
      .map((c) => ({ c, sc: Math.max(score(c.title, s), score((c.keywords ?? '') + ' ' + (c.subtitle ?? ''), s) - 300) }))
      .filter((x) => x.sc > 0);
    scored.sort((a, b) => b.sc - a.sc);
    const top = scored.slice(0, s ? 60 : 40).map((x) => x.c);
    for (const h of hits)
      top.push({
        id: 'hit:' + h.docId + h.section + h.field + h.snippet.slice(0, 20),
        group: 'Found in documents',
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
      <input bind:this={input} bind:value={q} onkeydown={onKey} placeholder="Jump to a section, document, folder… or search text" spellcheck="false" />
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
