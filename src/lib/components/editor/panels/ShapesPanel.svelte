<script lang="ts">
  import { editor } from '../../../editor/editor.svelte';
  import { createPhoto, createShape } from '../../../model/factory';
  import type { Fill, MaskShape, ShapeKind } from '../../../model/types';
  import { MASK_SHAPES } from '../../../render/shapes';

  const shapes: { id: ShapeKind; name: string }[] = [...MASK_SHAPES, { id: 'ring', name: 'Ring' }, { id: 'line', name: 'Line' }];

  const fills: Fill[] = [
    { kind: 'solid', color: '#ff5d8f' },
    { kind: 'solid', color: '#ffc145' },
    { kind: 'solid', color: '#2dd4bf' },
    { kind: 'solid', color: '#8b5cf6' },
    { kind: 'solid', color: '#ffffff' },
    { kind: 'solid', color: '#1d1b26' },
  ];

  let fillIdx = $state(0);

  function preview(id: ShapeKind): string {
    // Tiny SVG preview built from simple primitives.
    switch (id) {
      case 'circle':
        return '<circle cx="20" cy="20" r="16"/>';
      case 'ring':
        return '<circle cx="20" cy="20" r="14" fill="none" stroke="currentColor" stroke-width="4"/>';
      case 'pill':
        return '<rect x="3" y="11" width="34" height="18" rx="9"/>';
      case 'squircle':
        return '<rect x="5" y="5" width="30" height="30" rx="11"/>';
      case 'arch':
        return '<path d="M6 36V20a14 14 0 0 1 28 0v16z"/>';
      case 'heart':
        return '<path d="M20 35 7 22C1 16 5 6 13 6c3 0 5 2 7 4 2-2 4-4 7-4 8 0 12 10 6 16z"/>';
      case 'star':
        return '<path d="m20 3 5 11 12 1-9 8 3 12-11-7-11 7 3-12-9-8 12-1z"/>';
      case 'hexagon':
        return '<path d="M11 5h18l9 15-9 15H11L2 20z"/>';
      case 'octagon':
        return '<path d="M13 4h14l9 9v14l-9 9H13l-9-9V13z"/>';
      case 'diamond':
        return '<path d="M20 3 37 20 20 37 3 20z"/>';
      case 'triangle':
        return '<path d="M20 4 37 35H3z"/>';
      case 'scallop':
        return '<circle cx="20" cy="20" r="14"/><g>' + Array.from({ length: 12 }, (_, i) => `<circle cx="${20 + Math.cos((i * Math.PI) / 6) * 14}" cy="${20 + Math.sin((i * Math.PI) / 6) * 14}" r="4"/>`).join('') + '</g>';
      case 'cloud':
        return '<path d="M10 32a7 7 0 0 1-1-14 10 10 0 0 1 19-4 8 8 0 0 1 3 18z"/>';
      case 'blob':
        return '<path d="M21 4c9 0 15 6 15 14s-4 17-16 17S4 29 4 20 12 4 21 4z"/>';
      case 'line':
        return '<rect x="3" y="18" width="34" height="4" rx="2"/>';
      default:
        return '<rect x="5" y="7" width="30" height="26" rx="2"/>';
    }
  }

  function add(id: ShapeKind) {
    const S = editor.pageShort;
    const w = id === 'line' ? S * 0.5 : S * 0.25;
    const h = id === 'line' ? S * 0.008 : id === 'pill' ? S * 0.12 : S * 0.25;
    const fill = id === 'ring' ? null : fills[fillIdx];
    const stroke = id === 'ring' ? { width: S * 0.012, color: (fills[fillIdx] as { color: string }).color } : null;
    editor.place(createShape({ x: 0, y: 0, w, h, shape: id, fill, stroke, radius: id === 'rect' ? 0.08 : 0 }));
  }

  function addFrame(id: MaskShape) {
    const S = editor.pageShort;
    editor.place(createPhoto({ x: 0, y: 0, w: S * 0.35, h: S * 0.35, mask: { shape: id, radius: id === 'rect' ? 0.06 : 0 } }));
  }
</script>

<div class="head">
  <h3>Shapes</h3>
  <p class="hint">Decorative shapes, plus empty photo frames you can drop photos into.</p>
</div>
<div class="fills">
  {#each fills as f, i (i)}
    <button class="fill" class:on={fillIdx === i} style:background={(f as { color: string }).color} onclick={() => (fillIdx = i)} aria-label="Shape colour"></button>
  {/each}
</div>
<div class="sub">Shapes</div>
<div class="grid">
  {#each shapes as s (s.id)}
    <button class="sh" title={s.name} onclick={() => add(s.id)} style:color={(fills[fillIdx] as { color: string }).color}>
      <svg viewBox="0 0 40 40" fill="currentColor" aria-hidden="true">{@html preview(s.id)}</svg>
    </button>
  {/each}
</div>
<div class="sub">Photo frames</div>
<div class="grid">
  {#each MASK_SHAPES as s (s.id)}
    <button class="sh frame" title="{s.name} photo frame" onclick={() => addFrame(s.id)}>
      <svg viewBox="0 0 40 40" fill="currentColor" aria-hidden="true">{@html preview(s.id)}</svg>
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
  .fills {
    display: flex;
    gap: 6px;
    padding: 4px 14px 8px;
  }
  .fill {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 0;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.2);
  }
  .fill.on {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--accent);
  }
  .sub {
    padding: 10px 14px 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
    padding: 0 14px 8px;
  }
  .sh {
    aspect-ratio: 1;
    padding: 10px;
    border-radius: 9px;
    border: 1px solid transparent;
    background: var(--panel-2);
    cursor: pointer;
  }
  .sh:hover {
    border-color: var(--accent);
  }
  .sh svg {
    width: 100%;
    height: 100%;
  }
  .frame {
    color: var(--text-3);
  }
</style>
