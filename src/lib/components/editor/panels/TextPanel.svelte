<script lang="ts">
  import { fontInfo } from '../../../assets/fonts';
  import { editor } from '../../../editor/editor.svelte';
  import type { TextElement } from '../../../model/types';
  import { naturalTextWidth } from '../../../render/text';
  import { presetElement, TEXT_PRESETS as presets, type TextPreset } from '../../../text/styles';

  // The presets live in text/styles.ts: the story canvas and section titles use them too.
  function add(p: TextPreset) {
    const S = editor.pageShort;
    const size = p.size * S;
    const look = presetElement(p, size, S);
    const draft = { ...look, fill: look.fill ?? '#1d1b26' } as Partial<TextElement>;
    // One undo step for adding, sizing and centring.
    editor.begin();
    const el = editor.addText(draft);
    const w = el.curve ? el.w : Math.min(editor.doc.page.width * 0.9, naturalTextWidth({ ...el }) * 1.04);
    editor.patch(el.id, { w, x: editor.doc.page.width / 2 - w / 2 } as Partial<TextElement>);
    editor.end();
  }
</script>

<div class="head">
  <h3>Text</h3>
  <p class="hint">Click a style to add it, then double-click the text on the page to edit it.</p>
</div>
<div class="list">
  {#each presets as p (p.id)}
    {@const f = fontInfo(p.look.font)}
    {@const outline = !!p.look.stroke?.width}
    <button class="preset" onclick={() => add(p)}>
      <span
        class="sample"
        style:font-family="'{f.family}'"
        style:font-weight={p.look.weight ?? 400}
        style:letter-spacing="{p.look.letterSpacing ?? 0}em"
        style:color={p.look.background || outline ? '#fff' : undefined}
        style:-webkit-text-stroke={outline ? '1.2px #000' : undefined}
        style:paint-order={outline ? 'stroke fill' : undefined}
        style:background={p.look.background?.color}
        class:pill={!!p.look.background}>{p.sample}</span
      >
      <span class="lbl">{p.label}</span>
    </button>
  {/each}
</div>

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
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 4px 14px 20px;
  }
  .preset {
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
  .preset:hover {
    border-color: var(--accent);
  }
  .sample {
    font-size: 21px;
    line-height: 1.25;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }
  .sample.pill {
    font-size: 13px;
    padding: 3px 10px;
    border-radius: 99px;
  }
  .lbl {
    font-size: 11px;
    color: var(--text-3);
    font-weight: 600;
  }
</style>
