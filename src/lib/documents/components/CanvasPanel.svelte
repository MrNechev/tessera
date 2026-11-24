<script lang="ts">
  // Side panel of the canvas: frames (presentation steps, drag to reorder, notes) and
  // the document's sections as cards (show/hide on the canvas, jump to them).
  import Sortable from 'sortablejs';
  import { ensureCanvas } from '../lib/canvas';
  import type { DocState } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { toHTML } from '../lib/tiptap';
  import type { FrameItem, Rect, Section } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    ds,
    selected,
    hiddenCount,
    onbefore,
    onzoom,
    onselect,
    onpresent,
    onaddframe,
    oneditcard,
  }: {
    ds: DocState;
    selected: Set<string>;
    hiddenCount: number;
    onbefore: () => void;
    onzoom: (r: Rect) => void;
    onselect: (id: string) => void;
    onpresent: (step: number) => void;
    onaddframe: () => void;
    oneditcard: (id: string) => void;
  } = $props();

  let tab = $state<'frames' | 'cards'>('frames');
  let list = $state<HTMLDivElement>();
  let openNotes = $state<string | null>(null);

  const doc = $derived(ds.doc!);
  const frames = $derived((doc.canvas?.items ?? []).filter((i) => i.type === 'frame') as FrameItem[]);

  // drag to reorder frames = presentation order (frames keep their slots among the other items)
  $effect(() => {
    const el = list;
    if (!el || tab !== 'frames') return;
    let next: Node | null = null;
    const s = Sortable.create(el, {
      animation: 150,
      draggable: '.fr',
      handle: '.grip',
      ghostClass: 'ghost',
      onStart: (e) => (next = e.item.nextSibling),
      onEnd: (e) => {
        e.from.insertBefore(e.item, next);
        const a = e.oldDraggableIndex,
          b = e.newDraggableIndex;
        if (a === undefined || b === undefined || a === b) return;
        onbefore();
        const c = ensureCanvas(doc);
        const slots = c.items.map((it, i) => (it.type === 'frame' ? i : -1)).filter((i) => i >= 0);
        const fr = slots.map((i) => c.items[i]);
        const [m] = fr.splice(a, 1);
        fr.splice(b, 0, m);
        const items = [...c.items];
        slots.forEach((slot, k) => (items[slot] = fr[k]));
        c.items = items;
        ds.touch();
      },
    });
    return () => s.destroy();
  });

  function label(s: Section, i: number) {
    if (s.title) return s.title;
    const t = toHTML(s.text).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim();
    if (t) return t.slice(0, 60);
    if (s.media.length) return s.media[0].path.split('/').pop()!;
    return `Section ${i + 1}`;
  }

  function thumb(s: Section) {
    const m = s.media[0];
    if (!m) return '';
    const r = resolver.cache.get(m.libraryId + '\u0000' + m.path);
    return r?.found ? `${r.derived}t320.webp` : '';
  }

  function toggleHidden(s: Section) {
    if (!s.canvas) return;
    onbefore();
    if (s.canvas.hidden) {
      delete s.canvas.hidden;
      onzoom(s.canvas);
      onselect(s.id);
    } else s.canvas.hidden = true;
    ds.touch();
  }
</script>

<aside class="cp">
  <div class="tabs">
    <button class:on={tab === 'frames'} onclick={() => (tab = 'frames')}>Frames <span class="muted">{frames.length}</span></button>
    <button class:on={tab === 'cards'} onclick={() => (tab = 'cards')}>Cards <span class="muted">{doc.sections.length}{hiddenCount ? ` · ${hiddenCount} hidden` : ''}</span></button>
  </div>

  {#if tab === 'frames'}
    <p class="hint muted">
      {#if frames.length}Frames are the steps of the canvas presentation, in this order. Drag to reorder.{:else}No frames yet: the presentation steps through the cards. Draw frames
        (<span class="kbd">F</span>) around what each step should show.{/if}
    </p>
    <div class="list" bind:this={list}>
      {#each frames as f, i (f.id)}
        <div class="fr" class:sel={selected.has(f.id)}>
          <div class="row">
            <span class="grip" title="Drag to reorder"><Icon name="grip" size={14} /></span>
            <span class="n">{i + 1}</span>
            <input
              class="ft"
              value={f.title}
              placeholder="Frame {i + 1}"
              onfocus={onbefore}
              oninput={(e) => { f.title = e.currentTarget.value; ds.touch(); }}
            />
            <button class="icon-btn" title="Zoom to it" onclick={() => { onselect(f.id); onzoom(f); }}><Icon name="maximize" size={15} /></button>
            <button class="icon-btn" class:on={openNotes === f.id || !!f.notes} title="Speaker notes" onclick={() => (openNotes = openNotes === f.id ? null : f.id)}><Icon name="notes" size={15} /></button>
            <button class="icon-btn" title="Present from here" onclick={() => onpresent(i)}><Icon name="present" size={15} /></button>
          </div>
          {#if openNotes === f.id}
            <textarea
              class="input notes"
              rows="4"
              placeholder="Speaker notes for this step"
              value={f.notes ?? ''}
              onfocus={onbefore}
              oninput={(e) => { f.notes = e.currentTarget.value || undefined; ds.touch(); }}
            ></textarea>
          {/if}
        </div>
      {/each}
    </div>
    <button class="btn sm add" onclick={onaddframe}><Icon name="frame" size={14} /> Frame around the view</button>
  {:else}
    <p class="hint muted">Every section of the story is a card. Hidden cards stay in the document.</p>
    <div class="list">
      {#each doc.sections as s, i (s.id)}
        {@const t = thumb(s)}
        <div class="cr" class:sel={selected.has(s.id)} class:hidden={s.canvas?.hidden}>
          <button class="cl" onclick={() => { if (s.canvas && !s.canvas.hidden) { onselect(s.id); onzoom(s.canvas); } }} ondblclick={() => oneditcard(s.id)}>
            <span class="n">{i + 1}</span>
            {#if t}<img src={t} alt="" loading="lazy" />{:else}<span class="noimg"><Icon name={s.media.length ? 'image' : 'notes'} size={13} /></span>{/if}
            <span class="l">{label(s, i)}</span>
          </button>
          <button class="icon-btn" title={s.canvas?.hidden ? 'Show on the canvas' : 'Hide from the canvas'} onclick={() => toggleHidden(s)}>
            <Icon name={s.canvas?.hidden ? 'eye-off' : 'eye'} size={15} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</aside>

<style>
  .cp {
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-left: 1px solid var(--line);
    background: var(--bg);
  }
  .tabs {
    display: flex;
    border-bottom: 1px solid var(--line);
  }
  .tabs button {
    flex: 1;
    padding: 10px 6px;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-2);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .tabs .muted {
    font-weight: 400;
    font-size: 12px;
  }
  .hint {
    font-size: 12px;
    margin: 10px 12px 6px;
    line-height: 1.45;
  }
  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 6px;
  }
  .fr {
    border-radius: 8px;
    padding: 3px 4px;
  }
  .fr.sel,
  .cr.sel {
    background: var(--accent-soft);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .row .icon-btn {
    width: 26px;
    height: 26px;
    flex: none;
  }
  .grip {
    cursor: grab;
    color: var(--text-3);
    display: grid;
    place-items: center;
    width: 16px;
    flex: none;
  }
  .n {
    font: 11px var(--mono);
    color: var(--text-3);
    min-width: 18px;
    text-align: right;
    flex: none;
  }
  .ft {
    flex: 1;
    min-width: 0;
    height: 26px;
    border: 1px solid transparent;
    border-radius: 5px;
    background: none;
    padding: 0 6px;
    font-size: 13px;
    outline: none;
  }
  .ft:hover,
  .ft:focus {
    border-color: var(--line-strong);
    background: var(--bg-2);
  }
  .notes {
    width: 100%;
    margin: 4px 0 6px;
    font-size: 12.5px;
  }
  :global(.cp .ghost) {
    opacity: 0.35;
  }
  .add {
    margin: 8px 12px 12px;
    align-self: flex-start;
  }
  .cr {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .cr.hidden .cl {
    opacity: 0.45;
  }
  .cl {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 4px 6px;
    background: none;
    border: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
    font-size: 13px;
  }
  .cl:hover {
    background: var(--surface-2);
  }
  .cl img,
  .noimg {
    width: 34px;
    height: 24px;
    border-radius: 4px;
    object-fit: cover;
    flex: none;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    color: var(--text-3);
  }
  .l {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
