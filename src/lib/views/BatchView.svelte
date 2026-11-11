<script lang="ts">
  import { FolderOpen, ImagePlus, LoaderCircle, Wand } from '@lucide/svelte';
  import { onMount } from 'svelte';
  import { open as openDialog } from '@tauri-apps/plugin-dialog';
  import { api, onProgress, pickFolder } from '../api/backend';
  import type { BatchOptions, MetadataPolicy, ProbeInfo, ResizeMode, WatermarkOptions } from '../api/types';
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
  import { batchOptions as o, busyElsewhere, isNum, reportResults, toInt, toolsQueue as queue } from '../stores/batch.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import { RENAME_TOKENS, baseName, buildNames, sortFiles, type RenameSpec } from '../tools/rename';
  import { canvasToPng, placements, renderTextWatermark, resizedSize, watermarkSize, type TextWatermark, type WatermarkPosition } from '../tools/watermark';

  /* ---------------------------------------------------------- options */

  const wmFonts = FONTS.filter((f) => ['sans', 'display', 'script', 'serif', 'handwriting'].includes(f.category));

  /** Largest pixel size accepted (the backend stores sizes as u32). */
  const MAX_PX = 65535;
  const px = $derived(toInt(o.resizeValue, 16, MAX_PX, 2048));

  const resizeMode = $derived<ResizeMode>(
    !o.doResize
      ? { kind: 'none' }
      : o.resizeKind === 'longEdge'
        ? { kind: 'longEdge', pixels: px }
        : o.resizeKind === 'width'
          ? { kind: 'fit', width: px, height: 0 }
          : o.resizeKind === 'height'
            ? { kind: 'fit', width: 0, height: px }
            : { kind: 'percent', percent: isNum(o.resizeValue) ? Math.min(100, Math.max(1, o.resizeValue)) : 50 },
  );

  /** Nothing changes the pixels or metadata: files can be copied — or renamed in place. */
  const pureRename = $derived(!o.doResize && !o.doWatermark && o.format === 'keep' && o.metadata.mode === 'keepAll');
  const canMove = $derived(pureRename && o.doRename && o.directory === null);

  /** The rename spec with its number fields sanitised (emptied inputs yield null). */
  const renameSpec = $derived<RenameSpec>({ ...o.rename, start: toInt(o.rename.start, 0, 1e9, 1), padding: toInt(o.rename.padding, 0, 8, 3) });

  const ordered = $derived(o.doRename ? sortFiles(queue.files, o.rename.sort) : queue.files);
  const names = $derived(
    o.doRename ? buildNames(ordered, renameSpec) : ordered.map((f) => (o.directory === null ? `${baseName(f.path)}${o.suffix}` : baseName(f.path))),
  );

  const other = $derived(busyElsewhere(queue));
  const invalid = $derived(
    o.doResize && !isNum(o.resizeValue)
      ? `Enter a ${o.resizeKind === 'percent' ? 'percentage' : 'size'} to resize to.`
      : o.doRename && (!isNum(o.rename.start) || !isNum(o.rename.padding))
        ? 'Enter a start number and digits for the new names.'
        : null,
  );
  const blocked = $derived(other ? `Another job is running in ${other.label}.` : invalid);

  function setResizeKind(k: typeof o.resizeKind) {
    o.resizeKind = k;
    o.resizeValue = k === 'percent' ? 50 : 2048;
  }

  /* ----------------------------------------------------- watermark mark */

  let mark = $state<HTMLCanvasElement | ImageBitmap | null>(null);
  let markTimer: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    // Re-render the text watermark when its settings change (debounced).
    const t = $state.snapshot(o.wmText) as TextWatermark;
    const kind = o.wmKind;
    const logo = o.wmLogo;
    const on = o.doWatermark;
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
    if (typeof res === 'string') o.wmLogo = res;
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
    void o.wmSize;
    void o.wmOpacity;
    void o.wmMargin;
    void o.wmPosition;
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
    if (o.doWatermark && mark) {
      // Compute in output pixels, then map to the preview.
      const [ow, oh] = resizedSize(f.width, f.height, resizeMode);
      const k = cw / ow;
      const [mw, mh] = watermarkSize(ow, oh, mark.width, mark.height, o.wmSize);
      ctx.globalAlpha = o.wmOpacity;
      for (const [x, y] of placements(ow, oh, mw, mh, o.wmPosition, o.wmMargin)) ctx.drawImage(mark, x * k, y * k, mw * k, mh * k);
      ctx.globalAlpha = 1;
    }
  });

  /* ---------------------------------------------------------------- run */

  async function run() {
    if (!queue.files.length || queue.running || blocked) return;
    if (o.doWatermark && o.wmKind === 'image' && !o.wmLogo) {
      toasts.push({ kind: 'warning', title: 'Choose a logo first', message: 'Or switch the watermark to text.' });
      return;
    }
    queue.running = true;
    queue.results = [];
    queue.progress = { done: 0, total: queue.files.length, current: null };
    try {
      let watermark: WatermarkOptions | null = null;
      if (o.doWatermark) {
        const image = o.wmKind === 'image' ? o.wmLogo! : await api.saveTempImage(await canvasToPng(await renderTextWatermark($state.snapshot(o.wmText) as TextWatermark)));
        watermark = { image, size: o.wmSize, opacity: o.wmOpacity, position: o.wmPosition, margin: o.wmMargin };
      }
      const options: BatchOptions = {
        resize: resizeMode,
        watermark,
        format: o.format === 'keep' ? { kind: 'keep' } : { kind: 'to', format: o.format },
        quality: o.quality,
        metadata: $state.snapshot(o.metadata) as MetadataPolicy,
        directory: o.directory,
        conflict: o.conflict,
        transfer: canMove && o.moveOriginals ? 'move' : 'copy',
        background: [255, 255, 255],
      };
      const jobs = ordered.map((f, i) => ({ input: f.path, name: names[i] }));
      queue.results = await api.batchProcess(jobs, options);
      reportResults(queue.results, { verb: 'processed', title: 'Batch finished', failTitle: 'Batch failed' });
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
            {#if o.doResize}
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
    {#if o.doRename && queue.files.length}
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
      <Toggle checked={o.doResize} label="Resize photos" hint="Photos are never enlarged." onchange={(v) => (o.doResize = v)} />
      {#if o.doResize}
        <Segmented
          options={[
            { value: 'longEdge', label: 'Long edge' },
            { value: 'width', label: 'Width' },
            { value: 'height', label: 'Height' },
            { value: 'percent', label: '%' },
          ]}
          value={o.resizeKind}
          onchange={(v) => setResizeKind(v as typeof o.resizeKind)}
        />
        <div class="row">
          <input class="input grow" type="number" min="1" bind:value={o.resizeValue} />
          <span class="hint">{o.resizeKind === 'percent' ? '%' : 'px'}</span>
        </div>
        {#if o.resizeKind !== 'percent'}
          <div class="presets">
            {#each [1080, 1600, 2048, 3000, 4096] as p (p)}
              <button class="btn small" class:active={o.resizeValue === p} onclick={() => (o.resizeValue = p)}>{p}</button>
            {/each}
          </div>
        {/if}
      {/if}
    </Section>

    <Section title="Watermark">
      <Toggle checked={o.doWatermark} label="Add a watermark" onchange={(v) => (o.doWatermark = v)} />
      {#if o.doWatermark}
        <Segmented options={[{ value: 'text', label: 'Text' }, { value: 'image', label: 'Logo image' }]} value={o.wmKind} onchange={(v) => (o.wmKind = v as 'text' | 'image')} />
        {#if o.wmKind === 'text'}
          <input class="input" bind:value={o.wmText.text} placeholder="© Your Name" />
          <select class="select" bind:value={o.wmText.font}>
            {#each wmFonts as f (f.id)}<option value={f.id}>{f.family}</option>{/each}
          </select>
          <ColorInput label="Colour" value={o.wmText.color} onchange={(c) => (o.wmText.color = c)} />
          <Toggle checked={o.wmText.weight >= 600} label="Bold" onchange={(v) => (o.wmText.weight = v ? 700 : 400)} />
          <Toggle checked={o.wmText.outline} label="Outline" onchange={(v) => (o.wmText.outline = v)} />
          <Toggle checked={o.wmText.shadow} label="Shadow" onchange={(v) => (o.wmText.shadow = v)} />
        {:else}
          <button class="btn" onclick={chooseLogo}><ImagePlus size={15} /> {o.wmLogo ? o.wmLogo.split('/').pop() : 'Choose logo…'}</button>
          <p class="hint">A PNG with a transparent background works best.</p>
        {/if}
        <span class="label">Position</span>
        <div class="pos-grid">
          {#each POSITIONS as p (p)}
            <button class="pos" class:on={o.wmPosition === p} onclick={() => (o.wmPosition = p)} aria-label="Position {p}"><span></span></button>
          {/each}
        </div>
        <Toggle checked={o.wmPosition === 'tile'} label="Repeat across the photo" onchange={(v) => (o.wmPosition = v ? 'tile' : 'br')} />
        <Slider label="Size" value={o.wmSize} min={0.03} max={0.8} step={0.01} scale={100} unit="%" onchange={(v) => (o.wmSize = v)} />
        <Slider label="Opacity" value={o.wmOpacity} min={0.05} max={1} step={0.01} scale={100} unit="%" onchange={(v) => (o.wmOpacity = v)} />
        {#if o.wmPosition !== 'tile'}
          <Slider label="Distance from edge" value={o.wmMargin} min={0} max={0.15} step={0.005} scale={100} decimals={1} unit="%" onchange={(v) => (o.wmMargin = v)} />
        {/if}
      {/if}
    </Section>

    <Section title="Rename">
      <Toggle checked={o.doRename} label="Rename files" onchange={(v) => (o.doRename = v)} />
      {#if o.doRename}
        <input class="input" bind:value={o.rename.pattern} />
        <div class="tokens">
          {#each RENAME_TOKENS as t (t.token)}
            <button class="chip" title={t.help} onclick={() => (o.rename.pattern += (o.rename.pattern.endsWith(' ') || !o.rename.pattern ? '' : ' ') + t.token)}>{t.token}</button>
          {/each}
        </div>
        <div class="grid2">
          <label><span class="label">Start at</span><input class="input" type="number" min="0" bind:value={o.rename.start} /></label>
          <label><span class="label">Digits</span><input class="input" type="number" min="0" max="8" bind:value={o.rename.padding} /></label>
        </div>
        <div class="grid2">
          <label>
            <span class="label">Order</span>
            <select class="select" bind:value={o.rename.sort}>
              <option value="added">As added</option>
              <option value="name">By file name</option>
              <option value="date">By date taken</option>
            </select>
          </label>
          <label>
            <span class="label">Letters</span>
            <select class="select" bind:value={o.rename.letterCase}>
              <option value="keep">As typed</option>
              <option value="lower">lowercase</option>
              <option value="upper">UPPERCASE</option>
            </select>
          </label>
        </div>
        <label class="row">
          <span class="label">Spaces</span>
          <span class="spacer"></span>
          <select class="select" value={o.rename.spaces ?? 'keep'} onchange={(e) => {
            const v = (e.target as HTMLSelectElement).value;
            o.rename.spaces = v === 'keep' ? null : v;
          }}>
            <option value="keep">Keep spaces</option>
            <option value="_">Use _</option>
            <option value="-">Use -</option>
            <option value="">Remove</option>
          </select>
        </label>
      {:else if o.directory === null}
        <div class="row">
          <span class="label">Add to file names</span>
          <span class="spacer"></span>
          <input class="input suffix" bind:value={o.suffix} />
        </div>
      {/if}
    </Section>

    <Section title="Save">
      <div class="row">
        <span class="label">Format</span>
        <span class="spacer"></span>
        <select class="select" bind:value={o.format}>
          <option value="keep">Same as original</option>
          <option value="jpeg">JPEG</option>
          <option value="png">PNG</option>
          <option value="webp">WebP</option>
          <option value="avif">AVIF</option>
          <option value="tiff">TIFF</option>
        </select>
      </div>
      {#if !pureRename}
        <Slider label="Quality" value={o.quality} min={40} max={100} unit="%" onchange={(v) => (o.quality = Math.round(v))} />
      {/if}
      <Segmented
        options={[
          { value: 'same', label: 'Next to originals' },
          { value: 'save', label: 'Save folder' },
          { value: 'pick', label: 'Choose…' },
        ]}
        value={o.directory === null ? 'same' : o.directory === app.settings.saveDirectory ? 'save' : 'pick'}
        onchange={async (v) => {
          if (v === 'same') o.directory = null;
          else if (v === 'save') o.directory = app.settings.saveDirectory || null;
          else {
            const d = await pickFolder('Save processed photos to', o.directory ?? app.settings.saveDirectory);
            if (d) o.directory = d;
          }
        }}
      />
      {#if o.directory}<div class="dir"><FolderOpen size={14} /><span class="truncate">{o.directory}</span></div>{/if}
      <div class="row">
        <span class="label">If a file exists</span>
        <span class="spacer"></span>
        <select class="select" bind:value={o.conflict}>
          <option value="rename">Keep both</option>
          <option value="skip">Skip</option>
          <option value="overwrite">Replace</option>
        </select>
      </div>
      {#if canMove}
        <Toggle checked={o.moveOriginals} label="Rename the originals instead of copying" hint="Nothing else changes, so this is safe and instant." onchange={(v) => (o.moveOriginals = v)} />
      {/if}
    </Section>

    <Section title="Metadata" open={false}>
      <MetadataOptions value={o.metadata} onchange={(v) => (o.metadata = v)} />
    </Section>

    <div class="go">
      {#if queue.progress}
        <div class="bar"><span style:width="{(queue.progress.done / Math.max(1, queue.progress.total)) * 100}%"></span></div>
        <span class="hint truncate">{queue.progress.done}/{queue.progress.total} {queue.progress.current?.split('/').pop() ?? ''}</span>
        <button class="btn block" onclick={() => void api.cancelBatch()}>Cancel</button>
      {:else}
        {#if blocked && queue.files.length}<p class="hint">{blocked}</p>{/if}
        <button
          class="btn primary large block"
          disabled={!queue.files.length || queue.running || !!blocked || (!o.doResize && !o.doWatermark && !o.doRename && o.format === 'keep')}
          onclick={run}
        >
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
  .go p {
    margin: 0;
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
