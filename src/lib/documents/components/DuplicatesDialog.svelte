<script lang="ts">
  // Duplicate finder: identical files (same content) and visually similar photos. Tessera
  // never deletes anything; this shows where the copies are so you can tidy up yourself.
  import { onMount } from 'svelte';
  import { api, fmtBytes, fmtDate } from '../lib/api';
  import type { FileItem } from '../lib/types';
  import Icon from './Icon.svelte';

  let { onclose, onopen }: { onclose: () => void; onopen: (f: FileItem) => void } = $props();

  let groups = $state<{ kind: 'identical' | 'similar'; files: FileItem[] }[] | null>(null);
  let pending = $state(0);
  let dist = $state(5);
  let tab = $state<'identical' | 'similar'>('identical');
  let error = $state('');

  async function load() {
    groups = null;
    try {
      const r = await api.duplicates(dist);
      groups = r.groups;
      pending = r.hashPending;
    } catch (e) {
      error = (e as Error).message;
    }
  }

  onMount(() => {
    load();
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  const shown = $derived((groups ?? []).filter((g) => g.kind === tab));
  const wasted = $derived(
    (groups ?? []).filter((g) => g.kind === 'identical').reduce((a, g) => a + g.files[0].size * (g.files.length - 1), 0),
  );

  function copyPath(f: FileItem) {
    navigator.clipboard?.writeText(f.path).catch(() => {});
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog dd" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Duplicates</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="tabs">
      <button class:on={tab === 'identical'} onclick={() => (tab = 'identical')}>Identical files {#if groups}<span class="muted">{groups.filter((g) => g.kind === 'identical').length}</span>{/if}</button>
      <button class:on={tab === 'similar'} onclick={() => (tab = 'similar')}>Similar photos {#if groups}<span class="muted">{groups.filter((g) => g.kind === 'similar').length}</span>{/if}</button>
      <span class="grow"></span>
      {#if tab === 'similar'}
        <label class="muted sens">Sensitivity
          <select class="input" bind:value={dist} onchange={load}>
            <option value={2}>strict</option><option value={5}>normal</option><option value={9}>loose</option>
          </select>
        </label>
      {/if}
    </div>
    <div class="body">
      <p class="muted note">
        Tessera never deletes library files. Use this list to find copies, then tidy up in your file manager.
        {#if tab === 'identical' && wasted}Removing the extra copies would free about <b>{fmtBytes(wasted)}</b>.{/if}
        {#if tab === 'similar' && pending}Still fingerprinting {pending.toLocaleString()} images in the background; reopen later for more.{/if}
      </p>
      {#if error}<p class="err">{error}</p>
      {:else if !groups}<p class="muted">Looking…</p>
      {:else if !shown.length}<p>No {tab === 'identical' ? 'identical files' : 'similar photos'} found.</p>
      {:else}
        {#each shown.slice(0, 300) as g, gi (gi)}
          <div class="grp">
            {#each g.files as f (f.id)}
              <div class="it">
                <img src="/d/{f.key}/t320.webp" alt="" loading="lazy" />
                <div class="meta">
                  <div class="p" title={f.path}>{f.path}</div>
                  <div class="muted">{fmtBytes(f.size)}{f.width ? ` · ${f.width}×${f.height}` : ''} · {fmtDate(f.taken || f.mtime)}</div>
                  <div class="acts">
                    <button class="btn sm" onclick={() => onopen(f)}><Icon name="folder" size={13} /> Show in library</button>
                    <button class="btn sm ghost" onclick={() => copyPath(f)}><Icon name="copy" size={13} /> Copy path</button>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/each}
        {#if shown.length > 300}<p class="muted">…and {shown.length - 300} more groups.</p>{/if}
      {/if}
    </div>
  </div>
</div>

<style>
  .dd {
    width: min(980px, 100%);
    height: calc(100vh - 48px);
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    border: none;
    background: none;
    padding: 6px 10px;
    border-radius: 6px;
    cursor: pointer;
    font-weight: 600;
    color: var(--text-2);
  }
  .tabs button.on {
    background: var(--surface-3);
    color: var(--text);
  }
  .grow {
    flex: 1;
  }
  .sens {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .sens .input {
    height: 28px;
  }
  .note {
    margin-top: 0;
  }
  .grp {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 8px;
    padding: 10px;
    margin-bottom: 10px;
    border: 1px solid var(--line-strong);
    border-radius: 10px;
  }
  .it {
    display: flex;
    gap: 10px;
    min-width: 0;
  }
  .it img {
    width: 96px;
    height: 72px;
    object-fit: cover;
    border-radius: 6px;
    background: var(--bg-2);
    flex: none;
  }
  .meta {
    min-width: 0;
    font-size: 12.5px;
  }
  .p {
    overflow-wrap: anywhere;
    font-weight: 600;
  }
  .acts {
    display: flex;
    gap: 4px;
    margin-top: 4px;
  }
  .err {
    color: var(--red);
  }
</style>
