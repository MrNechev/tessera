<script lang="ts">
  /**
   * "My cut-outs" in the collage Stickers panel: the transparent PNGs in
   * Outputs/Cut-outs (they're ordinary library files), newest first. Click
   * to add one to the page like a sticker.
   */
  import { Plus, Scissors } from '@lucide/svelte';
  import { pickImages } from '../api/backend';
  import { api as docApi } from '../documents/lib/api';
  import type { FileItem } from '../documents/lib/types';
  import { addCutout } from './collage';
  import { cutoutDialog, CUTOUTS_DIR, openCutout, OUTPUTS_LIBRARY } from './open.svelte';

  let items = $state<FileItem[]>([]);
  let loaded = $state(false);

  async function load() {
    try {
      const r = await docApi.files({ library: OUTPUTS_LIBRARY, dir: CUTOUTS_DIR, kind: 'image', sort: 'mtime', desc: true, limit: 120 });
      items = r.items;
    } catch {
      // No Outputs folder (yet), or the folder isn't indexed: nothing to show.
      items = [];
    }
    loaded = true;
  }

  // Load now and after every new cut-out.
  $effect(() => {
    void cutoutDialog.saved;
    void load();
  });

  async function cutNew() {
    const [path] = await pickImages(undefined, 'Cut out a photo');
    if (!path) return;
    openCutout({
      source: { path, name: path.split('/').pop() ?? 'photo' },
      actions: [{ id: 'add', label: 'Add to collage' }],
      onsaved: (r) => void addCutout(r.ref),
    });
  }

  const mediaUrl = (f: FileItem) => `/media/${encodeURIComponent(f.libraryId)}/${f.path.split('/').map(encodeURIComponent).join('/')}`;
</script>

<div class="mine">
  <div class="title">
    <span class="label">My cut-outs</span>
    <button class="btn small" title="Remove the background of a photo and use it as a sticker" onclick={cutNew}><Scissors size={13} /> New…</button>
  </div>
  {#if items.length}
    <div class="grid">
      {#each items as f (f.id)}
        <button class="st checker" title={f.name} onclick={() => void addCutout({ libraryId: f.libraryId, path: f.path })}>
          <img
            src="/d/{f.key}/t320.webp"
            alt={f.name}
            loading="lazy"
            draggable="false"
            onerror={(e) => {
              // No thumbnail (yet): the PNG itself is small enough.
              const img = e.currentTarget as HTMLImageElement;
              if (!img.src.includes('/media/')) img.src = mediaUrl(f);
            }}
          />
          <span class="plus"><Plus size={12} /></span>
        </button>
      {/each}
    </div>
  {:else if loaded}
    <p class="hint">Cut the subject out of any photo (Photos tab, the photo's Inspector, or the Library) — it shows up here as a sticker.</p>
  {/if}
</div>

<style>
  .mine {
    padding: 0 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .title {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    max-height: 180px;
    overflow: auto;
  }
  .st {
    position: relative;
    aspect-ratio: 1;
    padding: 4px;
    border-radius: 9px;
    border: 1px solid transparent;
    cursor: pointer;
  }
  .st:hover {
    border-color: var(--accent);
  }
  .st img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }
  .plus {
    position: absolute;
    right: 3px;
    bottom: 3px;
    display: none;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
  }
  .st:hover .plus {
    display: grid;
  }
</style>
