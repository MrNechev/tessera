<script lang="ts">
  // Context toolbar of the canvas selection: per-kind options (card, frame, note, sticker,
  // shape, arrow, drawing), then align / group / lock / reveal / order / duplicate / delete.
  import { ink, isConnector, NOTE_SIZES, PALETTE, union } from '../lib/canvas';
  import type { CanvasEditor } from '../lib/canvasEditor.svelte';
  import { headsOf } from '../lib/diagram/connectors';
  import { outlineExtra, outlineKind, outlinePath, shapeFields, shapeIdOf, SHAPES, type ShapeId } from '../lib/diagram/shapes';
  import type { ArrowItem, Head, IconItem, NoteItem, Rect, Route, ShapeItem, StickerItem } from '../lib/types';
  import Icon from './Icon.svelte';

  let { ed, onpresentdoc }: { ed: CanvasEditor; onpresentdoc: (sectionIndex: number) => void } = $props();

  const selItems = $derived(ed.selItems);
  const selCards = $derived(ed.selCards);
  const selBoxesCount = $derived(
    [...ed.selected].filter((id) => !ed.isLocked(id) && (ed.secMap.has(id) || (ed.itemMap.get(id) && ed.itemMap.get(id)!.type !== 'arrow'))).length,
  );
  const selKinds = $derived(new Set([...selItems.map((i) => i.type), ...(selCards.length ? ['card'] : [])]));
  const only = (k: string) => selKinds.size === 1 && selKinds.has(k);
  const firstNote = $derived(selItems.find((i) => i.type === 'note') as NoteItem | undefined);
  const firstShape = $derived(selItems.find((i) => i.type === 'rect' || i.type === 'ellipse') as ShapeItem | undefined);
  const firstArrow = $derived(selItems.find((i) => i.type === 'arrow') as ArrowItem | undefined);
  const firstSticker = $derived(selItems.find((i) => i.type === 'sticker') as StickerItem | undefined);
  const firstIcon = $derived(selItems.find((i) => i.type === 'icon') as IconItem | undefined);
  let shapeMenu = $state(false);

  function heads(start: Head, end: Head) {
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      i.start = start;
      i.end = end;
      delete i.head;
    });
  }
  function route(r: Route) {
    ed.setRoute(r);
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      if (r === 'straight') delete i.route;
      else {
        i.route = r;
        delete i.curve;
      }
    });
  }
  function toggleFill(sh: ShapeItem) {
    const on = !!sh.fill || (!!sh.fillColor && sh.fillColor !== 'none');
    ed.restyle((i) => {
      if (i.type !== 'rect' && i.type !== 'ellipse') return;
      delete i.fill;
      if (on) delete i.fillColor;
      else i.fillColor = i.color === 'ink' || i.color.startsWith('#') ? 'gray' : i.color;
    });
  }
  function toggleDash(a: ArrowItem) {
    const on = (a.dash ?? (a.dashed ? 'dashed' : 'solid')) !== 'solid';
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      delete i.dashed;
      if (on) delete i.dash;
      else i.dash = 'dashed';
    });
  }
  function openStyle() {
    ed.stylePanel = !ed.stylePanel;
    ed.styleMenu = false;
  }
  function openText() {
    ed.styleMenu = !ed.styleMenu;
    ed.stylePanel = false;
  }
  function curveBtn(a: ArrowItem) {
    if (isConnector(a) || a.from.id || a.to.id) route('curved');
    else ed.setProp('curve', a.curve ? undefined : 120, ['arrow']);
  }
  function straightBtn() {
    route('straight');
    ed.setProp('curve', undefined, ['arrow']);
  }

  const selBox = $derived.by(() => {
    const u = union([...ed.selected].map((id) => ed.rectOf(id)).filter(Boolean) as Rect[]);
    return u ? ed.toScreen(u) : null;
  });

  const ctxPos = $derived.by(() => {
    if (!selBox || ed.dragging || ed.editing) return null;
    const half = Math.min(ed.ctxW, ed.vw - 16) / 2;
    const x = Math.min(Math.max(selBox.x + selBox.w / 2, half + 8), ed.vw - half - 8);
    const above = selBox.y - 54;
    const y = above > 8 ? above : Math.min(selBox.y + selBox.h + 12, ed.vh - 54);
    return { x, y };
  });
</script>

{#snippet mini(id: ShapeId, size: number)}
  {@const f = shapeFields(id)}
  {@const k = outlineKind({ type: f.type, shape: f.shape })}
  {@const h = id === 'ellipse' || id === 'star' || id === 'triangle' ? size : Math.round(size * 0.72)}
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    <g transform="translate(0 {(size - h) / 2})">
      <path d={outlinePath(k, size, h, 1.8, id === 'round' ? h / 4 : 2)} fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round" />
      {#if outlineExtra(k, size, h, 1.8)}<path d={outlineExtra(k, size, h, 1.8)} fill="none" stroke="currentColor" stroke-width="1.8" />{/if}
    </g>
  </svg>
{/snippet}

{#if ctxPos && ed.selected.size}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="ctx" bind:offsetWidth={ed.ctxW} style="left:{ctxPos.x}px;top:{ctxPos.y}px" onpointerdown={(e) => e.stopPropagation()}>
    {#if only('card') && selCards.length === 1}
      <button class="btn sm" onclick={() => ed.openDrawer(selCards[0].id)}><Icon name="pencil" size={14} /> Edit</button>
      <button class="icon-btn" title="Fit height to the content" onclick={() => ed.fitCardHeight(selCards[0].id)}><Icon name="fit-h" /></button>
      <button class="icon-btn" title="Present the story from this section" onclick={() => onpresentdoc(ed.ds.sectionIndex(selCards[0].id))}><Icon name="present" /></button>
      <span class="vsep"></span>
    {/if}
    {#if only('frame') && selItems.length === 1 && selItems[0].type === 'frame'}
      {@const f = selItems[0]}
      <input
        class="input fname"
        placeholder="Frame title"
        value={f.title}
        onfocus={() => ed.before()}
        oninput={(e) => {
          f.title = e.currentTarget.value;
          ed.change();
        }}
        onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      />
      <span class="muted step">step {ed.items.filter((i) => i.type === 'frame').indexOf(f) + 1}</span>
      <button class="icon-btn" title="Zoom to this frame" onclick={() => ed.zoomToRect(f, 0.04, 4)}><Icon name="maximize" /></button>
      <span class="vsep"></span>
    {/if}
    {#if firstNote && only('note')}
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={firstNote.color === p.key} title={p.label} style="background:{p.paper}" onclick={() => ed.setProp('color', p.key, ['note'])} aria-label={p.label}></button>
      {/each}
      <button class="sw none" class:on={firstNote.color === 'none'} title="No background (text)" onclick={() => ed.setProp('color', 'none', ['note'])} aria-label="No background"></button>
      <span class="vsep"></span>
      {#each NOTE_SIZES as s (s.v)}
        <button class="seg" class:on={firstNote.size === s.v} onclick={() => ed.setProp('size', s.v, ['note'])}>{s.label}</button>
      {/each}
      <input
        class="input fsize"
        type="number"
        min="6"
        max="600"
        title="Text size in points (or pick S…XXL)"
        value={firstNote.size}
        onchange={(e) => {
          const v = Math.round(+e.currentTarget.value);
          if (v >= 6 && v <= 600) ed.setProp('size', v, ['note']);
        }}
        onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
      />
      <span class="vsep"></span>
      {#each ['left', 'center', 'right'] as a (a)}
        <button class="icon-btn" class:on={(firstNote.align || 'left') === a} title="Align {a}" onclick={() => ed.setProp('align', a === 'left' ? undefined : a, ['note'])}
          ><Icon name="align-{a}" /></button
        >
      {/each}
      <span class="vsep"></span>
      <button class="btn sm ghost" class:on={ed.styleMenu} title="Text style: fonts, outline, shadow… (the collage text styles)" onclick={() => (ed.styleMenu = !ed.styleMenu)}>
        <Icon name="type" size={14} /> Style
      </button>
      <span class="vsep"></span>
    {/if}
    {#if firstSticker && only('sticker')}
      <button class="icon-btn" class:on={!!firstSticker.flipX} title="Flip horizontally" onclick={() => ed.stickerOp('flipX')}><Icon name="flip-h" /></button>
      <button class="icon-btn" class:on={!!firstSticker.flipY} title="Flip vertically" onclick={() => ed.stickerOp('flipY')}><Icon name="flip-v" /></button>
      <button class="icon-btn" title="Turn 90° (or drag the round handle)" onclick={() => ed.stickerOp('turn')}><Icon name="rotate-cw" /></button>
      {#if selItems.some((i) => i.type === 'sticker' && i.rotation)}
        <button class="btn sm ghost" title="Straighten" onclick={() => ed.stickerOp('straighten')}>0°</button>
      {/if}
      <span class="vsep"></span>
    {/if}
    {#if (firstShape && only(firstShape.type)) || (firstShape && selKinds.size === 2 && selKinds.has('rect') && selKinds.has('ellipse'))}
      {@const sh = firstShape!}
      <button class="sw ink" class:on={sh.color === 'ink'} title="Default" style="background:var(--text)" onclick={() => ed.setProp('color', 'ink')} aria-label="Default colour"></button>
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={sh.color === p.key} title={p.label} style="background:{p.ink}" onclick={() => ed.setProp('color', p.key)} aria-label={p.label}></button>
      {/each}
      <span class="vsep"></span>
      <button class="icon-btn" class:on={!!sh.fill || (!!sh.fillColor && sh.fillColor !== 'none')} title="Fill (more colours in Style)" onclick={() => toggleFill(sh)}><Icon name="droplet" /></button>
      <span class="shapemenu">
        <button class="icon-btn" class:on={shapeMenu} title="Change the shape" onclick={() => (shapeMenu = !shapeMenu)}>{@render mini(shapeIdOf(sh), 18)}</button>
      </span>
      <button class="icon-btn" title="Write in the shape (double-click, F2)" onclick={() => ed.startEdit(sh.id)}><Icon name="shapetext" /></button>
      {#if sh.text}
        {#each NOTE_SIZES.slice(0, 4) as s (s.v)}
          <button class="seg" class:on={(sh.size || 24) === s.v} onclick={() => ed.setProp('size', s.v, ['rect', 'ellipse'])}>{s.label}</button>
        {/each}
        <button class="btn sm ghost" class:on={ed.styleMenu} title="Text style: fonts, outline, shadow…" onclick={openText}><Icon name="type" size={14} /> Text</button>
      {/if}
      <span class="vsep"></span>
    {/if}
    {#if firstArrow && only('arrow')}
      {@const a = firstArrow}
      <button class="sw ink" class:on={a.color === 'ink'} title="Default" style="background:var(--text)" onclick={() => ed.setProp('color', 'ink')} aria-label="Default colour"></button>
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={a.color === p.key} title={p.label} style="background:{p.ink}" onclick={() => ed.setProp('color', p.key)} aria-label={p.label}></button>
      {/each}
      {@const h = headsOf(a)}
      {@const r = a.route ?? 'straight'}
      <span class="vsep"></span>
      <button class="icon-btn" class:on={h.start === 'none' && h.end === 'none'} title="Line" onclick={() => heads('none', 'none')}><Icon name="line" /></button>
      <button class="icon-btn" class:on={h.start === 'none' && h.end !== 'none'} title="Arrow" onclick={() => heads('none', 'arrow')}><Icon name="arrow-ur" /></button>
      <button class="icon-btn" class:on={h.start !== 'none' && h.end !== 'none'} title="Arrows at both ends" onclick={() => heads('arrow', 'arrow')}><Icon name="arrow-both" /></button>
      <button class="icon-btn" class:on={(a.dash ?? (a.dashed ? 'dashed' : 'solid')) !== 'solid'} title="Dashed" onclick={() => toggleDash(a)}><Icon name="dash" /></button>
      <span class="vsep"></span>
      <button class="icon-btn" class:on={r === 'straight' && !a.curve} title="Straight" onclick={straightBtn}><Icon name="line" /></button>
      <button class="icon-btn" class:on={r === 'curved' || (r === 'straight' && !!a.curve)} title="Curved (a free arrow: drag the middle handle)" onclick={() => curveBtn(a)}><Icon name="curve" /></button>
      <button class="icon-btn" class:on={r === 'elbow'} title="Elbow: right angles, around the shapes" onclick={() => route('elbow')}><Icon name="elbow" /></button>
      {#if selItems.length === 1}
        <input
          class="input fname"
          placeholder="Label"
          value={a.text ?? ''}
          onfocus={() => ed.before()}
          oninput={(e) => {
            a.text = e.currentTarget.value || undefined;
            ed.change();
          }}
          onkeydown={(e) => e.key === 'Enter' && e.currentTarget.blur()}
        />
      {/if}
      <span class="vsep"></span>
    {/if}
    {#if firstIcon && only('icon')}
      <button class="sw ink" class:on={firstIcon.color === 'ink'} title="Default" style="background:var(--text)" onclick={() => ed.setProp('color', 'ink')} aria-label="Default colour"></button>
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={firstIcon.color === p.key} title={p.label} style="background:{p.ink}" onclick={() => ed.setProp('color', p.key)} aria-label={p.label}></button>
      {/each}
      <span class="vsep"></span>
    {/if}
    {#if selItems.some((i) => i.type === 'path') && only('path')}
      {#each ['red', 'yellow', 'green', 'blue', 'purple', 'ink'] as c (c)}
        <button class="sw" style="background:{ink(c)}" title="Colour" aria-label="Colour" onclick={() => ed.setProp('color', c, ['path'])}></button>
      {/each}
      <span class="vsep"></span>
    {/if}
    {#if selItems.length}
      <button class="icon-btn" class:on={ed.stylePanel} title="Style: colours, line, fill, opacity, arrowheads" onclick={openStyle}><Icon name="palette" /></button>
    {/if}
    {#if ed.canTidy}
      <button class="icon-btn" title="Tidy up top-down (the selected or connected shapes)" onclick={() => ed.tidy('TB')}><Icon name="tidy-tb" /></button>
      <button class="icon-btn" title="Tidy up left to right" onclick={() => ed.tidy('LR')}><Icon name="tidy-lr" /></button>
    {/if}
    {#if selItems.length || ed.canTidy}<span class="vsep"></span>{/if}
    {#if selBoxesCount >= 2}
      <button class="icon-btn" title="Align left" onclick={() => ed.align('left')}><Icon name="align-l" /></button>
      <button class="icon-btn" title="Align centres" onclick={() => ed.align('center')}><Icon name="align-c" /></button>
      <button class="icon-btn" title="Align right" onclick={() => ed.align('right')}><Icon name="align-r" /></button>
      <button class="icon-btn" title="Align top" onclick={() => ed.align('top')}><Icon name="align-t" /></button>
      <button class="icon-btn" title="Align middles" onclick={() => ed.align('middle')}><Icon name="align-m" /></button>
      <button class="icon-btn" title="Align bottom" onclick={() => ed.align('bottom')}><Icon name="align-b" /></button>
      {#if selBoxesCount >= 3}
        <button class="icon-btn" title="Distribute horizontally" onclick={() => ed.distribute('x')}><Icon name="dist-h" /></button>
        <button class="icon-btn" title="Distribute vertically" onclick={() => ed.distribute('y')}><Icon name="dist-v" /></button>
      {/if}
      <span class="vsep"></span>
    {/if}
    {#if ed.selected.size >= 2 && !ed.selGrouped}<button class="icon-btn" title="Group (Ctrl+G)" onclick={() => ed.groupSelection()}><Icon name="group" /></button>{/if}
    {#if ed.selGrouped}<button class="icon-btn on" title="Ungroup (Ctrl+Shift+G)" onclick={() => ed.ungroupSelection()}><Icon name="group" /></button>{/if}
    <button class="icon-btn" class:on={ed.selLocked} title={ed.selLocked ? 'Unlock (Ctrl+Shift+L)' : 'Lock in place (Ctrl+Shift+L)'} onclick={() => ed.toggleLock()}
      ><Icon name={ed.selLocked ? 'lock' : 'unlock'} /></button
    >
    <span class="reveal" title="Canvas presentation: show on the n-th click inside its frame (– = always visible)">
      <Icon name="eye" size={14} />
      <button class="mini" onclick={() => ed.setReveal(Math.max(0, ed.selReveal - 1))} disabled={!ed.selReveal}>−</button>
      <span>{ed.selReveal || '–'}</span>
      <button class="mini" onclick={() => ed.setReveal(ed.selReveal + 1)}>+</button>
    </span>
    <span class="vsep"></span>
    {#if selCards.length || selItems.some((i) => i.type !== 'arrow')}
      <button class="icon-btn" title="Bring to front (Ctrl+])" onclick={() => ed.reorder(true)}><Icon name="arrow-up" /></button>
      <button class="icon-btn" title="Send to back (Ctrl+[)" onclick={() => ed.reorder(false)}><Icon name="arrow-down" /></button>
    {/if}
    <button class="icon-btn" title="Duplicate (Ctrl+D)" onclick={() => ed.duplicateSelection()}><Icon name="copy" /></button>
    <button class="icon-btn danger" title={selCards.length && !selItems.length ? 'Remove from the canvas (Delete) — stays in the story' : 'Delete (Del)'} onclick={() => ed.deleteSelection()}>
      <Icon name={selCards.length && !selItems.length ? 'eye-off' : 'trash'} />
    </button>
  </div>
  {#if shapeMenu && firstShape}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="shapepop" style="left:{ctxPos.x}px;top:{ctxPos.y + 46}px" onpointerdown={(e) => e.stopPropagation()}>
      {#each SHAPES as s (s.id)}
        <button
          class="cell"
          class:on={shapeIdOf(firstShape) === s.id}
          title={s.label}
          onclick={() => {
            ed.changeShape(s.id);
            shapeMenu = false;
          }}>{@render mini(s.id, 24)}</button
        >
      {/each}
    </div>
  {/if}
{/if}

<style>
  .shapepop {
    position: absolute;
    transform: translateX(-50%);
    display: grid;
    grid-template-columns: repeat(6, 38px);
    gap: 3px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 11px;
    box-shadow: var(--shadow);
    z-index: 7;
  }
  .shapepop .cell {
    height: 38px;
    display: grid;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 7px;
    background: var(--surface-2);
    color: var(--text);
    cursor: pointer;
  }
  .shapepop .cell.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .shapemenu .icon-btn {
    color: var(--text);
  }
  .ctx {
    position: absolute;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 5px 6px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 11px;
    box-shadow: var(--shadow);
    z-index: 6;
    white-space: nowrap;
    max-width: calc(100% - 24px);
    overflow-x: auto;
  }
  .ctx .icon-btn.danger:hover {
    color: var(--red);
  }
  .fname {
    height: 28px;
    width: 180px;
  }
  .fsize {
    height: 28px;
    width: 62px;
    margin-left: 4px;
  }
  .step {
    font-size: 12px;
    margin: 0 4px;
  }
  .reveal {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    color: var(--text-2);
    padding: 0 4px;
  }
  .reveal span {
    min-width: 12px;
    text-align: center;
  }
  .mini {
    width: 20px;
    height: 20px;
    border: 1px solid var(--line-strong);
    border-radius: 5px;
    background: var(--surface-2);
    cursor: pointer;
    padding: 0;
    line-height: 1;
  }
</style>
