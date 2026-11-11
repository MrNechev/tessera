<script lang="ts">
  import { ImagePlus } from '@lucide/svelte';
  import { untrack } from 'svelte';
  import { pickImages } from '../api/backend';
  import type { ImportedImage } from '../api/types';
  import { buildPhotoBook, buildPhotoCalendar, type BookStyle } from '../editor/projects';
  import { openEditor } from '../editor/session.svelte';
  import { app } from '../stores/app.svelte';
  import { library } from '../stores/library.svelte';
  import { toasts } from '../stores/toasts.svelte';
  import Modal from './ui/Modal.svelte';
  import Segmented from './ui/Segmented.svelte';
  import Toggle from './ui/Toggle.svelte';

  interface Props {
    kind: 'book' | 'calendar';
    onclose: () => void;
  }
  let { kind, onclose }: Props = $props();

  const now = new Date();
  let source = $state<'library' | 'pick'>(library.items.length ? 'library' : 'pick');
  let picked = $state<ImportedImage[]>([]);
  let busy = $state(false);

  // Photo book
  let presetId = $state('scrapbook-12');
  let orientation = $state<'portrait' | 'landscape'>('landscape');
  let perPage = $state(0);
  // The dialog is opened for one kind; its default title is fixed at creation.
  let title = $state(untrack(() => (kind === 'book' ? 'Our Story' : 'Our Year')));
  let subtitle = $state(String(now.getFullYear()));
  let style = $state<BookStyle>('clean');
  let pageNumbers = $state(true);

  // Calendar
  let year = $state(now.getMonth() >= 9 ? now.getFullYear() + 1 : now.getFullYear());
  let firstDay = $state<0 | 1>(1);

  const photos = $derived(source === 'library' ? library.items : picked);

  async function pick() {
    const files = await pickImages(app.settings.recentImportDir);
    if (!files.length) return;
    busy = true;
    picked = await library.import(files);
    busy = false;
    source = 'pick';
  }

  function create() {
    if (!photos.length) {
      toasts.push({ kind: 'warning', title: 'Choose some photos first' });
      return;
    }
    const list = [...photos].sort((a, b) => (a.takenAt ?? '').localeCompare(b.takenAt ?? ''));
    const doc =
      kind === 'book'
        ? buildPhotoBook(list, { presetId, orientation, perPage, title, subtitle, style, pageNumbers })
        : buildPhotoCalendar(list, { year, firstDay, title });
    openEditor(doc);
    onclose();
  }
</script>

<Modal title={kind === 'book' ? 'New photo book' : 'New photo calendar'} {onclose} width={500}>
  <div class="col">
    <span class="label">Photos</span>
    <div class="row">
      {#if library.items.length}
        <Segmented
          full={false}
          options={[
            { value: 'library', label: `My ${library.items.length} imported photos` },
            { value: 'pick', label: picked.length ? `${picked.length} chosen` : 'Choose…' },
          ]}
          value={source}
          onchange={(v) => (v === 'pick' && !picked.length ? void pick() : (source = v as 'library' | 'pick'))}
        />
      {/if}
      <button class="btn small" onclick={pick} disabled={busy}><ImagePlus size={14} /> {busy ? 'Importing…' : 'Choose photos'}</button>
    </div>
    <p class="hint">
      {photos.length} photo{photos.length === 1 ? '' : 's'}, sorted by the date they were taken.
      {kind === 'calendar' ? 'One photo per month — photos repeat if there are fewer than 13.' : ''}
    </p>

    <label class="col tight"><span class="label">Title</span><input class="input" bind:value={title} /></label>

    {#if kind === 'book'}
      <label class="col tight"><span class="label">Subtitle</span><input class="input" bind:value={subtitle} /></label>
      <div class="grid2">
        <label class="col tight">
          <span class="label">Page size</span>
          <select class="select" bind:value={presetId}>
            <option value="scrapbook-12">Square 12×12″</option>
            <option value="square">Square (screen)</option>
            <option value="a4">A4</option>
            <option value="letter">US Letter</option>
            <option value="photo-8x10">8×10″</option>
          </select>
        </label>
        <label class="col tight">
          <span class="label">Photos per page</span>
          <select class="select" bind:value={perPage}>
            <option value={0}>Varied (1–6)</option>
            {#each [1, 2, 3, 4, 6, 8] as n (n)}<option value={n}>{n}</option>{/each}
          </select>
        </label>
      </div>
      {#if presetId !== 'scrapbook-12' && presetId !== 'square'}
        <Segmented options={[{ value: 'landscape', label: 'Landscape' }, { value: 'portrait', label: 'Portrait' }]} value={orientation} onchange={(v) => (orientation = v as 'portrait' | 'landscape')} />
      {/if}
      <Segmented
        options={[
          { value: 'clean', label: 'Clean white' },
          { value: 'warm', label: 'Warm paper' },
          { value: 'dark', label: 'Dark' },
        ]}
        value={style}
        onchange={(v) => (style = v as BookStyle)}
      />
      <Toggle checked={pageNumbers} label="Page numbers" onchange={(v) => (pageNumbers = v)} />
    {:else}
      <div class="grid2">
        <label class="col tight">
          <span class="label">Year</span>
          <select class="select" bind:value={year}>
            {#each [now.getFullYear() - 1, now.getFullYear(), now.getFullYear() + 1, now.getFullYear() + 2] as y (y)}<option value={y}>{y}</option>{/each}
          </select>
        </label>
        <label class="col tight">
          <span class="label">Week starts on</span>
          <select class="select" bind:value={firstDay}>
            <option value={1}>Monday</option>
            <option value={0}>Sunday</option>
          </select>
        </label>
      </div>
      <p class="hint">A4 portrait: a cover and one page per month. Mark birthdays on each month in the Properties panel.</p>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" onclick={create} disabled={!photos.length || busy}>Create {kind === 'book' ? 'photo book' : 'calendar'}</button>
  {/snippet}
</Modal>

<style>
  .tight {
    gap: 4px;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
</style>
