<script lang="ts">
  import { FolderOpen, RefreshCw } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, pickFolder } from '../api/backend';
  import type { ExportFormat, ThemePref } from '../api/types';
  import Segmented from '../components/ui/Segmented.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import { buildFilename } from '../editor/exporter';
  import { DPI_CHOICES, PAGE_PRESETS } from '../model/pages';
  import { app } from '../stores/app.svelte';
  import { formatBytes } from '../stores/batch.svelte';
  import { library } from '../stores/library.svelte';
  import { toasts } from '../stores/toasts.svelte';

  const s = $derived(app.settings);
  let cache = $state<number | null>(null);
  let checking = $state(false);

  // Save slider changes once the user pauses, not on every tick.
  let qualityTimer: ReturnType<typeof setTimeout> | null = null;
  function setQuality(v: number) {
    app.settings.exportQuality = Math.round(v);
    if (qualityTimer) clearTimeout(qualityTimer);
    qualityTimer = setTimeout(() => void app.saveSettings({ exportQuality: app.settings.exportQuality }), 400);
  }

  async function chooseDir() {
    const dir = await pickFolder('Choose where Save puts your collages', s.saveDirectory);
    if (dir) await app.saveSettings({ saveDirectory: dir });
  }

  async function check() {
    checking = true;
    await app.checkUpdates(true);
    checking = false;
  }

  let clearing = $state(false);

  onMount(async () => {
    try {
      cache = await api.cacheSize();
    } catch {
      cache = null;
    }
  });
</script>

<div class="settings">
  <h2>Settings</h2>

  <section class="card">
    <h3>Saving</h3>
    <div class="field">
      <div class="lbl">
        <span>Save folder</span>
        <span class="hint">The <b>Save</b> button (Ctrl+S) puts a copy of your collage here — no questions asked.</span>
      </div>
      <div class="row dir">
        <span class="path truncate" title={s.saveDirectory}>{s.saveDirectory || 'Not set'}</span>
        <button class="btn small" onclick={chooseDir}>Change…</button>
        {#if s.saveDirectory}<button class="icon-btn small" title="Open folder" onclick={() => void api.openPath(s.saveDirectory)}><FolderOpen size={14} /></button>{/if}
      </div>
    </div>
    <div class="field">
      <div class="lbl">
        <span>File name</span>
        <span class="hint">Use {'{name}'}, {'{date}'} and {'{time}'}. Example: <b>{buildFilename(s.filenamePattern, 'My collage', s.exportFormat)}</b></span>
      </div>
      <input class="input" value={s.filenamePattern} onchange={(e) => void app.saveSettings({ filenamePattern: (e.target as HTMLInputElement).value || '{name}' })} />
    </div>
    <div class="field">
      <div class="lbl"><span>Format for Save</span></div>
      <Segmented
        full={false}
        options={[
          { value: 'jpeg', label: 'JPEG' },
          { value: 'png', label: 'PNG' },
          { value: 'webp', label: 'WebP' },
          { value: 'avif', label: 'AVIF' },
          { value: 'tiff', label: 'TIFF' },
          { value: 'pdf', label: 'PDF' },
        ]}
        value={s.exportFormat}
        onchange={(v) => void app.saveSettings({ exportFormat: v as ExportFormat })}
      />
    </div>
    <div class="field">
      <div class="lbl"><span>Quality</span><span class="hint">For JPEG, WebP, AVIF and PDF.</span></div>
      <div class="slider"><Slider value={s.exportQuality} min={40} max={100} label="Quality" unit="%" onchange={setQuality} /></div>
    </div>
  </section>

  <section class="card">
    <h3>New collages</h3>
    <div class="field">
      <div class="lbl"><span>Default page</span></div>
      <div class="row">
        <select class="select" value={s.defaultPreset} onchange={(e) => void app.saveSettings({ defaultPreset: (e.target as HTMLSelectElement).value })}>
          {#each PAGE_PRESETS as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
        </select>
        <select class="select" value={s.defaultOrientation} onchange={(e) => void app.saveSettings({ defaultOrientation: (e.target as HTMLSelectElement).value as 'portrait' | 'landscape' })}>
          <option value="landscape">Landscape</option>
          <option value="portrait">Portrait</option>
        </select>
        <select class="select" value={s.defaultDpi} onchange={(e) => void app.saveSettings({ defaultDpi: parseInt((e.target as HTMLSelectElement).value) })}>
          {#each DPI_CHOICES as d (d)}<option value={d}>{d} DPI</option>{/each}
        </select>
      </div>
    </div>
  </section>

  <section class="card">
    <h3>Appearance</h3>
    <div class="field">
      <div class="lbl"><span>Theme</span></div>
      <Segmented
        full={false}
        options={[
          { value: 'system', label: 'System' },
          { value: 'dark', label: 'Dark' },
          { value: 'light', label: 'Light' },
        ]}
        value={s.theme}
        onchange={(v) => void app.saveSettings({ theme: v as ThemePref })}
      />
    </div>
  </section>

  <section class="card">
    <h3>Updates</h3>
    {#if app.isFlatpak}
      <p class="hint">You’re running the Flatpak — updates arrive through your software centre or <code>flatpak update</code>.</p>
    {:else}
      <Toggle checked={s.checkUpdates} label="Check for new versions automatically" hint="Once a day, asks GitHub for the latest release. Nothing else is sent." onchange={(v) => void app.saveSettings({ checkUpdates: v })} />
    {/if}
    <div class="row">
      <span>Tessera {app.info?.version ?? __APP_VERSION__} <span class="pill accent">{app.info?.channel ?? 'alpha'}</span></span>
      {#if app.update?.hasUpdate}<span class="pill success">{app.update.latest} available</span>{/if}
      <span class="spacer"></span>
      {#if app.update?.hasUpdate && app.update.url}
        <button class="btn small primary" onclick={() => void api.openUrl(app.update!.url!)}>Download</button>
      {/if}
      <button class="btn small" onclick={check} disabled={checking}><RefreshCw size={14} class={checking ? 'spin' : ''} /> Check now</button>
    </div>
  </section>

  <section class="card">
    <h3>Storage</h3>
    <div class="row">
      <span class="hint">Preview cache: {cache === null ? '…' : formatBytes(cache)}. Previews are rebuilt automatically when needed.</span>
      <span class="spacer"></span>
      <button
        class="btn small"
        disabled={clearing}
        onclick={async () => {
          clearing = true;
          try {
            const freed = await api.clearCache();
            toasts.push({ kind: 'success', title: `Freed ${formatBytes(freed)}` });
            // Photos already in use get their previews back straight away.
            await library.rebuild();
            cache = await api.cacheSize();
          } catch (e) {
            toasts.push({ kind: 'error', title: 'Could not clear the cache', message: String(e) });
          } finally {
            clearing = false;
          }
        }}>{clearing ? 'Clearing…' : 'Clear cache'}</button
      >
    </div>
  </section>

  <section class="card about">
    <h3>About</h3>
    <p>
      <b>Tessera</b> — a tessera is a single tile of a mosaic. Free and open-source software under the GNU GPL v3 or later.
    </p>
    <p class="hint">
      Inspired by PhotoCollage by Adrien Vergé, whose column layout lives on as the “Classic” engine. Fonts are licensed under the SIL Open Font
      License or Apache 2.0; stickers, patterns and templates are dedicated to the public domain (CC0), so everything you make is yours.
    </p>
    <div class="row">
      <button class="btn small" onclick={() => void api.openUrl('https://github.com/MrNechev/tessera')}>Website & source</button>
      <button class="btn small" onclick={() => void api.openUrl('https://github.com/MrNechev/tessera/issues/new/choose')}>Report a problem</button>
    </div>
  </section>
</div>

<style>
  .settings {
    height: 100%;
    overflow-y: auto;
    padding: 32px 44px 60px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 900px;
  }
  h2 {
    font-size: 24px;
    margin-bottom: 6px;
  }
  section {
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  h3 {
    font-size: 15px;
  }
  .field {
    display: flex;
    align-items: center;
    gap: 20px;
  }
  .lbl {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .field .input {
    width: 300px;
  }
  .dir {
    max-width: 420px;
  }
  .path {
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg-2);
    border: 1px solid var(--border);
    color: var(--text-2);
    min-width: 0;
    flex: 1;
  }
  .slider {
    width: 300px;
  }
  .about p {
    margin: 0;
    line-height: 1.55;
  }
</style>
