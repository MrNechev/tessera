<script lang="ts">
  import { LoaderCircle, Minimize2, Scan } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, fileUrl, onProgress } from '../api/backend';
  import {
    defaultMetadataPolicy,
    type ColorMode,
  type CompressLevel,
    type CompressOptions,
    type CompressPreviewInfo,
    type ImageFormat,
    type MetadataPolicy,
    type OutputSpec,
  } from '../api/types';
  import FileQueue from '../components/tools/FileQueue.svelte';
  import MetadataOptions from '../components/tools/MetadataOptions.svelte';
  import OutputOptions from '../components/tools/OutputOptions.svelte';
  import Results from '../components/tools/Results.svelte';
  import Section from '../components/ui/Section.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import { compressQueue as queue, formatBytes } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';

  type LevelId = CompressLevel['kind'];
  const LEVELS: { id: LevelId; name: string; desc: string }[] = [
    { id: 'visuallyLossless', name: 'Smart — visually lossless', desc: 'Smallest file that still looks identical to the original.' },
    { id: 'balanced', name: 'Balanced', desc: 'Great quality, noticeably smaller files.' },
    { id: 'small', name: 'Small', desc: 'Aggressive savings for web and messaging.' },
    { id: 'lossless', name: 'Lossless only', desc: 'Pixel-perfect: only optimises how the data is stored.' },
    { id: 'custom', name: 'Custom quality', desc: 'Pick the encoder quality yourself.' },
    { id: 'targetSize', name: 'Target file size', desc: 'Fit under a size limit (e.g. for email or forms).' },
  ];

  let level = $state<LevelId>('visuallyLossless');
  let quality = $state(80);
  let targetKb = $state(500);
  let format = $state<ImageFormat | 'keep'>('keep');
  let longEdge = $state(0);
  let metadata = $state<MetadataPolicy>(defaultMetadataPolicy());
  let output = $state<OutputSpec>({ directory: null, suffix: '-compressed', conflict: 'rename' });
  let keepIfLarger = $state(true);
  let color = $state<ColorMode>('keep');

  let selected = $state<string | null>(null);
  let preview = $state<CompressPreviewInfo | null>(null);
  let previewing = $state(false);
  let split = $state(50);
  let actual = $state(false);

  function options(): CompressOptions {
    const lv: CompressLevel =
      level === 'custom' ? { kind: 'custom', quality } : level === 'targetSize' ? { kind: 'targetSize', bytes: Math.round(targetKb * 1024) } : ({ kind: level } as CompressLevel);
    return {
      level: lv,
      format: format === 'keep' ? { kind: 'keep' } : { kind: 'to', format },
      resize: longEdge > 0 ? { kind: 'longEdge', pixels: longEdge } : { kind: 'none' },
      metadata: $state.snapshot(metadata) as MetadataPolicy,
      output: $state.snapshot(output) as OutputSpec,
      keepIfLarger,
      color,
    };
  }

  async function runPreview(path: string) {
    selected = path;
    previewing = true;
    try {
      preview = await api.compressPreview(path, options());
    } catch (e) {
      preview = null;
      toasts.push({ kind: 'error', title: 'Preview failed', message: String(e) });
    } finally {
      previewing = false;
    }
  }

  async function run() {
    if (!queue.files.length || queue.running) return;
    queue.running = true;
    queue.results = [];
    queue.progress = { done: 0, total: queue.files.length, current: null };
    try {
      queue.results = await api.compressImages(
        queue.files.map((f) => f.path),
        options(),
      );
      const ok = queue.results.filter((r) => r.status === 'done');
      const before = ok.reduce((s, r) => s + r.inputBytes, 0);
      const after = ok.reduce((s, r) => s + r.outputBytes, 0);
      toasts.push({
        kind: 'success',
        title: 'Compression finished',
        message: `${ok.length} image${ok.length === 1 ? '' : 's'} · saved ${formatBytes(Math.max(0, before - after))}`,
      });
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Compression failed', message: String(e) });
    } finally {
      queue.running = false;
      queue.progress = null;
    }
  }

  onMount(() => {
    const un = onProgress((p) => {
      if (p.task === 'compress' && queue.running) queue.progress = { done: p.done, total: p.total, current: p.current ?? null };
    });
    return () => void un.then((f) => f());
  });

  const savings = $derived(preview ? 1 - preview.bytes / preview.originalBytes : 0);
</script>

<div class="tool">
  <div class="main">
    <header class="title">
      <h2>Compress images</h2>
      <p class="hint">Shrink files while keeping them looking just like the original. Click a file to preview the result.</p>
    </header>
    <div class="split">
      <FileQueue {queue} {selected} onselect={(p) => void runPreview(p)} />
      <div class="preview card">
        {#if previewing}
          <div class="center"><LoaderCircle size={22} class="spin" /><span class="hint">Finding the best settings…</span></div>
        {:else if preview}
          <div class="stats">
            <span><b>{formatBytes(preview.originalBytes)}</b> → <b>{formatBytes(preview.bytes)}</b></span>
            <span class="pill" class:success={savings > 0} class:warning={savings <= 0}>{savings > 0 ? `−${(savings * 100).toFixed(1)}%` : 'already optimal'}</span>
            <span class="hint">{preview.format.toUpperCase()}{preview.quality ? ` · q${preview.quality}` : ''}{preview.ssim ? ` · SSIM ${preview.ssim.toFixed(4)}` : ''}</span>
            <span class="spacer"></span>
            <button class="icon-btn small" class:active={actual} title="Toggle 100% zoom" onclick={() => (actual = !actual)}><Scan size={14} /></button>
          </div>
          <div class="compare" class:actual>
            <div class="stack">
              <img src={fileUrl(preview.originalPreviewPath)} alt="Original" />
              <img class="top" src={fileUrl(preview.previewPath)} alt="Compressed" style:clip-path="inset(0 0 0 {split}%)" />
              <div class="handle" style:left="{split}%"></div>
            </div>
          </div>
          <div class="row labels"><span class="hint">Original</span><input type="range" min="0" max="100" bind:value={split} style:--fill="{split}%" /><span class="hint">Compressed</span></div>
        {:else}
          <div class="center hint">Select a file to see a before / after comparison.</div>
        {/if}
      </div>
    </div>
    <Results results={queue.results} />
  </div>

  <aside class="options card">
    <Section title="Compression">
      <div class="levels">
        {#each LEVELS as l (l.id)}
          <label class="level" class:on={level === l.id}>
            <input type="radio" name="level" value={l.id} bind:group={level} />
            <span class="lname">{l.name}</span>
            <span class="hint">{l.desc}</span>
          </label>
        {/each}
      </div>
      {#if level === 'custom'}
        <Slider label="Quality" value={quality} min={10} max={100} onchange={(v) => (quality = Math.round(v))} />
      {:else if level === 'targetSize'}
        <div class="row">
          <span class="label">Maximum size</span>
          <span class="spacer"></span>
          <input class="input" type="number" min="10" bind:value={targetKb} style="width:100px" /> KB
        </div>
      {/if}
    </Section>

    <Section title="Format & size">
      <div class="row">
        <span class="label">Output format</span>
        <span class="spacer"></span>
        <select class="select" bind:value={format}>
          <option value="keep">Same as original</option>
          <option value="jpeg">JPEG</option>
          <option value="webp">WebP (smaller)</option>
          <option value="avif">AVIF (smallest)</option>
          <option value="png">PNG</option>
        </select>
      </div>
      <div class="row">
        <span class="label">Longest side</span>
        <span class="spacer"></span>
        <select class="select" bind:value={longEdge}>
          <option value={0}>Original size</option>
          <option value={4096}>4096 px</option>
          <option value={3000}>3000 px</option>
          <option value={2048}>2048 px</option>
          <option value={1600}>1600 px</option>
          <option value={1280}>1280 px</option>
        </select>
      </div>
      <Toggle checked={keepIfLarger} label="Keep the original if it can’t be made smaller" onchange={(v) => (keepIfLarger = v)} />
      <Toggle checked={color === 'srgb'} label="Convert wide-gamut colours to sRGB" hint="Display P3 / Adobe RGB → sRGB. Off keeps the profile." onchange={(v) => (color = v ? 'srgb' : 'keep')} />
    </Section>

    <Section title="Metadata" open={false}>
      <MetadataOptions value={metadata} onchange={(v) => (metadata = v)} />
    </Section>

    <Section title="Save to">
      <OutputOptions value={output} onchange={(v) => (output = v)} />
    </Section>

    <div class="go">
      {#if queue.progress}
        <div class="bar"><span style:width="{(queue.progress.done / Math.max(1, queue.progress.total)) * 100}%"></span></div>
        <span class="hint truncate">{queue.progress.done}/{queue.progress.total} {queue.progress.current?.split('/').pop() ?? ''}</span>
        <button class="btn block" onclick={() => void api.cancelBatch()}>Cancel</button>
      {:else}
        {#if selected}
          <button class="btn block" onclick={() => selected && void runPreview(selected)}>Update preview</button>
        {/if}
        <button class="btn primary large block" disabled={!queue.files.length} onclick={run}>
          <Minimize2 size={16} /> Compress {queue.files.length || ''} image{queue.files.length === 1 ? '' : 's'}
        </button>
      {/if}
    </div>
  </aside>
</div>

<style>
  .tool {
    display: flex;
    gap: 16px;
    height: 100%;
    padding: 20px;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .title h2 {
    font-size: 22px;
  }
  .title p {
    margin: 4px 0 0;
  }
  .split {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr);
    gap: 14px;
  }
  .preview {
    display: flex;
    flex-direction: column;
    min-height: 0;
    overflow: hidden;
    padding: 12px;
    gap: 10px;
  }
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .stats {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .compare {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: grid;
    place-items: center;
    background: var(--bg-2);
    border-radius: 8px;
  }
  .compare.actual {
    overflow: auto;
    place-items: start;
  }
  .stack {
    position: relative;
    max-width: 100%;
    max-height: 100%;
    display: inline-block;
  }
  .compare.actual .stack {
    max-width: none;
    max-height: none;
  }
  .stack img {
    display: block;
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .compare:not(.actual) .stack img {
    max-height: calc(100vh - 360px);
  }
  .compare.actual .stack img {
    max-width: none;
    max-height: none;
  }
  .stack .top {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: #fff;
    box-shadow: 0 0 6px rgba(0, 0, 0, 0.5);
    pointer-events: none;
  }
  .labels input {
    flex: 1;
  }
  .options {
    width: 340px;
    flex: none;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .levels {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .level {
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    cursor: pointer;
  }
  .level.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .level input {
    display: none;
  }
  .lname {
    font-weight: 600;
  }
  .go {
    padding: 16px 14px;
    margin-top: auto;
    position: sticky;
    bottom: 0;
    background: var(--panel);
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .bar {
    height: 6px;
    border-radius: 6px;
    background: var(--border-strong);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent-grad);
  }
</style>
