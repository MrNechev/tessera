<script lang="ts">
  import { fontInfo } from '../../../assets/fonts';
  import { editor } from '../../../editor/editor.svelte';
  import type { TextElement } from '../../../model/types';
  import { naturalTextWidth } from '../../../render/text';

  interface Preset {
    label: string;
    sample: string;
    el: Partial<TextElement>;
    /** Size as a fraction of the page's short side. */
    size: number;
  }

  const presets: Preset[] = [
    { label: 'Heading', sample: 'Add a heading', size: 0.09, el: { font: 'playfair-display', weight: 700, text: 'Add a heading' } },
    { label: 'Subheading', sample: 'Add a subheading', size: 0.05, el: { font: 'montserrat', weight: 600, text: 'Add a subheading' } },
    { label: 'Body', sample: 'Add a little bit of body text', size: 0.03, el: { font: 'inter', weight: 400, text: 'Add a little bit of body text' } },
    { label: 'Script', sample: 'With love', size: 0.1, el: { font: 'great-vibes', text: 'With love', fill: '#c2185b' } },
    { label: 'Handwritten', sample: 'best day ever!', size: 0.07, el: { font: 'caveat', weight: 700, text: 'best day ever!', fill: '#2b2340' } },
    { label: 'Retro', sample: 'Summer', size: 0.1, el: { font: 'pacifico', text: 'Summer', fill: '#ff6b4a', stroke: { width: 0, color: '#ffffff' } } },
    { label: 'Poster', sample: 'GOOD TIMES', size: 0.1, el: { font: 'bebas-neue', text: 'GOOD TIMES', letterSpacing: 0.08, fill: '#1d1b26' } },
    { label: 'Bubbly', sample: 'Party time', size: 0.08, el: { font: 'fredoka', weight: 600, text: 'Party time', fill: '#7b2ff7' } },
    { label: 'Marker', sample: 'Adventure!', size: 0.07, el: { font: 'permanent-marker', text: 'Adventure!', fill: '#2f3b2f' } },
    { label: 'Elegant caps', sample: 'FOREVER', size: 0.06, el: { font: 'cinzel', weight: 700, text: 'FOREVER', letterSpacing: 0.25, fill: '#a8813a' } },
    { label: 'Label', sample: 'SALE · NEW · WOW', size: 0.035, el: { font: 'montserrat', weight: 800, text: 'BEST FRIENDS', fill: '#ffffff', background: { color: '#ff5d8f', padding: 0.45, radius: 1 } } },
    { label: 'Arched', sample: 'Happy Birthday', size: 0.07, el: { font: 'shrikhand', text: 'Happy Birthday', fill: '#ff4f81', curve: 0.35 } },
    { label: 'Christmas', sample: 'Merry Christmas', size: 0.08, el: { font: 'mountains-of-christmas', weight: 700, text: 'Merry Christmas', fill: '#c62828' } },
    { label: 'Spooky', sample: 'Boo!', size: 0.1, el: { font: 'creepster', text: 'Boo!', fill: '#ff8c1a' } },
  ];

  function add(p: Preset) {
    const S = editor.pageShort;
    const size = p.size * S;
    const draft = { ...p.el, size, fill: p.el.fill ?? '#1d1b26' } as Partial<TextElement>;
    const el = editor.addText(draft);
    const w = Math.min(editor.doc.page.width * 0.9, naturalTextWidth({ ...el }) * 1.04);
    if (!el.curve) editor.patch(el.id, { w } as Partial<TextElement>);
    editor.patch(el.id, { x: editor.doc.page.width / 2 - w / 2 } as Partial<TextElement>);
  }
</script>

<div class="head">
  <h3>Text</h3>
  <p class="hint">Click a style to add it, then double-click the text on the page to edit it.</p>
</div>
<div class="list">
  {#each presets as p (p.label)}
    {@const f = fontInfo(p.el.font ?? 'inter')}
    <button class="preset" onclick={() => add(p)}>
      <span
        class="sample"
        style:font-family="'{f.family}'"
        style:font-weight={p.el.weight ?? 400}
        style:letter-spacing="{p.el.letterSpacing ?? 0}em"
        style:color={p.el.background ? '#fff' : undefined}
        style:background={p.el.background?.color}
        class:pill={!!p.el.background}>{p.sample}</span
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
