<script lang="ts">
  import { onMount } from 'svelte';
  import { api, fmtBytes, fmtDate, fmtDuration, srcset } from '../lib/api';
  import { prefs } from '../lib/prefs.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import type { FileItem, Usage } from '../lib/types';
  import Icon from './Icon.svelte';

  let { file, onclose, oninsert }: { file: FileItem; onclose: () => void; oninsert?: () => void } = $props();
  const r = $derived(resolver.get({ libraryId: file.libraryId, path: file.path }));
  const src = $derived(r?.present?.url || r?.doc?.url || '');
  let uses = $state<Usage[] | null>(null);
  let live = $state(false);

  function openUse(u: Usage) {
    onclose();
    router.go(`/doc/${u.docId}${u.sectionId ? `?s=${u.sectionId}` : ''}`);
  }

  onMount(() => {
    api.usage(file.libraryId, file.path).then((u) => (uses = u)).catch(() => (uses = []));
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="lb" onclick={onclose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="stage" onclick={(e) => e.stopPropagation()}>
    {#if file.kind === 'image' && live && file.motion}
      <video src="/d/{file.key}/motion.mp4" autoplay loop muted playsinline></video>
    {:else if file.kind === 'image'}
      <img srcset={srcset(`/d/${file.key}/`, file.width)} sizes="90vw" alt={file.name} />
    {:else if file.kind === 'audio'}
      <!-- svelte-ignore a11y_media_has_caption -->
      <audio src={src || `/media/${file.libraryId}/${file.path.split('/').map(encodeURIComponent).join('/')}`} controls autoplay></audio>
    {:else if file.kind === 'gif'}
      <video src="/d/{file.key}/gif.mp4" autoplay loop muted playsinline></video>
    {:else if src}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video {src} controls autoplay playsinline poster="/d/{file.key}/t1280.webp"></video>
    {:else}
      <div class="nope">
        <img src="/d/{file.key}/t1280.webp" alt="" />
        <p>This {file.vcodec?.toUpperCase()} video needs a browser-compatible copy. Insert it into a document and it is converted automatically.</p>
      </div>
    {/if}
  </div>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="info" onclick={(e) => e.stopPropagation()}>
    <div class="name">{file.path}</div>
    <div class="muted">
      {fmtBytes(file.size)}{file.width ? ` · ${file.width}×${file.height}` : ''}{file.duration ? ` · ${fmtDuration(file.duration)}` : ''}{file.vcodec ? ` · ${file.vcodec}` : ''}{file.acodec ? `/${file.acodec}` : ''}{file.hdr ? ' · HDR' : ''}
    </div>
    <div class="muted">
      {#if file.taken}<span title="Date taken"><Icon name="calendar" size={12} /> {fmtDate(file.taken)}</span>{/if}
      {#if file.place}<span title="{file.lat?.toFixed(5)}, {file.lon?.toFixed(5)}"> · <Icon name="pin" size={12} /> {file.place}</span>{/if}
      {#if file.camera}<span> · {file.camera}</span>{/if}
    </div>
    <div class="uses">
      {#if uses === null}<span class="muted">…</span>
      {:else if !uses.length}<span class="muted">Not used in any document</span>
      {:else}
        <span class="muted">Used in</span>
        {#each uses.slice(0, 6) as u, i (i)}
          <button class="chip" onclick={() => openUse(u)} title="Open">{u.docTitle || 'Untitled'}{u.where === 'section' ? ` · ${u.title || 'section ' + (u.section + 1)}` : ` · ${u.where}`}</button>
        {/each}
        {#if uses.length > 6}<span class="muted">+{uses.length - 6}</span>{/if}
      {/if}
    </div>
    <div class="grow"></div>
    {#if file.motion}<button class="btn" class:primary={live} onclick={() => (live = !live)}><Icon name="play" /> Live</button>{/if}
    <button class="btn" title={prefs.isFavorite(file) ? 'Remove from favourites' : 'Add to favourites'} onclick={() => prefs.toggleFavorite({ libraryId: file.libraryId, path: file.path })}>
      <Icon name="star" /> {prefs.isFavorite(file) ? 'Starred' : 'Star'}
    </button>
    {#if oninsert}<button class="btn primary" onclick={oninsert}><Icon name="plus" /> Insert</button>{/if}
    <a class="btn" href="/media/{file.libraryId}/{file.path.split('/').map(encodeURIComponent).join('/')}?download=1"><Icon name="download" /> Original</a>
    <button class="btn" onclick={onclose}><Icon name="x" /> Close</button>
  </div>
</div>

<style>
  .lb {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: #000e;
    display: flex;
    flex-direction: column;
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .stage img,
  .stage video {
    max-width: 100%;
    max-height: calc(100vh - 120px);
    object-fit: contain;
    border-radius: 6px;
  }
  .nope {
    text-align: center;
    color: #ddd;
    max-width: 640px;
  }
  .nope img {
    max-height: 50vh;
    opacity: 0.6;
  }
  .info {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
    padding: 12px 20px;
    background: #0008;
    color: #eee;
  }
  .uses {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .uses .chip {
    border: none;
    cursor: pointer;
  }
  .name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 50%;
  }
  .grow {
    flex: 1;
  }
  a.btn {
    text-decoration: none;
  }
</style>
