<script lang="ts">
  import { editor } from '../../../editor/editor.svelte';
  import { newFromTemplate } from '../../../editor/session.svelte';
  import { library } from '../../../stores/library.svelte';
  import { TEMPLATES } from '../../../templates/catalog';
  import { TEMPLATE_CATEGORIES, type TemplateCategory, type TemplateDef } from '../../../templates/types';
  import Modal from '../../ui/Modal.svelte';
  import Toggle from '../../ui/Toggle.svelte';
  import TemplateThumb from '../TemplateThumb.svelte';

  interface Props {
    /** Wide gallery mode (home screen). */
    wide?: boolean;
    onpick?: (t: TemplateDef) => void;
  }
  let { wide = false, onpick }: Props = $props();

  let category = $state<TemplateCategory | 'all'>('all');
  let confirm = $state<TemplateDef | null>(null);
  let useLibrary = $state(true);

  const list = $derived(category === 'all' ? TEMPLATES : TEMPLATES.filter((t) => t.category === category));
  const cats = TEMPLATE_CATEGORIES.filter((c) => TEMPLATES.some((t) => t.category === c.id));

  function pick(t: TemplateDef) {
    if (onpick) return onpick(t);
    if (editor.doc.elements.length > 0) confirm = t;
    else newFromTemplate(t, useLibrary);
  }
</script>

{#if !wide}
  <div class="head">
    <h3>Templates</h3>
    <p class="hint">Ready-made designs for every occasion. Your photos drop right in.</p>
  </div>
{/if}

<div class="chips" class:wide>
  <button class="chip" class:on={category === 'all'} onclick={() => (category = 'all')}>All</button>
  {#each cats as c (c.id)}
    <button class="chip" class:on={category === c.id} onclick={() => (category = c.id)}><span class="emoji">{c.emoji}</span> {c.name}</button>
  {/each}
</div>

<div class="grid" class:wide>
  {#each list as t (t.id)}
    <button class="tpl" onclick={() => pick(t)} title={t.description}>
      <TemplateThumb template={t} width={wide ? 260 : 130} />
      <span class="name">{t.name}</span>
      {#if wide}<span class="desc">{t.description}</span>{/if}
    </button>
  {/each}
</div>

{#if confirm}
  {@const t = confirm}
  <Modal title="Start from “{t.name}”?" onclose={() => (confirm = null)} width={440}>
    <p>This opens the template as a new collage. Your current collage is saved automatically and stays in <b>Recent</b> on the home screen.</p>
    {#if library.items.length}
      <Toggle
        checked={useLibrary}
        label="Fill it with my imported photos"
        hint="{library.items.length} photo{library.items.length > 1 ? 's' : ''} in your library"
        onchange={(v) => (useLibrary = v)}
      />
    {/if}
    {#snippet footer()}
      <button class="btn" onclick={() => (confirm = null)}>Cancel</button>
      <button
        class="btn primary"
        onclick={() => {
          newFromTemplate(t, useLibrary);
          confirm = null;
        }}>Use template</button
      >
    {/snippet}
  </Modal>
{/if}

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
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 4px 14px 12px;
  }
  .chips.wide {
    padding: 0 0 16px;
  }
  .chip {
    height: 26px;
    padding: 0 10px;
    border-radius: 99px;
    border: 1px solid var(--border-strong);
    background: transparent;
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .chip:hover {
    color: var(--text);
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .emoji {
    opacity: 0.8;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 10px;
    padding: 0 14px 20px;
    align-items: start;
  }
  .grid.wide {
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 22px 18px;
    padding: 0;
  }
  .tpl {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
    text-align: left;
  }
  .tpl :global(.thumb) {
    transition: transform 0.15s, box-shadow 0.15s;
  }
  .tpl:hover :global(.thumb) {
    transform: translateY(-2px);
    box-shadow: 0 0 0 2px var(--accent), var(--shadow);
  }
  .name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
  }
  .desc {
    font-size: 11.5px;
    color: var(--text-3);
    margin-top: -3px;
  }
</style>
