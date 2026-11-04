<script lang="ts">
  import { ArrowLeftRight, LoaderCircle } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { api, onProgress } from '../api/backend';
  import { defaultMetadataPolicy, type ConvertOptions, type ImageFormat, type MetadataPolicy, type OutputSpec } from '../api/types';
  import FileQueue from '../components/tools/FileQueue.svelte';
  import MetadataOptions from '../components/tools/MetadataOptions.svelte';
  import OutputOptions from '../components/tools/OutputOptions.svelte';
  import Results from '../components/tools/Results.svelte';
  import ColorInput from '../components/ui/ColorInput.svelte';
  import Section from '../components/ui/Section.svelte';
  import Segmented from '../components/ui/Segmented.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import { app } from '../stores/app.svelte';
  import { convertQueue as queue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { parseHex } from '../render/fills';

  const encodable = $derived(app.info?.formats.filter((f) => f.canEncode) ?? []);

  let target = $state<ImageFormat | 'keep'>('jpeg');
  let qualityMode = $state<'original' | 'reduce'>('original');
  let reduce = $state(20);
  let resizeMode = $state<'none' | 'percent' | 'longEdge'>('none');
  let percent = $state(50);
  let longEdge = $state(2048);
  let metadata = $state<MetadataPolicy>(defaultMetadataPolicy());
  let output = $state<OutputSpec>({ directory: null, suffix: '', conflict: 'rename' });
  let bg = $state('#ffffff');

  const cap = $derived(target === 'keep' ? null : encodable.find((f) => f.format === target));

  async function run() {
    if (!queue.files.length || queue.running) return;
    const rgb = parseHex(bg) ?? [255, 255, 255];
    const options: ConvertOptions = {
      format: target === 'keep' ? { kind: 'keep' } : { kind: 'to', format: target },
      quality: qualityMode === 'original' ? { kind: 'original' } : { kind: 'reduce', percent: reduce },
      resize: resizeMode === 'none' ? { kind: 'none' } : resizeMode === 'percent' ? { kind: 'percent', percent } : { kind: 'longEdge', pixels: longEdge },
      metadata: $state.snapshot(metadata) as MetadataPolicy,
      output: $state.snapshot(output) as OutputSpec,
      background: rgb,
    };
    queue.running = true;
    queue.results = [];
    queue.progress = { done: 0, total: queue.files.length, current: null };
    try {
      queue.results = await api.convertImages(
        queue.files.map((f) => f.path),
        options,
      );
      const failed = queue.results.filter((r) => r.status === 'failed').length;
      toasts.push({
        kind: failed ? 'warning' : 'success',
        title: failed ? `Converted with ${failed} error${failed > 1 ? 's' : ''}` : 'Conversion finished',
        message: `${queue.results.filter((r) => r.status === 'done').length} of ${queue.results.length} images converted.`,
      });
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
      <p class="hint">Change formats in bulk. Keep every bit of quality and metadata — or slim things down.</p>
    </header>
    <FileQueue {queue} />
    <Results results={queue.results} />
  </div>

  <aside class="options card">
    <Section title="Convert to">
      <select class="select" bind:value={target}>
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
        <ColorInput label="Fill transparent areas with" value={bg} onchange={(c) => (bg = c)} />
      {/if}
    </Section>

    <Section title="Quality">
      <Segmented
        options={[
          { value: 'original', label: 'Keep 100%' },
          { value: 'reduce', label: 'Reduce' },
        ]}
        value={qualityMode}
        onchange={(v) => (qualityMode = v)}
      />
      {#if qualityMode === 'original'}
        <p class="hint">Lossless wherever the format allows. JPEG → JPEG copies the image data untouched.</p>
      {:else}
        <Slider label="Lower quality by" value={reduce} min={1} max={90} unit="%" onchange={(v) => (reduce = Math.round(v))} />
        <p class="hint">Encoder quality {100 - reduce}. Smaller files, slight loss of detail.</p>
      {/if}
    </Section>

    <Section title="Resize" open={false}>
      <Segmented
        options={[
          { value: 'none', label: 'Original' },
          { value: 'percent', label: 'Percent' },
          { value: 'longEdge', label: 'Long edge' },
        ]}
        value={resizeMode}
        onchange={(v) => (resizeMode = v)}
      />
      {#if resizeMode === 'percent'}
        <Slider label="Scale" value={percent} min={5} max={100} unit="%" onchange={(v) => (percent = Math.round(v))} />
      {:else if resizeMode === 'longEdge'}
        <div class="row">
          <span class="label">Longest side</span>
          <span class="spacer"></span>
          <input class="input" type="number" min="16" max="20000" bind:value={longEdge} style="width:100px" /> px
        </div>
      {/if}
    </Section>

    <Section title="Metadata">
      <MetadataOptions value={metadata} onchange={(v) => (metadata = v)} />
    </Section>

    <Section title="Save to">
      <OutputOptions value={output} onchange={(v) => (output = v)} />
    </Section>

    <div class="go">
      {#if queue.progress}
        <div class="prog">
          <div class="bar"><span style:width="{(queue.progress.done / Math.max(1, queue.progress.total)) * 100}%"></span></div>
          <span class="hint truncate">{queue.progress.done}/{queue.progress.total} {queue.progress.current?.split('/').pop() ?? ''}</span>
        </div>
        <button class="btn block" onclick={() => void api.cancelBatch()}>Cancel</button>
      {:else}
        <button class="btn primary large block" disabled={!queue.files.length} onclick={run}>
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
