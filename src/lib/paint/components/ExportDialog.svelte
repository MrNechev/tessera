<script lang="ts">
  /** Export the drawing as a standard image into Outputs/Drawings (and/or download it in the browser). */
  import { Download, ImageDown } from '@lucide/svelte';
  import { isTauri } from '../../api/backend';
  import Modal from '../../components/ui/Modal.svelte';
  import { downloadDrawing, exportDrawing, exportFileName, type ExportFormat } from '../session.svelte';
  import { paint } from '../state.svelte';

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();
  const d = $derived(paint.doc!);

  const KEY = 'tessera.paint.export';
  let saved: { format?: ExportFormat; quality?: number } = {};
  try {
    saved = JSON.parse(localStorage.getItem(KEY) ?? '{}');
  } catch {
    saved = {};
  }
  const transparent = paint.doc!.hasTransparency();
  let format = $state<ExportFormat>(saved.format ?? (transparent ? 'png' : 'jpeg'));
  let quality = $state(saved.quality ?? 90);
  let scale = $state(1);

  function remember() {
    try {
      localStorage.setItem(KEY, JSON.stringify({ format, quality }));
    } catch {
      /* ignore */
    }
  }

  async function go(download: boolean) {
    remember();
    const res = await exportDrawing(format, quality, scale, download);
    if (res) onclose();
  }

  async function local() {
    remember();
    await downloadDrawing(format, quality, scale);
    onclose();
  }

  const FORMATS: { id: ExportFormat; label: string; hint: string }[] = [
    { id: 'png', label: 'PNG', hint: 'Sharp, keeps transparency' },
    { id: 'jpeg', label: 'JPEG', hint: 'Photos, smallest files' },
    { id: 'webp', label: 'WebP', hint: 'Small, keeps transparency' },
  ];
</script>

<Modal title="Export image" width={480} {onclose}>
  <div class="formats">
    {#each FORMATS as f (f.id)}
      <button class="fmt" class:on={format === f.id} onclick={() => (format = f.id)}>
        <b>{f.label}</b>
        <span class="hint">{f.hint}</span>
      </button>
    {/each}
  </div>
  {#if format !== 'png'}
    <label class="q">
      <span>Quality</span>
      <input type="range" min="40" max="100" value={quality} style:--fill="{((quality - 40) / 60) * 100}%" oninput={(e) => (quality = +e.currentTarget.value)} />
      <span class="v">{quality}</span>
    </label>
  {/if}
  <label class="q">
    <span>Size</span>
    <select class="select" bind:value={scale}>
      <option value={1}>{d.width} × {d.height} (full)</option>
      <option value={0.5}>{Math.round(d.width / 2)} × {Math.round(d.height / 2)} (half)</option>
      <option value={0.25}>{Math.round(d.width / 4)} × {Math.round(d.height / 4)} (quarter)</option>
      {#if d.width * d.height <= 4_000_000}<option value={2}>{d.width * 2} × {d.height * 2} (double)</option>{/if}
    </select>
  </label>
  {#if format === 'jpeg' && transparent}
    <p class="hint">JPEG has no transparency: transparent parts become white.</p>
  {/if}
  <p class="hint where">Saved as a new file: <b>Outputs › Drawings › {exportFileName(d, format)}</b>. Your layers stay in the drawing, so you can come back and change it.</p>
  {#snippet footer()}
    {#if !isTauri}
      <button class="btn ghost" style:margin-right="auto" title="Download straight to this computer (not saved in Outputs)" onclick={() => void local()}><Download size={15} /> Download only</button>
    {/if}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={!!paint.busy} onclick={() => void go(false)}><ImageDown size={15} /> {isTauri ? 'Export' : 'Export & download'}</button>
  {/snippet}
</Modal>

<style>
  .formats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .fmt {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 10px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--panel-2);
    cursor: pointer;
    text-align: left;
  }
  .fmt.on {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .fmt .hint {
    font-size: 11px;
  }
  .q {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
    color: var(--text-2);
    font-size: 12px;
  }
  .q span:first-child {
    width: 52px;
  }
  .q input[type='range'] {
    flex: 1;
  }
  .q .select {
    flex: 1;
  }
  .v {
    width: 30px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .where {
    margin-top: 14px;
  }
</style>
