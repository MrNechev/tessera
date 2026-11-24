<script lang="ts">
  import { onMount } from 'svelte';
  import { api, fmtBytes, fmtDate } from '../lib/api';
  import type { DocState } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { toasts } from '../lib/toast.svelte';
  import { toHTML } from '../lib/tiptap';
  import type { Doc, HistoryEntry } from '../lib/types';
  import Icon from './Icon.svelte';

  let { ds, onclose }: { ds: DocState; onclose: () => void } = $props();
  let entries = $state<HistoryEntry[] | null>(null);
  let preview = $state<Doc | null>(null);
  let busy = $state(false);

  async function load() {
    entries = await api.history(ds.doc!.id);
  }

  onMount(() => {
    load();
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  async function show(v: number) {
    preview = await api.version(ds.doc!.id, v);
    resolver.prefetch(preview.sections.flatMap((s) => s.media));
  }

  async function snapshot() {
    await ds.save();
    await api.snapshot(ds.doc!.id);
    toasts.show('Version saved', 'ok');
    load();
  }

  async function restore() {
    if (!preview || !confirm(`Restore version ${preview.version}? The current version stays in the history.`)) return;
    busy = true;
    try {
      await ds.save();
      const d = await api.restore(ds.doc!.id, preview.version);
      ds.replace(d);
      toasts.show(`Restored version ${preview.version}`, 'ok');
      onclose();
    } catch (e) {
      toasts.error((e as Error).message);
    } finally {
      busy = false;
    }
  }

  function thumb(m: { libraryId: string; path: string }) {
    const r = resolver.cache.get(m.libraryId + '\u0000' + m.path);
    return r?.found ? `${r.derived}t320.webp` : '';
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog wide" onclick={(e) => e.stopPropagation()}>
    <header>
      <Icon name="history" />
      <h2>Version history</h2>
      <button class="btn sm" onclick={snapshot}>Save a version now</button>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="cols">
      <div class="list">
        <div class="cur">
          <b>Current</b> · v{ds.doc?.version}
          <div class="muted">{ds.doc ? fmtDate(ds.doc.updatedAt) : ''}</div>
        </div>
        {#if entries === null}
          <p class="muted pad">Loading…</p>
        {:else if entries.length === 0}
          <p class="muted pad">No earlier versions yet. A version is kept automatically every few minutes while you edit.</p>
        {:else}
          {#each entries as e (e.version)}
            <button class="ent" class:sel={preview?.version === e.version} onclick={() => show(e.version)}>
              <b>v{e.version}</b> · {fmtDate(e.savedAt)}
              <div class="muted">{e.title} · {e.sections} sections · {e.media} media · {fmtBytes(e.bytes)}</div>
            </button>
          {/each}
        {/if}
      </div>
      <div class="pv">
        {#if preview}
          <div class="pvhead">
            <h3>{preview.title} <span class="muted">v{preview.version}</span></h3>
            <button class="btn primary sm" disabled={busy} onclick={restore}>Restore this version</button>
          </div>
          {#each preview.sections as s, i (s.id)}
            <div class="ps">
              <div class="pst"><span class="muted">{i + 1}</span> {s.title || ''}</div>
              {#if s.media.length}
                <div class="thumbs">
                  {#each s.media.slice(0, 8) as m (m.path)}
                    {@const t = thumb(m)}
                    {#if t}<img src={t} alt="" loading="lazy" />{:else}<span class="miss" title={m.path}>?</span>{/if}
                  {/each}
                </div>
              {/if}
              <div class="prose small">{@html toHTML(s.text)}</div>
            </div>
          {/each}
        {:else}
          <p class="muted pad">Select a version to preview it.</p>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .wide {
    width: min(1100px, 100%);
    height: min(780px, calc(100vh - 48px));
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 300px 1fr;
  }
  .list {
    overflow: auto;
    border-right: 1px solid var(--line);
    padding: 8px;
  }
  .cur {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    margin-bottom: 6px;
  }
  .ent {
    display: block;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    background: none;
    border-radius: 8px;
    cursor: pointer;
  }
  .ent:hover {
    background: var(--surface-2);
  }
  .ent.sel {
    background: var(--accent-soft);
  }
  .ent .muted,
  .cur .muted {
    font-size: 12px;
  }
  .pv {
    overflow: auto;
    padding: 14px 18px;
  }
  .pvhead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 10px;
  }
  .pvhead h3 {
    margin: 0;
    font: 650 17px var(--ui);
  }
  .ps {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 12px;
    margin-bottom: 10px;
  }
  .pst {
    font-weight: 600;
    margin-bottom: 6px;
  }
  .thumbs {
    display: flex;
    gap: 4px;
    margin-bottom: 6px;
  }
  .thumbs img,
  .miss {
    width: 64px;
    height: 44px;
    object-fit: cover;
    border-radius: 4px;
    background: var(--surface-3);
    display: grid;
    place-items: center;
  }
  .small {
    font-size: 14px;
  }
  .pad {
    padding: 10px;
  }
</style>
