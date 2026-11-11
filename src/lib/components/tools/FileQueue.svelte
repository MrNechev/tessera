<script lang="ts">
  import { FilePlus, FolderPlus, LoaderCircle, Trash2, X } from '@lucide/svelte';
  import { pickFolder, pickImages } from '../../api/backend';
  import { app } from '../../stores/app.svelte';
  import { formatBytes, type BatchQueue } from '../../stores/batch.svelte';

  interface Props {
    queue: BatchQueue;
    selected?: string | null;
    onselect?: (path: string) => void;
  }
  let { queue, selected = null, onselect }: Props = $props();

  const total = $derived(queue.files.reduce((s, f) => s + f.fileSize, 0));

  async function addFiles() {
    const files = await pickImages(app.settings.recentImportDir);
    await queue.add(files);
  }
  async function addFolder() {
    const dir = await pickFolder('Add a folder', app.settings.recentImportDir);
    if (dir) await queue.add([dir]);
  }
</script>

<div class="queue card">
  <header>
    <div>
      <h3>Files</h3>
      <span class="hint">{queue.files.length} image{queue.files.length === 1 ? '' : 's'} · {formatBytes(total)}</span>
    </div>
    <span class="spacer"></span>
    {#if queue.adding}<LoaderCircle size={16} class="spin" />{/if}
    <button class="btn small" onclick={addFiles}><FilePlus size={14} /> Add files</button>
    <button class="btn small" onclick={addFolder}><FolderPlus size={14} /> Add folder</button>
    {#if queue.files.length}
      <button class="icon-btn small" title="Clear list" onclick={() => queue.clear()} disabled={queue.running}><Trash2 size={14} /></button>
    {/if}
  </header>
  {#if queue.files.length}
    <div class="list">
      {#each queue.files as f (f.path)}
        <div
          class="file"
          class:sel={selected === f.path}
          role="button"
          tabindex="0"
          onclick={() => onselect?.(f.path)}
          onkeydown={(e) => e.key === 'Enter' && onselect?.(f.path)}
        >
          <span class="fmt">{f.format.toUpperCase()}</span>
          <span class="name truncate" title={f.path}>{f.path.split('/').pop()}</span>
          <span class="meta">{f.width}×{f.height}</span>
          <span class="meta size">{formatBytes(f.fileSize)}</span>
          <button class="icon-btn small" title="Remove" disabled={queue.running} onclick={(e) => { e.stopPropagation(); queue.remove(f.path); }}><X size={13} /></button>
        </div>
      {/each}
    </div>
  {:else}
    <div class="drop">
      <p><b>Drop images or folders here</b></p>
      <p class="hint">JPEG, PNG, WebP, AVIF, HEIC, camera RAW, TIFF, JPEG XL and more</p>
    </div>
  {/if}
</div>

<style>
  .queue {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  h3 {
    font-size: 14px;
  }
  .list {
    overflow-y: auto;
    padding: 6px;
    flex: 1;
  }
  .file {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 6px 0 10px;
    border-radius: 7px;
    cursor: pointer;
  }
  .file:hover {
    background: var(--hover);
  }
  .file.sel {
    background: var(--accent-soft);
  }
  .fmt {
    width: 44px;
    font-size: 10px;
    font-weight: 700;
    color: var(--text-3);
    text-align: center;
    padding: 2px 0;
    border-radius: 4px;
    background: var(--panel-3);
  }
  .name {
    flex: 1;
    min-width: 0;
  }
  .meta {
    color: var(--text-3);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .size {
    width: 72px;
    text-align: right;
  }
  .drop {
    flex: 1;
    margin: 16px;
    border: 1.5px dashed var(--border-strong);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 30px;
    color: var(--text-2);
  }
  .drop p {
    margin: 4px 0;
  }
</style>
