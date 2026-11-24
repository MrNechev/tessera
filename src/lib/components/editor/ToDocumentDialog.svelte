<script lang="ts">
  /**
   * Put the collage into a document: the pages are saved as images into
   * Outputs (a library, so documents can reference them) and added as a new
   * section of a new or an existing document.
   */
  import { onMount } from 'svelte';
  import { api as docApi, newId } from '../../documents/lib/api';
  import { router } from '../../documents/lib/router.svelte';
  import type { DocMeta, Section } from '../../documents/lib/types';
  import { editor } from '../../editor/editor.svelte';
  import { exportToOutputs, saveState } from '../../editor/session.svelte';
  import { app } from '../../stores/app.svelte';
  import { toasts } from '../../stores/toasts.svelte';
  import Modal from '../ui/Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let docs = $state<DocMeta[]>([]);
  let target = $state<string>('new');
  let title = $state(editor.doc.name);
  let scope = $state<'all' | 'current'>('all');
  let busy = $state(false);

  onMount(async () => {
    try {
      docs = (await docApi.docs()).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    } catch {
      docs = [];
    }
  });

  async function add() {
    busy = true;
    try {
      const refs = await exportToOutputs(scope);
      if (!refs) return;
      const section: Section = {
        id: newId(),
        title: editor.doc.name,
        layout: refs.length > 1 ? 'gallery' : 'media-top',
        media: refs.map((r) => ({ ...r, fit: 'contain' as const })),
        text: { type: 'doc', content: [{ type: 'paragraph' }] },
        slide: true,
      };
      let id: string;
      if (target === 'new') {
        const d = await docApi.importDoc({ title: title.trim() || editor.doc.name, sections: [section] });
        id = d.id;
      } else {
        const d = await docApi.getDoc(target);
        d.sections.push(section);
        await docApi.saveDoc(d.id, d.version, d);
        id = d.id;
      }
      toasts.push({ kind: 'success', title: 'Added to the story', message: `${refs.length} image${refs.length === 1 ? '' : 's'} saved in Outputs.` });
      onclose();
      router.go(`/doc/${id}?s=${section.id}`);
      app.view = 'documents';
    } catch (e) {
      toasts.push({ kind: 'error', title: 'Could not add the collage', message: String(e instanceof Error ? e.message : e) });
    } finally {
      busy = false;
    }
  }
</script>

<Modal title="Add to a story" width={460} {onclose}>
  <div class="col">
    <p class="hint">The collage is saved as {editor.pageCount > 1 ? 'images' : 'an image'} in Outputs and added as a new section.</p>
    <label class="col tight">
      <span class="label">Story</span>
      <select class="select" bind:value={target}>
        <option value="new">New story…</option>
        {#each docs as d (d.id)}<option value={d.id}>{d.title || 'Untitled'}</option>{/each}
      </select>
    </label>
    {#if target === 'new'}
      <label class="col tight"><span class="label">Title</span><input class="input" bind:value={title} /></label>
    {/if}
    {#if editor.pageCount > 1}
      <label class="col tight">
        <span class="label">Pages</span>
        <select class="select" bind:value={scope}>
          <option value="all">All {editor.pageCount} pages</option>
          <option value="current">Only page {editor.activePage + 1}</option>
        </select>
      </label>
    {/if}
  </div>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Cancel</button>
    <button class="btn primary" disabled={busy} onclick={() => void add()}>{busy ? (saveState.busy ? `Saving ${Math.round(saveState.progress * 100)}%` : 'Adding…') : 'Add'}</button>
  {/snippet}
</Modal>

<style>
  .col {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .tight {
    gap: 4px;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-2);
  }
</style>
