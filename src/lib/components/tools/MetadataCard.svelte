<script lang="ts">
  import { Camera, Clock, Info, LoaderCircle, MapPin, Palette, Ruler, X } from '@lucide/svelte';
  import { api } from '../../api/backend';
  import type { MetadataSummary } from '../../api/types';
  import { formatBytes } from '../../stores/batch.svelte';

  interface Props {
    path: string;
    onclose: () => void;
  }
  let { path, onclose }: Props = $props();

  let info = $state<MetadataSummary | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);
  let showAll = $state(false);

  $effect(() => {
    const p = path;
    loading = true;
    error = null;
    info = null;
    api
      .readMetadata(p)
      .then((m) => {
        if (p === path) info = m;
      })
      .catch((e) => {
        if (p === path) error = String(e);
      })
      .finally(() => (loading = false));
  });

  // Many cameras repeat the maker in the model name ("Canon" + "Canon EOS 600D").
  const camera = $derived.by(() => {
    if (!info) return '';
    const make = info.cameraMake?.trim() ?? '';
    const model = info.cameraModel?.trim() ?? '';
    if (make && model.toLowerCase().startsWith(make.toLowerCase())) return model;
    return [make, model].filter(Boolean).join(' ');
  });
  const exposure = $derived(
    info
      ? [
          info.exposureTime ? (/s$/.test(info.exposureTime.trim()) ? info.exposureTime : `${info.exposureTime} s`) : null,
          info.fNumber ? `ƒ/${info.fNumber}` : null,
          info.iso ? `ISO ${info.iso}` : null,
          info.focalLength ? `${info.focalLength} mm` : null,
        ]
          .filter(Boolean)
          .join(' · ')
      : '',
  );
</script>

<div class="meta card">
  <header>
    <Info size={15} />
    <span class="truncate title">{path.split('/').pop()}</span>
    <span class="spacer"></span>
    {#if info?.fields.length}
      <button class="btn small ghost" onclick={() => (showAll = !showAll)}>{showAll ? 'Summary' : `All ${info.fields.length} fields`}</button>
    {/if}
    <button class="icon-btn small" onclick={onclose} aria-label="Close"><X size={14} /></button>
  </header>
  {#if loading}
    <div class="row hint"><LoaderCircle size={14} class="spin" /> Reading metadata…</div>
  {:else if error}
    <p class="hint">{error}</p>
  {:else if info}
    {#if showAll}
      <div class="all">
        {#each info.fields as f, i (i)}
          <span class="k" title={f.ifd}>{f.tag}</span><span class="v truncate" title={f.value}>{f.value}</span>
        {/each}
      </div>
    {:else}
      <div class="grid">
        <div><Ruler size={14} /><span>{info.width}×{info.height} · {info.format?.toUpperCase() ?? '?'} · {formatBytes(info.fileSize)}</span></div>
        <div><Camera size={14} /><span>{camera || 'No camera info'}{info.lens ? ` · ${info.lens}` : ''}</span></div>
        <div><Clock size={14} /><span>{info.dateTaken?.replace('T', ' ') ?? 'No capture date'}</span></div>
        {#if exposure}<div><Camera size={14} /><span>{exposure}</span></div>{/if}
        <div>
          <MapPin size={14} />
          <span>
            {#if info.gpsLatitude !== null && info.gpsLongitude !== null}
              {info.gpsLatitude.toFixed(5)}, {info.gpsLongitude.toFixed(5)} <span class="pill warning">location</span>
            {:else}No location{/if}
          </span>
        </div>
        <div><Palette size={14} /><span>{info.hasIcc ? (info.iccDescription ?? 'Colour profile') : 'No colour profile'}{info.hasIcc && !info.iccIsSrgb ? ' (wide gamut)' : ''}{info.hasXmp ? ' · XMP' : ''}</span></div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .meta {
    padding: 10px 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 260px;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .title {
    font-weight: 600;
    color: var(--text);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 16px;
  }
  .grid div {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    color: var(--text-2);
  }
  .grid span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .all {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 14px;
    overflow-y: auto;
    font-size: 12px;
  }
  .k {
    color: var(--text-3);
  }
  .v {
    color: var(--text);
  }
</style>
