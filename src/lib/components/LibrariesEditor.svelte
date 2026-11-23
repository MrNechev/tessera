<script lang="ts">
  /**
   * Desktop: the folders Tessera shows in its Library and uses in documents.
   * Each one is read-only (the default) or read-write for the image tools.
   */
  import { FolderPlus, Lock, LockOpen, Trash2 } from '@lucide/svelte';
  import { pickFolder } from '../api/backend';
  import type { LibrarySetting } from '../api/types';
  import Segmented from './ui/Segmented.svelte';

  interface Props {
    value: LibrarySetting[];
    onchange: (v: LibrarySetting[]) => void;
  }
  let { value, onchange }: Props = $props();

  function slug(name: string, taken: Set<string>): string {
    const base =
      name
        .toLowerCase()
        .normalize('NFKD')
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '')
        .slice(0, 40) || 'library';
    let id = base;
    for (let i = 2; taken.has(id); i++) id = `${base}-${i}`;
    return id;
  }

  async function add() {
    const dir = await pickFolder('Add a folder with photos and videos');
    if (!dir || value.some((l) => l.path === dir)) return;
    const name = dir.split('/').filter(Boolean).pop() ?? 'Library';
    onchange([...value, { id: slug(name, new Set(value.map((l) => l.id))), name, path: dir, writable: false }]);
  }

  function update(i: number, patch: Partial<LibrarySetting>) {
    onchange(value.map((l, j) => (j === i ? { ...l, ...patch } : l)));
  }
</script>

<div class="libs">
  {#each value as l, i (l.id)}
    <div class="lib">
      <input class="input name" value={l.name} title="Name shown in the Library" onchange={(e) => update(i, { name: (e.target as HTMLInputElement).value || l.name })} />
      <span class="path truncate" title={l.path}>{l.path}</span>
      <Segmented
        full={false}
        options={[
          { value: 'ro', label: 'Read-only', icon: Lock, title: 'Tessera never changes files here' },
          { value: 'rw', label: 'Read-write', icon: LockOpen, title: 'The tools may save into this folder and replace originals' },
        ]}
        value={l.writable ? 'rw' : 'ro'}
        onchange={(v) => update(i, { writable: v === 'rw' })}
      />
      <button class="icon-btn small" title="Remove from Tessera (the folder itself is not touched)" onclick={() => onchange(value.filter((_, j) => j !== i))}><Trash2 size={14} /></button>
    </div>
  {:else}
    <p class="hint">No folders yet. Add the folders with your photos and videos.</p>
  {/each}
  <button class="btn small add" onclick={() => void add()}><FolderPlus size={14} /> Add folder…</button>
</div>

<style>
  .libs {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .lib {
    display: grid;
    grid-template-columns: 150px 1fr auto auto;
    align-items: center;
    gap: 10px;
  }
  .name {
    min-width: 0;
  }
  .path {
    color: var(--text-2);
    font-size: 12px;
  }
  .add {
    align-self: flex-start;
  }
  .hint {
    color: var(--text-3);
    margin: 0;
  }
</style>
