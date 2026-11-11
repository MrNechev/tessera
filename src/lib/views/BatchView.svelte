<script lang="ts">
  import { FolderOpen, ImagePlus, LoaderCircle, Wand } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api, onProgress, pickFolder } from '../api/backend';
  import {
    defaultMetadataPolicy,
    type BatchOptions,
    type ConflictPolicy,
    type ImageFormat,
    type MetadataPolicy,
    type ProbeInfo,
    type ResizeMode,
    type WatermarkOptions,
  } from '../api/types';
  import { FONTS } from '../assets/fonts';
  import FileQueue from '../components/tools/FileQueue.svelte';
  import MetadataOptions from '../components/tools/MetadataOptions.svelte';
  import Results from '../components/tools/Results.svelte';
  import ColorInput from '../components/ui/ColorInput.svelte';
  import Section from '../components/ui/Section.svelte';
  import Segmented from '../components/ui/Segmented.svelte';
  import Slider from '../components/ui/Slider.svelte';
  import Toggle from '../components/ui/Toggle.svelte';
  import { app } from '../stores/app.svelte';
  import { toolsQueue as queue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { DEFAULT_RENAME, RENAME_TOKENS, baseName, buildNames, sortFiles, type RenameSpec } from '../tools/rename';
  import { canvasToPng, placements, renderTextWatermark, resizedSize, watermarkSize, type TextWatermark, type WatermarkPosition } from '../tools/watermark';

  /* ---------------------------------------------------------- options */

  let doResize = $state(true);
  let resizeKind = $state<'longEdge' | 'width' | 'height' | 'percent'>('longEdge');
  let resizeValue = $state(2048);

  let doWatermark = $state(false);
  let wmKind = $state<'text' | 'image'>('text');
  let wmText = $state<TextWatermark>({ text: '© Your Name', font: 'montserrat', weight: 600, color: '#ffffff', outline: false, shadow: true });
  let wmLogo = $state<string | null>(null);
  let wmSize = $state(0.25);
  let wmOpacity = $state(0.75);
  let wmMargin = $state(0.03);
  let wmPosition = $state<WatermarkPosition>('br');

  let doRename = $state(false);
  let rename = $state<RenameSpec>({ ...DEFAULT_RENAME, pattern: '{date} {n}' });
  let suffix = $state('-edited');

  let format = $state<ImageFormat | 'keep'>('keep');
  let quality = $state(90);
  let metadata = $state<MetadataPolicy>(defaultMetadataPolicy());
  let directory = $state<string | null>(null);
  let conflict = $state<ConflictPolicy>('rename');
  let moveOriginals = $state(false);

  const wmFonts = FONTS.filter((f) => ['sans', 'display', 'script', 'serif', 'handwriting'].includes(f.category));

  const resizeMode = $derived<ResizeMode>(
    !doResize
      ? { kind: 'none' }
      : resizeKind === 'longEdge'
        ? { kind: 'longEdge', pixels: Math.max(16, resizeValue) }
        : resizeKind === 'width'
          ? { kind: 'fit', width: Math.max(16, resizeValue), height: 0 }
          : resizeKind === 'height'
            ? { kind: 'fit', width: 0, height: Math.max(16, resizeValue) }
            : { kind: 'percent', percent: Math.min(100, Math.max(1, resizeValue)) },
  );

  /** Nothing changes the pixels or metadata: files can be copied — or renamed in place. */
  const pureRename = $derived(!doResize && !doWatermark && format === 'keep' && metadata.mode === 'keepAll');
  const canMove = $derived(pureRename && doRename && directory === null);

  const ordered = $derived(doRename ? sortFiles(queue.files, rename.sort) : queue.files);
  const names = $derived(
    doRename ? buildNames(ordered, rename) : ordered.map((f) => (directory === null ? `${baseName(f.path)}${suffix}` : baseName(f.path))),
  );

  function setResizeKind(k: typeof resizeKind) {
    resizeKind = k;
    resizeValue = k === 'percent' ? 50 : 2048;
  }

  /* ----------------------------------------------------- watermark mark */

  let mark = $state<HTMLCanvasElement | ImageBitmap | null>(null);
  let markTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    // Re-render the text watermark when its settings change (debounced).
    const t = $state.snapshot(wmText) as TextWatermark;
    const kind = wmKind;
    const logo = wmLogo;
    const on = doWatermark;
    if (markTimer) clearTimeout(markTimer);
    markTimer = setTimeout(async () => {
      if (!on) return (mark = null);
      if (kind === 'text') mark = await renderTextWatermark(t);
      else if (logo) {
        try {
          mark = await createImageBitmap(await api.loadImagePixels(logo, 1600, 1600));
        } catch (e) {
          toasts.push({ kind: 'error', title: 'Could not load the logo', message: String(e) });
          mark = null;
        }
      } else mark = null;
    }, 200);
  });

  async function chooseLogo() {
    const res = await openDialog({
      multiple: false,
      title: 'Choose a logo or signature',
      filters: [{ name: 'Images', extensions: ['png', 'webp', 'jpg', 'jpeg', 'avif', 'tif', 'tiff'] }],
    });
    if (typeof res === 'string') wmLogo = res;
  }

  /* ------------------------------------------------------------ preview */

  let selected = $state<string | null>(null);
  let canvas: HTMLCanvasElement | undefined = $state();
  let photo = $state<{ path: string; bmp: ImageBitmap } | null>(null);
  const current = $derived<ProbeInfo | undefined>(queue.files.find((f) => f.path === selected) ?? queue.files[0]);

  $effect(() => {
    const f = current;
    if (!f || photo?.path === f.path) return;
    void api
      .loadImagePixels(f.path, 1400, 1000)
      .then((d) => createImageBitmap(d))
      .then((bmp) => (photo = { path: f.path, bmp }))
      .catch(() => (photo = null));
  });

  $effect(() => {
    // Draw the photo as it will come out: scaled to the output aspect, with the watermark.
    const c = canvas;
    const p = photo;
    const f = current;
    void mark;
    void wmSize;
    void wmOpacity;
    void wmMargin;
    void wmPosition;
    if (!c || !p || !f) return;
    const box = c.parentElement!.getBoundingClientRect();
    const scale = Math.min(box.width / p.bmp.width, box.height / p.bmp.height, 1);
    const dpr = window.devicePixelRatio || 1;
    const cw = Math.max(1, Math.round(p.bmp.width * scale));
    const ch = Math.max(1, Math.round(p.bmp.height * scale));
    c.width = cw * dpr;
    c.height = ch * dpr;
    c.style.width = `${cw}px`;
    c.style.height = `${ch}px`;
    const ctx = c.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.drawImage(p.bmp, 0, 0, cw, ch);
    if (doWatermark && mark) {
      // Compute in output pixels, then map to the preview.
      const [ow, oh] = resizedSize(f.width, f.height, resizeMode);
      const k = cw / ow;
      const [mw, mh] = watermarkSize(ow, oh, mark.width, mark.height, wmSize);
      ctx.globalAlpha = wmOpacity;
      for (const [x, y] of placements(ow, oh, mw, mh, wmPosition, wmMargin)) ctx.drawImage(mark, x * k, y * k, mw * k, mh * k);
      ctx.globalAlpha = 1;
    }
  });

  /* ---------------------------------------------------------------- run */

  async function run() {
    if (!queue.files.length || queue.running) return;
    if (doWatermark && wmKind === 'image' && !wmLogo) {
      toasts.push({ kind: 'warning', title: 'Choose a logo first', message: 'Or switch the watermark to text.' });
      return;
    }
    queue.running = true;
    queue.results = [];
    queue.progress = { done: 0, total: queue.files.length, current: null };
    try {
      let watermark: WatermarkOptions | null = null;
      if (doWatermark) {
        const image = wmKind === 'image' ? wmLogo! : await api.saveTempImage(await canvasToPng(await renderTextWatermark($state.snapshot(wmText) as TextWatermark)));
        watermark = { image, size: wmSize, opacity: wmOpacity, position: wmPosition, margin: wmMargin };
      }
      const options: BatchOptions = {
        resize: resizeMode,
        watermark,
        format: format === 'keep' ? { kind: 'keep' } : { kind: 'to', format },
        quality,
        metadata: $state.snapshot(metadata) as MetadataPolicy,
        directory,
        conflict,
        transfer: canMove && moveOriginals ? 'move' : 'copy',
        background: [255, 255, 255],
      };
      const jobs = ordered.map((f, i) => ({ input: f.path, name: names[i] }));
      queue.results = await api.batchProcess(jobs, options);
      const failed = queue.results.filter((r) => r.status === 'failed').length;
      toasts.push({
        kind: failed ? 'warning' : 'success',
        title: failed ? `Finished with ${failed} error${failed > 1 ? 's' : ''}` : 'Batch finished',
        message: `${queue.results.filter((r) => r.status === 'done').length} of ${queue.results.length} images processed.`,
      });
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Batch failed', message: String(e) });
    } finally {
      queue.running = false;
      queue.progress = null;
    }
  }

  onMount(() => {
    const un = onProgress((p) => {
      if (p.task === 'batch' && queue.running) queue.progress = { done: p.done, total: p.total, current: p.current ?? null };
    });
    return () => void un.then((f) => f());
  });

  const POSITIONS: WatermarkPosition[] = ['tl', 't', 'tr', 'l', 'c', 'r', 'bl', 'b', 'br'];
</script>

<div class="tool">
  <div class="main">
    <header class="title">
      <h2>Batch tools</h2>
      <p class="hint">Resize, watermark and rename many photos at once. Your originals are never changed unless you explicitly ask.</p>
    </header>
    <div class="split">
      <FileQueue {queue} selected={current?.path ?? null} onselect={(p) => (selected = p)} />
      <div class="preview card">
        {#if current}
          <div class="stage"><canvas bind:this={canvas}></canvas></div>
          <div class="info">
            {#if doResize}
              {@const out = resizedSize(current.width, current.height, resizeMode)}
              <span>{current.width}×{current.height} → <b>{out[0]}×{out[1]}</b></span>
            {:else}
              <span>{current.width}×{current.height}</span>
            {/if}
            <span class="spacer"></span>
            <span class="hint truncate">→ {names[ordered.findIndex((f) => f.path === current.path)] ?? ''}</span>
          </div>
        {:else}
          <div class="empty hint">Add photos to see a preview.</div>
        {/if}
      </div>
    </div>
    {#if doRename && queue.files.length}
      <div class="names card">
        <header><h3>New names</h3><span class="hint">{queue.files.length} files</span></header>
        <div class="list">
          {#each ordered.slice(0, 300) as f, i (f.path)}
            <div class="nm"><span class="old truncate">{f.path.split('/').pop()}</span><span class="arrow">→</span><span class="new truncate">{names[i]}</span></div>
          {/each}
        </div>
      </div>
    {/if}
    <Results results={queue.results} />
  </div>

  <aside class="options card">
    <Section title="Resize">
      <Toggle checked={doResize} label="Resize photos" hint="Photos are never enlarged." onchange={(v) => (doResize = v)} />
      {#if doResize}
        <Segmented
          options={[
            { value: 'longEdge', label: 'Long edge' },
            { value: 'width', label: 'Width' },
            { value: 'height', label: 'Height' },
            { value: 'percent', label: '%' },
          ]}
          value={resizeKind}
          onchange={(v) => setResizeKind(v as typeof resizeKind)}
        />
        <div class="row">
          <input class="input grow" type="number" min="1" bind:value={resizeValue} />
          <span class="hint">{resizeKind === 'percent' ? '%' : 'px'}</span>
        </div>
        {#if resizeKind !== 'percent'}
          <div class="presets">
            {#each [1080, 1600, 2048, 3000, 4096] as p (p)}
              <button class="btn small" class:active={resizeValue === p} onclick={() => (resizeValue = p)}>{p}</button>
            {/each}
          </div>
        {/if}
      {/if}
    </Section>

    <Section title="Watermark">
      <Toggle checked={doWatermark} label="Add a watermark" onchange={(v) => (doWatermark = v)} />
      {#if doWatermark}
        <Segmented options={[{ value: 'text', label: 'Text' }, { value: 'image', label: 'Logo image' }]} value={wmKind} onchange={(v) => (wmKind = v as 'text' | 'image')} />
        {#if wmKind === 'text'}
          <input class="input" bind:value={wmText.text} placeholder="© Your Name" />
          <select class="select" bind:value={wmText.font}>
            {#each wmFonts as f (f.id)}<option value={f.id}>{f.family}</option>{/each}
          </select>
          <ColorInput label="Colour" value={wmText.color} onchange={(c) => (wmText.color = c)} />
          <Toggle checked={wmText.weight >= 600} label="Bold" onchange={(v) => (wmText.weight = v ? 700 : 400)} />
          <Toggle checked={wmText.outline} label="Outline" onchange={(v) => (wmText.outline = v)} />
          <Toggle checked={wmText.shadow} label="Shadow" onchange={(v) => (wmText.shadow = v)} />
        {:else}
          <button class="btn" onclick={chooseLogo}><ImagePlus size={15} /> {wmLogo ? wmLogo.split('/').pop() : 'Choose logo…'}</button>
          <p class="hint">A PNG with a transparent background works best.</p>
        {/if}
        <span class="label">Position</span>
        <div class="pos-grid">
          {#each POSITIONS as p (p)}
            <button class="pos" class:on={wmPosition === p} onclick={() => (wmPosition = p)} aria-label="Position {p}"><span></span></button>
          {/each}
        </div>
        <Toggle checked={wmPosition === 'tile'} label="Repeat across the photo" onchange={(v) => (wmPosition = v ? 'tile' : 'br')} />
        <Slider label="Size" value={wmSize} min={0.03} max={0.8} step={0.01} scale={100} unit="%" onchange={(v) => (wmSize = v)} />
        <Slider label="Opacity" value={wmOpacity} min={0.05} max={1} step={0.01} scale={100} unit="%" onchange={(v) => (wmOpacity = v)} />
        {#if wmPosition !== 'tile'}
          <Slider label="Distance from edge" value={wmMargin} min={0} max={0.15} step={0.005} scale={100} decimals={1} unit="%" onchange={(v) => (wmMargin = v)} />
        {/if}
      {/if}
    </Section>

    <Section title="Rename">
      <Toggle checked={doRename} label="Rename files" onchange={(v) => (doRename = v)} />
      {#if doRename}
        <input class="input" bind:value={rename.pattern} />
        <div class="tokens">
          {#each RENAME_TOKENS as t (t.token)}
            <button class="chip" title={t.help} onclick={() => (rename.pattern += (rename.pattern.endsWith(' ') || !rename.pattern ? '' : ' ') + t.token)}>{t.token}</button>
          {/each}
        </div>
        <div class="grid2">
          <label><span class="label">Start at</span><input class="input" type="number" min="0" bind:value={rename.start} /></label>
          <label><span class="label">Digits</span><input class="input" type="number" min="0" max="8" bind:value={rename.padding} /></label>
        </div>
        <div class="grid2">
          <label>
            <span class="label">Order</span>
            <select class="select" bind:value={rename.sort}>
              <option value="added">As added</option>
              <option value="name">By file name</option>
              <option value="date">By date taken</option>
            </select>
          </label>
          <label>
            <span class="label">Letters</span>
            <select class="select" bind:value={rename.letterCase}>
              <option value="keep">As typed</option>
              <option value="lower">lowercase</option>
              <option value="upper">UPPERCASE</option>
            </select>
          </label>
        </div>
        <label class="row">
          <span class="label">Spaces</span>
          <span class="spacer"></span>
          <select class="select" value={rename.spaces ?? 'keep'} onchange={(e) => {
            const v = (e.target as HTMLSelectElement).value;
            rename.spaces = v === 'keep' ? null : v;
          }}>
            <option value="keep">Keep spaces</option>
            <option value="_">Use _</option>
            <option value="-">Use -</option>
            <option value="">Remove</option>
          </select>
        </label>
      {:else if directory === null}
        <div class="row">
          <span class="label">Add to file names</span>
          <span class="spacer"></span>
          <input class="input suffix" bind:value={suffix} />
        </div>
      {/if}
    </Section>

    <Section title="Save">
      <div class="row">
        <span class="label">Format</span>
        <span class="spacer"></span>
        <select class="select" bind:value={format}>
          <option value="keep">Same as original</option>
          <option value="jpeg">JPEG</option>
          <option value="png">PNG</option>
          <option value="webp">WebP</option>
          <option value="avif">AVIF</option>
          <option value="tiff">TIFF</option>
        </select>
      </div>
      {#if !pureRename}
        <Slider label="Quality" value={quality} min={40} max={100} unit="%" onchange={(v) => (quality = Math.round(v))} />
      {/if}
      <Segmented
        options={[
          { value: 'same', label: 'Next to originals' },
          { value: 'save', label: 'Save folder' },
          { value: 'pick', label: 'Choose…' },
        ]}
        value={directory === null ? 'same' : directory === app.settings.saveDirectory ? 'save' : 'pick'}
        onchange={async (v) => {
          if (v === 'same') directory = null;
          else if (v === 'save') directory = app.settings.saveDirectory || null;
          else {
            const d = await pickFolder('Save processed photos to', directory ?? app.settings.saveDirectory);
            if (d) directory = d;
          }
        }}
      />
      {#if directory}<div class="dir"><FolderOpen size={14} /><span class="truncate">{directory}</span></div>{/if}
      <div class="row">
        <span class="label">If a file exists</span>
        <span class="spacer"></span>
        <select class="select" bind:value={conflict}>
          <option value="rename">Keep both</option>
          <option value="skip">Skip</option>
          <option value="overwrite">Replace</option>
        </select>
      </div>
      {#if canMove}
        <Toggle checked={moveOriginals} label="Rename the originals instead of copying" hint="Nothing else changes, so this is safe and instant." onchange={(v) => (moveOriginals = v)} />
      {/if}
    </Section>

    <Section title="Metadata" open={false}>
      <MetadataOptions value={metadata} onchange={(v) => (metadata = v)} />
    </Section>

    <div class="go">
      {#if queue.progress}
        <div class="bar"><span style:width="{(queue.progress.done / Math.max(1, queue.progress.total)) * 100}%"></span></div>
        <span class="hint truncate">{queue.progress.done}/{queue.progress.total} {queue.progress.current?.split('/').pop() ?? ''}</span>
        <button class="btn block" onclick={() => void api.cancelBatch()}>Cancel</button>
      {:else}
        <button class="btn primary large block" disabled={!queue.files.length || (!doResize && !doWatermark && !doRename && format === 'keep')} onclick={run}>
          {#if queue.running}<LoaderCircle size={16} class="spin" />{:else}<Wand size={16} />{/if}
          Process {queue.files.length || ''} image{queue.files.length === 1 ? '' : 's'}
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
    padding: 12px;
    gap: 10px;
  }
  .stage {
    flex: 1;
    min-height: 0;
    display: grid;
    place-items: center;
    background: var(--bg-2);
    border-radius: 8px;
    overflow: hidden;
  }
  .info {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .empty {
    flex: 1;
    display: grid;
    place-items: center;
  }
  .names {
    max-height: 26%;
    min-height: 120px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .names header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }
  .names h3 {
    font-size: 14px;
    flex: 1;
  }
  .list {
    overflow-y: auto;
    padding: 6px 16px;
  }
  .nm {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 10px;
    padding: 3px 0;
    font-size: 12.5px;
  }
  .old {
    color: var(--text-3);
  }
  .arrow {
    color: var(--text-3);
  }
  .new {
    font-weight: 600;
  }
  .options {
    width: 340px;
    flex: none;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .options .select,
  .options .input {
    min-width: 0;
  }
  .presets,
  .tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    height: 24px;
    padding: 0 8px;
    border-radius: 99px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-2);
    font-size: 11.5px;
    cursor: pointer;
  }
  .chip:hover {
    color: var(--text);
    border-color: var(--accent);
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .grid2 label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .suffix {
    width: 120px;
  }
  .pos-grid {
    display: grid;
    grid-template-columns: repeat(3, 34px);
    gap: 4px;
  }
  .pos {
    width: 34px;
    height: 26px;
    border-radius: 6px;
    border: 1px solid var(--border-strong);
    background: var(--bg-2);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .pos span {
    width: 8px;
    height: 8px;
    border-radius: 2px;
    background: var(--text-3);
  }
  .pos.on {
    border-color: var(--accent);
  }
  .pos.on span {
    background: var(--accent);
  }
  .dir {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    color: var(--text-2);
    font-size: 12px;
    min-width: 0;
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
