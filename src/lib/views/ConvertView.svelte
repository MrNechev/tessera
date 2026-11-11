<script lang="ts">
  import { ArrowLeftRight, LoaderCircle } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, onProgress } from '../api/backend';
  import type { ConvertOptions, MetadataPolicy, OutputSpec } from '../api/types';
  import FileQueue from '../components/tools/FileQueue.svelte';
  import MetadataCard from '../components/tools/MetadataCard.svelte';
  import MetadataOptions from '../components/tools/MetadataOptions.svelte';
  import OutputOptions from '../components/tools/OutputOptions.svelte';
  import Results from '../components/tools/Results.svelte';
  import ColorInput from '../components/ui/ColorInput.svelte';
  import Section from '../components/ui/Section.svelte';
  import Segmented from '../components/ui/Segmented.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import { app } from '../stores/app.svelte';
  import { busyElsewhere, convertOptions as o, convertQueue as queue, isNum, reportResults, toInt } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { parseHex } from '../render/fills';

  const encodable = $derived(app.info?.formats.filter((f) => f.canEncode) ?? []);

  let inspected = $state<string | null>(null);

  const cap = $derived(o.target === 'keep' ? null : encodable.find((f) => f.format === o.target));

  const other = $derived(busyElsewhere(queue));
  const invalid = $derived(o.resizeMode === 'longEdge' && !isNum(o.longEdge) ? 'Enter a size for the longest side.' : null);
  const blocked = $derived(other ? `Another job is running in ${other.label}.` : invalid);

  async function run() {
    if (!queue.files.length || queue.running || blocked) return;
    const rgb = parseHex(o.bg) ?? [255, 255, 255];
    const options: ConvertOptions = {
      format: o.target === 'keep' ? { kind: 'keep' } : { kind: 'to', format: o.target },
      quality: o.qualityMode === 'original' ? { kind: 'original' } : { kind: 'reduce', percent: o.reduce },
      resize:
        o.resizeMode === 'none'
          ? { kind: 'none' }
          : o.resizeMode === 'percent'
            ? { kind: 'percent', percent: o.percent }
            : { kind: 'longEdge', pixels: toInt(o.longEdge, 16, 20000, 2048) },
      metadata: $state.snapshot(o.metadata) as MetadataPolicy,
      output: $state.snapshot(o.output) as OutputSpec,
      background: rgb,
      color: o.color,
    };
    queue.running = true;
    queue.results = [];
    queue.progress = { done: 0, total: queue.files.length, current: null };
    try {
      queue.results = await api.convertImages(
        queue.files.map((f) => f.path),
        options,
      );
      reportResults(queue.results, { verb: 'converted', title: 'Conversion finished', failTitle: 'Conversion failed' });
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Conversion failed', message: String(e) });
    } finally {
      queue.running = false;
      queue.progress = null;
    }
  }

  onMount(() => {
    const un = onProgress((p) => {
      if (p.task === 'convert' && queue.running) queue.progress = { done: p.done, total: p.total, current: p.current ?? null };
    });
    return () => void un.then((f) => f());
  });
</script>

<div class="tool">
  <div class="main">
    <header class="title">
      <h2>Convert images</h2>
      <p class="hint">Change formats in bulk. Keep every bit of quality and metadata — or slim things down. Click a file to see its metadata.</p>
    </header>
    <FileQueue {queue} selected={inspected} onselect={(p) => (inspected = p)} />
    {#if inspected && queue.files.some((f) => f.path === inspected)}
      <MetadataCard path={inspected} onclose={() => (inspected = null)} />
    {/if}
    <Results results={queue.results} />
  </div>

  <aside class="options card">
    <Section title="Convert to">
      <select class="select" bind:value={o.target}>
        <option value="keep">Keep original format</option>
        {#each encodable as f (f.format)}<option value={f.format}>{f.label} (.{f.extension})</option>{/each}
      </select>
      {#if cap}
        <p class="hint">
          {cap.lossless && cap.lossy ? 'Supports lossy and lossless.' : cap.lossless ? 'Lossless format.' : 'Lossy format.'}
          {cap.exif ? '' : ' Can’t store EXIF metadata.'}{cap.alpha ? '' : ' No transparency.'}
        </p>
      {/if}
      {#if cap && !cap.alpha}
        <ColorInput label="Fill transparent areas with" value={o.bg} onchange={(c) => (o.bg = c)} />
      {/if}
    </Section>

    <Section title="Quality">
      <Segmented
        options={[
          { value: 'original', label: 'Keep 100%' },
          { value: 'reduce', label: 'Reduce' },
        ]}
        value={o.qualityMode}
        onchange={(v) => (o.qualityMode = v)}
      />
      {#if o.qualityMode === 'original'}
        <p class="hint">Lossless wherever the format allows. JPEG → JPEG copies the image data untouched.</p>
      {:else}
        <Slider label="Lower quality by" value={o.reduce} min={1} max={90} unit="%" onchange={(v) => (o.reduce = Math.round(v))} />
        <p class="hint">Encoder quality {100 - o.reduce}. Smaller files, slight loss of detail.</p>
      {/if}
    </Section>

    <Section title="Resize" open={false}>
      <Segmented
        options={[
          { value: 'none', label: 'Original' },
          { value: 'percent', label: 'Percent' },
          { value: 'longEdge', label: 'Long edge' },
        ]}
        value={o.resizeMode}
        onchange={(v) => (o.resizeMode = v)}
      />
      {#if o.resizeMode === 'percent'}
        <Slider label="Scale" value={o.percent} min={5} max={100} unit="%" onchange={(v) => (o.percent = Math.round(v))} />
      {:else if o.resizeMode === 'longEdge'}
        <div class="row">
          <span class="label">Longest side</span>
          <span class="spacer"></span>
          <input class="input" type="number" min="16" max="20000" bind:value={o.longEdge} style="width:100px" /> px
        </div>
      {/if}
    </Section>

    <Section title="Colour">
      <Segmented
        options={[
          { value: 'srgb', label: 'Convert to sRGB' },
          { value: 'keep', label: 'Keep profile' },
        ]}
        value={o.color}
        onchange={(v) => (o.color = v)}
      />
      <p class="hint">
        {o.color === 'srgb'
          ? 'Wide-gamut photos (Display P3 from phones, Adobe RGB from cameras) are converted so they look right everywhere. RAW files are always developed to sRGB.'
          : 'Pixels and the embedded colour profile are left exactly as they are.'}
      </p>
    </Section>

    <Section title="Metadata">
      <MetadataOptions value={o.metadata} onchange={(v) => (o.metadata = v)} />
    </Section>

    <Section title="Save to">
      <OutputOptions value={o.output} onchange={(v) => (o.output = v)} />
    </Section>

    <div class="go">
      {#if queue.progress}
        <div class="prog">
          <div class="bar"><span style:width="{(queue.progress.done / Math.max(1, queue.progress.total)) * 100}%"></span></div>
          <span class="hint truncate">{queue.progress.done}/{queue.progress.total} {queue.progress.current?.split('/').pop() ?? ''}</span>
        </div>
        <button class="btn block" onclick={() => void api.cancelBatch()}>Cancel</button>
      {:else}
        {#if blocked && queue.files.length}<p class="hint">{blocked}</p>{/if}
        <button class="btn primary large block" disabled={!queue.files.length || queue.running || !!blocked} onclick={run}>
          {#if queue.running}<LoaderCircle size={16} class="spin" />{:else}<ArrowLeftRight size={16} />{/if}
          Convert {queue.files.length || ''} image{queue.files.length === 1 ? '' : 's'}
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
  .options {
    width: 340px;
    flex: none;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .options .select {
    width: 100%;
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
  .go p {
    margin: 0;
  }
  .prog {
    display: flex;
    flex-direction: column;
    gap: 6px;
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
    transition: width 0.2s;
  }
</style>
