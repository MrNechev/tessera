<script lang="ts">
  /**
   * Put the collage into a story, as a new section of a new or an existing story:
   * - live (the default): the section shows the collage project itself, so later
   *   edits here show up in the story;
   * - as an image: the pages are saved as JPEGs into Outputs (a library, so
   *   stories can reference them), a snapshot that no longer changes.
   */
  import { onMount } from 'svelte';
  import { api as docApi, newId } from '../../documents/lib/api';
  import { refToOpenProject } from '../../documents/lib/collages.svelte';
  import { router } from '../../documents/lib/router.svelte';
  import type { DocMeta, Section } from '../../documents/lib/types';
  import { editor } from '../../editor/editor.svelte';
  import { autosave, exportToOutputs, saveState } from '../../editor/session.svelte';
  import { app } from '../../stores/app.svelte';
  import { toasts } from '../../stores/toasts.svelte';
  import Modal from '../ui/Modal.svelte';

  let { onclose }: { onclose: () => void } = $props();

  let docs = $state<DocMeta[]>([]);
  let target = $state<string>('new');
  let title = $state(editor.doc.name);
  let scope = $state<'all' | 'current'>('all');
  let mode = $state<'live' | 'image'>('live');
  let busy = $state(false);

  onMount(async () => {
    try {
      docs = (await docApi.docs()).sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
    } catch {
      docs = [];
    }
  });

  const emptyText = () => ({ type: 'doc', content: [{ type: 'paragraph' }] });

  /** One live section per page (a story section shows one page). */
  async function liveSections(): Promise<Section[]> {
    // The story points at the project library: make sure this collage is in it, as it is now
    // (a project opened from a .tessera file may never have been saved there).
    await autosave();
    const pages = scope === 'all' ? Array.from({ length: editor.pageCount }, (_, i) => i) : [editor.activePage];
    return pages.map((i) => ({
      id: newId(),
      title: pages.length > 1 ? `${editor.doc.name} · page ${i + 1}` : editor.doc.name,
      layout: 'collage',
      media: [],
      collage: refToOpenProject(i),
      text: emptyText(),
      slide: true,
    }));
  }

  async function imageSections(): Promise<Section[] | null> {
    const refs = await exportToOutputs(scope);
    if (!refs) return null;
    return [
      {
        id: newId(),
        title: editor.doc.name,
        layout: refs.length > 1 ? 'gallery' : 'media-top',
        media: refs.map((r) => ({ ...r, fit: 'contain' as const })),
        text: emptyText(),
        slide: true,
      },
    ];
  }

  async function add() {
    busy = true;
    try {
      const sections = mode === 'live' ? await liveSections() : await imageSections();
      if (!sections) return;
      let id: string;
      if (target === 'new') {
        const d = await docApi.importDoc({ title: title.trim() || editor.doc.name, sections });
        id = d.id;
      } else {
        const d = await docApi.getDoc(target);
        d.sections.push(...sections);
        await docApi.saveDoc(d.id, d.version, d);
        id = d.id;
      }
      toasts.push(
        mode === 'live'
          ? { kind: 'success', title: 'Added to the story', message: 'As a live collage: edit it here and the story follows.' }
          : {
              kind: 'success',
              title: 'Added to the story',
              message: `${sections[0].media.length} image${sections[0].media.length === 1 ? '' : 's'} saved in Outputs.`,
            },
      );
      onclose();
      router.go(`/doc/${id}?s=${sections[0].id}`);
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
    <div class="col tight" role="radiogroup" aria-label="How to add it">
      <label class="choice">
        <input type="radio" name="todoc-mode" value="live" bind:group={mode} />
        <span><b>As a live collage</b> (stays editable)<br /><span class="hint">The story shows this collage; changes you make here show up there too.</span></span>
      </label>
      <label class="choice">
        <input type="radio" name="todoc-mode" value="image" bind:group={mode} />
        <span><b>As an image</b><br /><span class="hint">Saved as {editor.pageCount > 1 ? 'images' : 'an image'} in Outputs: a snapshot that no longer changes.</span></span>
      </label>
    </div>
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
          <option value="all">All {editor.pageCount} pages{mode === 'live' ? ' (one section each)' : ''}</option>
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
  .choice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border, transparent);
    border-radius: 8px;
    cursor: pointer;
  }
  .choice input {
    margin-top: 3px;
  }
</style>
