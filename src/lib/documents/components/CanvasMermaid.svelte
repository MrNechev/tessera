<script lang="ts">
  // Mermaid import: type or paste a flowchart; it becomes shapes and connectors laid out
  // automatically (all editable afterwards).
  import type { CanvasEditor } from '../lib/canvasEditor.svelte';
  import { parseMermaid } from '../lib/diagram/mermaid';
  import Icon from './Icon.svelte';

  let { ed }: { ed: CanvasEditor } = $props();

  const EXAMPLE = `flowchart TD
  A([Start]) --> B[Write the plan]
  B --> C{Looks good?}
  C -->|Yes| D[Do it]
  C -->|No| B
  D --> E([Done])`;

  const text = $derived(ed.mermaidText ?? '');
  const check = $derived.by(() => {
    if (!text.trim()) return { ok: false, msg: '' };
    try {
      const g = parseMermaid(text);
      return { ok: g.nodes.length > 0, msg: `${g.nodes.length} box${g.nodes.length === 1 ? '' : 'es'}, ${g.edges.length} link${g.edges.length === 1 ? '' : 's'}${g.groups.length ? `, ${g.groups.length} group${g.groups.length === 1 ? '' : 's'}` : ''}` };
    } catch (e) {
      return { ok: false, msg: (e as Error).message };
    }
  });

  function run() {
    if (ed.importMermaid(text)) ed.mermaidText = null;
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="back" onpointerdown={(e) => e.stopPropagation()} onclick={(e) => e.target === e.currentTarget && (ed.mermaidText = null)}>
  <div class="dlg" role="dialog" aria-label="Import a Mermaid flowchart">
    <header>
      <b>Mermaid flowchart</b>
      <span class="grow"></span>
      <button class="icon-btn" title="Close (Esc)" onclick={() => (ed.mermaidText = null)}><Icon name="x" /></button>
    </header>
    <p class="muted">Paste a flowchart written in Mermaid (it starts with <code>flowchart TD</code> or <code>graph LR</code>). Boxes, links, labels and subgraphs become shapes, connectors and frames.</p>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      class="input"
      spellcheck="false"
      placeholder={EXAMPLE}
      value={text}
      oninput={(e) => (ed.mermaidText = e.currentTarget.value)}
      onkeydown={(e) => {
        e.stopPropagation();
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) run();
        if (e.key === 'Escape') ed.mermaidText = null;
      }}
      autofocus
    ></textarea>
    <footer>
      <span class="status" class:bad={!!text.trim() && !check.ok}>{check.msg}</span>
      <span class="grow"></span>
      {#if !text.trim()}<button class="btn sm ghost" onclick={() => (ed.mermaidText = EXAMPLE)}>Use the example</button>{/if}
      <button class="btn sm" onclick={() => (ed.mermaidText = null)}>Cancel</button>
      <button class="btn sm primary" disabled={!check.ok} onclick={run}>Add to the canvas</button>
    </footer>
  </div>
</div>

<style>
  .back {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--bg) 45%, transparent);
    display: grid;
    place-items: center;
    z-index: 9;
  }
  .dlg {
    width: min(620px, calc(100% - 32px));
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 14px;
    box-shadow: var(--shadow);
    padding: 12px 16px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  header,
  footer {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .grow {
    flex: 1;
  }
  p {
    margin: 0;
    font-size: 13px;
  }
  code {
    font-family: var(--mono);
    font-size: 12px;
  }
  textarea {
    min-height: 220px;
    font: 13px/1.45 var(--mono);
    resize: vertical;
  }
  .status {
    font-size: 12px;
    color: var(--text-2);
  }
  .status.bad {
    color: var(--red);
  }
</style>
