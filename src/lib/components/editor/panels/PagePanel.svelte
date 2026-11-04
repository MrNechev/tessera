<script lang="ts">
  import { RectangleHorizontal, RectangleVertical } from '@lucide/svelte';
  import { editor } from '../../../editor/editor.svelte';
  import { DPI_CHOICES, MAX_PAGE_PIXELS, MAX_PAGE_SIDE, PAGE_PRESETS, describePage, presetById, presetSize } from '../../../model/pages';
  import { toasts } from '../../../stores/toasts.svelte';
  import Section from '../../ui/Section.svelte';
  import Segmented from '../../ui/Segmented.svelte';

  const page = $derived(editor.doc.page);
  const preset = $derived(presetById(page.preset));
  const orientation = $derived<'portrait' | 'landscape'>(page.width > page.height ? 'landscape' : 'portrait');
  let customW = $state(0);
  let customH = $state(0);
  $effect.pre(() => {
    customW = page.width;
    customH = page.height;
  });

  const groups = $derived([...new Set(PAGE_PRESETS.map((p) => p.group))]);

  function applyPreset(id: string, orient = orientation, dpi = page.dpi) {
    const p = presetById(id);
    if (!p) return;
    const d = p.px ? 72 : dpi;
    const [w, h] = presetSize(p, d, p.px && p.px[0] === p.px[1] ? 'auto' : orient);
    // Social/screen presets have a fixed orientation unless it's a print preset.
    const [fw, fh] = p.px ? (orient === 'landscape' ? [Math.max(w, h), Math.min(w, h)] : [Math.min(w, h), Math.max(w, h)]) : [w, h];
    const [pw, ph] = p.px && p.px[0] !== p.px[1] && orient === (p.px[0] > p.px[1] ? 'landscape' : 'portrait') ? p.px : [fw, fh];
    editor.setPage(pw, ph, d, p.id);
  }

  function applyCustom() {
    const w = Math.round(customW);
    const h = Math.round(customH);
    if (!(w >= 16 && h >= 16 && w <= MAX_PAGE_SIDE && h <= MAX_PAGE_SIDE && w * h <= MAX_PAGE_PIXELS)) {
      toasts.push({ kind: 'warning', title: 'Unsupported size', message: `Each side must be 16–${MAX_PAGE_SIDE} px and at most ${MAX_PAGE_PIXELS / 1e6} megapixels.` });
      return;
    }
    editor.setPage(w, h, page.dpi, undefined);
  }

  function setOrientation(o: 'portrait' | 'landscape') {
    if (o === orientation) return;
    if (preset && !(preset.px && preset.px[0] === preset.px[1])) applyPreset(preset.id, o);
    else editor.setPage(page.height, page.width, page.dpi, page.preset);
  }

  function setDpi(dpi: number) {
    if (preset && preset.mm) applyPreset(preset.id, orientation, dpi);
    else editor.update((d) => {
      d.page.dpi = dpi;
    });
  }
</script>

<div class="head">
  <h3>Page</h3>
  <p class="hint">{describePage(page.width, page.height, page.dpi)}</p>
</div>

<Section title="Size">
  <select class="select" value={page.preset ?? ''} onchange={(e) => applyPreset((e.target as HTMLSelectElement).value)}>
    <option value="" disabled>Custom size</option>
    {#each groups as g (g)}
      <optgroup label={g}>
        {#each PAGE_PRESETS.filter((p) => p.group === g) as p (p.id)}
          <option value={p.id}>{p.name}{p.px ? ` — ${p.px[0]}×${p.px[1]}` : ''}</option>
        {/each}
      </optgroup>
    {/each}
  </select>
  <Segmented
    options={[
      { value: 'portrait', label: 'Portrait', icon: RectangleVertical },
      { value: 'landscape', label: 'Landscape', icon: RectangleHorizontal },
    ]}
    value={orientation}
    onchange={(v) => setOrientation(v as 'portrait' | 'landscape')}
  />
  <div class="row">
    <span class="label">Print resolution</span>
    <span class="spacer"></span>
    <select class="select" value={page.dpi} onchange={(e) => setDpi(parseInt((e.target as HTMLSelectElement).value))}>
      {#each DPI_CHOICES as d (d)}<option value={d}>{d} DPI</option>{/each}
    </select>
  </div>
  <p class="hint">300 DPI is ideal for printing. Photo layouts and text scale with the page.</p>
</Section>

<Section title="Custom size (pixels)">
  <div class="row">
    <input class="input grow" type="number" min="16" max={MAX_PAGE_SIDE} bind:value={customW} aria-label="Width" />
    <span class="hint">×</span>
    <input class="input grow" type="number" min="16" max={MAX_PAGE_SIDE} bind:value={customH} aria-label="Height" />
  </div>
  <button class="btn block" onclick={applyCustom} disabled={customW === page.width && customH === page.height}>Apply size</button>
</Section>

<style>
  .head {
    padding: 16px 14px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    font-size: 15px;
  }
  .select {
    width: 100%;
  }
  .row .select {
    width: auto;
  }
</style>
