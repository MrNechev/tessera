<script lang="ts">
  import type { ExportFormat } from '../../api/types';
  import { editor } from '../../editor/editor.svelte';
  import { saveAs } from '../../editor/session.svelte';
  import { app } from '../../stores/app.svelte';
  import Modal from '../ui/Modal.svelte';
  import Segmented from '../ui/Segmented.svelte';
  import Slider from '../ui/Slider.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let format = $state<ExportFormat>(app.settings.exportFormat);
  let quality = $state(app.settings.exportQuality);
  let scale = $state(1);

  const lossy = $derived(format === 'jpeg' || format === 'webp' || format === 'avif' || format === 'pdf');
  const W = $derived(Math.round(editor.doc.page.width * scale));
  const H = $derived(Math.round(editor.doc.page.height * scale));
  const notes: Record<ExportFormat, string> = {
    jpeg: 'Best for sharing and printing photos. Small files, no transparency.',
    png: 'Lossless. Larger files; perfect quality for text-heavy designs.',
    webp: 'Modern format: much smaller than JPEG at the same quality.',
    avif: 'Smallest files with excellent quality. Slower to save; not supported everywhere yet.',
    tiff: 'Lossless, for professional print workflows.',
    pdf: 'A single-page PDF at the exact page size — ideal for print shops.',
  };

  async function go() {
    onclose();
    await saveAs(format, quality, scale);
  }
</script>

<Modal title="Save image as" {onclose} width={500}>
  <div class="col">
    <span class="label">Format</span>
    <Segmented
      options={[
        { value: 'jpeg', label: 'JPEG' },
        { value: 'png', label: 'PNG' },
        { value: 'webp', label: 'WebP' },
        { value: 'avif', label: 'AVIF' },
        { value: 'tiff', label: 'TIFF' },
        { value: 'pdf', label: 'PDF' },
      ]}
      value={format}
      onchange={(v) => (format = v)}
    />
    <p class="hint">{notes[format]}</p>

    {#if lossy}
      <Slider label="Quality" value={quality} min={40} max={100} step={1} unit="%" onchange={(v) => (quality = Math.round(v))} />
    {/if}

    <span class="label">Size</span>
    <Segmented
      options={[
        { value: 0.5, label: '50%' },
        { value: 1, label: '100%' },
        { value: 1.5, label: '150%' },
        { value: 2, label: '200%' },
      ]}
      value={scale}
      onchange={(v) => (scale = v)}
    />
    <p class="hint">{W} × {H} px · {(W * H / 1e6).toFixed(1)} MP · {Math.round(editor.doc.page.dpi * scale)} DPI</p>
  </div>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={go}>Choose location…</button>
  {/snippet}
</Modal>
