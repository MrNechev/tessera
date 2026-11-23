<script lang="ts">
  /**
   * Web mode: choose files or folders on the server (libraries, Outputs,
   * Inbox) or upload from this computer. Desktop uses native dialogs instead.
   */
  import { ArrowUp, File as FileIcon, Folder, HardDrive, Image as ImageIcon, Upload } from '@lucide/svelte';
  import Modal from './ui/Modal.svelte';
  import { picker } from '../platform/picker.svelte';
  import { chooseLocalFiles, sendFiles, type UploadDest } from '../platform/upload';
  import { formatBytes as fmtBytes } from '../stores/batch.svelte';

  interface Entry {
    name: string;
    path: string;
    dir: boolean;
    size: number;
    modified: number;
    image: boolean;
  }
  interface Listing {
    path: string;
    parent: string | null;
    writable: boolean;
    entries: Entry[];
  }

  const req = $derived(picker.current!);
  let listing = $state<Listing | null>(null);
  let error = $state('');
  let loading = $state(false);
  let selected = $state<Set<string>>(new Set());
  let fileName = $state('');
  let keepInInbox = $state(false);
  let uploading = $state<string | null>(null);
  let hasInbox = $state(false);

  const exts = $derived(new Set((req.extensions ?? []).map((e) => e.toLowerCase())));
  function shown(e: Entry): boolean {
    if (e.dir) return true;
    if (req.mode === 'folder') return false;
    if (!exts.size) return true;
    const dot = e.name.lastIndexOf('.');
    return dot > 0 && exts.has(e.name.slice(dot + 1).toLowerCase());
  }
  const entries = $derived(listing ? listing.entries.filter(shown) : []);

  async function go(path: string) {
    loading = true;
    error = '';
    try {
      const res = await fetch(`/api/t/browse?path=${encodeURIComponent(path)}`, { credentials: 'same-origin' });
      const body = await res.json();
      if (!res.ok) throw new Error(body.error ?? res.statusText);
      listing = body;
      selected = new Set();
      if (body.path) picker.lastFolder = body.path;
      if (!body.path) hasInbox = body.entries.some((e: Entry) => /inbox/i.test(e.name));
    } catch (e) {
      error = String(e instanceof Error ? e.message : e);
      if (path) await go('');
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    fileName = req.fileName ?? '';
    void go(req.startPath ?? picker.lastFolder ?? '');
  });

  function toggle(e: Entry) {
    const s = new Set(req.multiple === false ? [] : selected);
    if (selected.has(e.path)) s.delete(e.path);
    else s.add(e.path);
    selected = s;
  }

  function open(e: Entry) {
    if (e.dir) void go(e.path);
    else if (req.mode === 'files') picker.finish([e.path]);
    else if (req.mode === 'save') fileName = e.name;
  }

  async function upload() {
    const files = await chooseLocalFiles(req.extensions ?? [], req.multiple !== false);
    if (!files.length) return;
    const dest: UploadDest = keepInInbox ? 'inbox' : 'temp';
    try {
      const paths = await sendFiles(files, dest, (p) => {
        uploading = `Uploading ${p.index + 1} of ${p.count}: ${p.file} (${Math.round((100 * p.sent) / Math.max(1, p.total))}%)`;
      });
      uploading = null;
      picker.finish(paths);
    } catch (e) {
      uploading = null;
      error = String(e instanceof Error ? e.message : e);
    }
  }

  const canConfirm = $derived(
    req.mode === 'files'
      ? selected.size > 0 || entries.some((e) => !e.dir && selected.has(e.path))
      : req.mode === 'folder'
        ? !!listing?.path && (!req.writableOnly || listing.writable)
        : !!listing?.path && listing.writable && fileName.trim().length > 0,
  );

  function confirm() {
    if (!canConfirm || !listing) return;
    if (req.mode === 'files') picker.finish([...selected]);
    else if (req.mode === 'folder') picker.finish([listing.path]);
    else picker.finish([`${listing.path.replace(/\/$/, '')}/${fileName.trim()}`]);
  }

  const crumbs = $derived.by(() => {
    if (!listing?.path) return [];
    const parts = listing.path.split('/').filter(Boolean);
    return parts.map((name, i) => ({ name, path: '/' + parts.slice(0, i + 1).join('/') }));
  });
</script>

<Modal title={req.title} width={720} onclose={() => picker.finish(null)}>
  <div class="bar">
    <button class="icon-btn small" title="Up" disabled={!listing?.path} onclick={() => void go(listing?.parent ?? '')}>
      <ArrowUp size={15} />
    </button>
    <button class="crumb" onclick={() => void go('')}><HardDrive size={13} /> Server</button>
    {#each crumbs.slice(-4) as c (c.path)}
      <span class="sep">/</span><button class="crumb" onclick={() => void go(c.path)}>{c.name}</button>
    {/each}
    {#if listing?.path && !listing.writable}<span class="ro" title="Tessera can read but not write here">read-only</span>{/if}
  </div>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="list" class:loading>
    {#each entries as e (e.path)}
      <div
        class="row"
        class:sel={selected.has(e.path)}
        role="button"
        tabindex="0"
        onclick={() => (e.dir || req.mode !== 'files' ? open(e) : toggle(e))}
        ondblclick={() => open(e)}
        onkeydown={(k) => k.key === 'Enter' && open(e)}
      >
        {#if !listing?.path}<HardDrive size={16} />{:else if e.dir}<Folder size={16} />{:else if e.image}<ImageIcon size={16} />{:else}<FileIcon size={16} />{/if}
        <span class="name">{e.name}</span>
        {#if !e.dir}<span class="size">{fmtBytes(e.size)}</span>{/if}
      </div>
    {:else}
      {#if !loading}<p class="empty">{listing?.path ? 'Nothing here.' : 'No folders are configured on this server.'}</p>{/if}
    {/each}
  </div>
  {#if req.mode === 'save'}
    <label class="fname">File name <input class="input" bind:value={fileName} onkeydown={(k) => k.key === 'Enter' && confirm()} /></label>
  {/if}
  {#if uploading}<p class="hint">{uploading}</p>{/if}
  {#snippet footer()}
    {#if req.mode === 'files'}
      <div class="upload">
        <button class="btn" onclick={() => void upload()} disabled={!!uploading}><Upload size={15} /> Upload from this computer…</button>
        {#if hasInbox}<label class="keep"><input type="checkbox" bind:checked={keepInInbox} /> Keep in Inbox</label>{/if}
      </div>
    {/if}
    <button class="btn" onclick={() => picker.finish(null)}>Cancel</button>
    <button class="btn primary" disabled={!canConfirm} onclick={confirm}>
      {req.mode === 'files' ? `Add ${selected.size || ''}`.trim() : req.mode === 'folder' ? 'Choose this folder' : 'Save'}
    </button>
  {/snippet}
</Modal>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .crumb {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px;
    border-radius: 6px;
    color: var(--text);
  }
  .crumb:hover {
    background: var(--panel-2);
  }
  .sep {
    color: var(--text-3);
  }
  .ro {
    margin-left: auto;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 99px;
    background: var(--panel-2);
    color: var(--text-3);
  }
  .list {
    height: min(52vh, 460px);
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 4px;
  }
  .list.loading {
    opacity: 0.6;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 7px;
    cursor: pointer;
    font-size: 13px;
    user-select: none;
  }
  .row:hover {
    background: var(--panel-2);
  }
  .row.sel {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .size {
    color: var(--text-3);
    font-size: 12px;
  }
  .empty,
  .hint {
    color: var(--text-3);
    font-size: 13px;
    padding: 10px;
  }
  .error {
    color: var(--danger, #e5484d);
    font-size: 13px;
    margin: 4px 0 8px;
  }
  .fname {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    font-size: 13px;
  }
  .fname input {
    flex: 1;
  }
  .upload {
    margin-right: auto;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .keep {
    font-size: 12px;
    display: flex;
    align-items: center;
    gap: 4px;
  }
</style>
