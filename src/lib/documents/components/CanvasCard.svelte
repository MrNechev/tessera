<script lang="ts">
  import BakedImg from '../../look/BakedImg.svelte';
  // A section shown as a card on the canvas. Read-only rendering: editing happens in the
  // section drawer. Level of detail follows the on-screen size: tiny cards show only a
  // thumbnail and the title, media is mounted only while `mounted`, and plays while `playing`.
  import { thumbUrl } from '../lib/api';
  import { CARD_HEAD, CARD_PAD, mediaRatio } from '../lib/canvas';
  import { resolver } from '../lib/resolver.svelte';
  import { isEmptyText, toHTML } from '../lib/tiptap';
  import type { Section } from '../lib/types';
  import MediaView from './MediaView.svelte';
  import RichText from './RichText.svelte';
  import CompareView from './CompareView.svelte';
  import CollageView from './CollageView.svelte';
  import { titleCss } from '../../text/styles';

  let {
    section,
    index,
    lod,
    mounted,
    playing,
    still = false,
    present = false,
    selected = false,
    soundFirst = true,
    concealed = false,
    flags = {},
    editing = false,
    ontext,
  }: {
    section: Section;
    index: number;
    /** settled zoom (screen px per world px) */
    lod: number;
    mounted: boolean;
    playing: boolean;
    /** thumbnails only (speaker-notes previews) */
    still?: boolean;
    present?: boolean;
    selected?: boolean;
    /** canvas presentation: whether this card's first video may play with sound */
    soundFirst?: boolean;
    concealed?: boolean;
    flags?: { reveal?: number; locked?: boolean };
    /** text edited in place on the canvas */
    editing?: boolean;
    ontext?: (field: 'title' | 'text', value: any) => void;
  } = $props();

  const c = $derived(section.canvas!);
  const screenW = $derived(c.w * lod);
  const tiny = $derived(!editing && (screenW < 150 || c.h * lod < 70));
  // a live collage has no media of its own
  const layout = $derived(
    section.layout === 'collage' ? 'collage' : section.media.length === 0 ? 'text-only' : section.layout === 'compare' ? 'media-top' : section.layout,
  );
  const compare = $derived(section.layout === 'compare' && section.media.length >= 2);
  const hasText = $derived(!isEmptyText(section.text));
  const carousel = $derived(layout === 'gallery' && section.gallery?.mode === 'carousel');
  const media = $derived(carousel ? section.media.slice(0, 1) : section.media);
  const cols = $derived(
    layout === 'gallery' && !carousel
      ? section.gallery?.columns || Math.ceil(Math.sqrt(media.length))
      : layout === 'media-top' && media.length > 1
        ? 2
        : 1,
  );
  const rows = $derived(Math.ceil(media.length / cols));
  const head = $derived((section.title || editing) && layout !== 'media-full' ? CARD_HEAD : 0);
  const side = $derived((layout === 'media-left' || layout === 'media-right') && hasText);

  // media box height: natural height for the width, but leave room for some text
  const mediaH = $derived.by(() => {
    if (layout === 'text-only') return 0;
    if (layout === 'media-full') return c.h;
    const inner = c.h - head - 2 * CARD_PAD;
    if (side) return inner;
    const mw = c.w - 2 * CARD_PAD;
    const natural = layout === 'collage' ? mw / (section.collage?.aspect || 4 / 3) : media.length > 1 || (layout === 'gallery' && !carousel) ? ((mw / cols) * 0.75) * rows : mw / mediaRatio(media[0]);
    const room = hasText ? inner - Math.min(90, inner * 0.3) : inner;
    return Math.max(24, Math.min(natural, room));
  });

  // srcset hint: the cell's width on screen, bucketed so zooming doesn't thrash requests
  const sizes = $derived.by(() => {
    const w = ((side ? c.w * 0.56 : c.w) / cols) * lod;
    const b = w <= 340 ? 320 : w <= 700 ? 640 : w <= 1300 ? 1280 : 2560;
    return `${b}px`;
  });

  const firstThumb = $derived.by(() => {
    const m = section.media[0];
    if (!m) return '';
    const r = resolver.get(m);
    return r?.found ? thumbUrl(r.derived, 320) : '';
  });
</script>

<div
  class="card layout-{layout}"
  class:sel={selected}
  class:present
  class:tiny
  class:concealed
  class:editing
  class:chapter={section.chapter}
  data-cid={section.id}
  data-kind="card"
  style="left:{c.x}px;top:{c.y}px;width:{c.w}px;height:{c.h}px"
>
  {#if tiny}
    {#if firstThumb}<img class="tthumb" src={firstThumb} alt="" draggable="false" />{/if}
    <div class="ttitle">{section.title || `Section ${index + 1}`}</div>
  {:else}
    {#if editing}
      <input class="ctitle edit" value={section.title ?? ''} placeholder="Title" oninput={(e) => ontext?.('title', e.currentTarget.value)} />
    {:else if head}<div class="ctitle" style={titleCss(section.titleStyle)}>{section.title}</div>{/if}
    <div class="cbody" class:side class:rev={layout === 'media-right'}>
      {#if compare && !still && !tiny}
        <div class="cmedia" style="height:{mediaH}px">
          <CompareView {section} present={present} active={playing} fill />
        </div>
      {:else if layout === 'collage'}
        <div class="cmedia" style="height:{mediaH}px">
          {#if mounted || still}<CollageView ref={section.collage} fill zoom={lod} {still} />{/if}
        </div>
      {:else if layout !== 'text-only'}
        <div
          class="cmedia"
          style="height:{mediaH}px;grid-template-columns:repeat({cols},minmax(0,1fr));grid-template-rows:repeat({rows},minmax(0,1fr))"
        >
          {#each media as m, i (i + m.path)}
            <div class="cell">
              {#if still}
                {@const r = resolver.get(m)}
                {#if r?.found}<BakedImg src={thumbUrl(r.derived, 1280)} look={m} maxW={1280} style="object-fit:{m.fit || (cols > 1 ? 'cover' : 'contain')}" />{/if}
              {:else}
                <MediaView
                  ref={m.fit || cols === 1 ? m : { ...m, fit: 'cover' }}
                  mode={present ? 'present' : 'canvas'}
                  fill
                  near={mounted}
                  active={playing}
                  controls={present || selected}
                  forceMute={present && !(soundFirst && i === 0)}
                  instance="canvas"
                  {sizes}
                />
              {/if}
            </div>
          {/each}
          {#if carousel && section.media.length > 1}<span class="badge">1 / {section.media.length}</span>{/if}
        </div>
      {/if}
      {#if editing}
        <div class="ctext edit">
          <RichText content={section.text} onchange={(j) => ontext?.('text', j)} placeholder="Write something…" />
        </div>
      {:else if hasText && layout !== 'media-full'}
        <div class="ctext prose">{@html toHTML(section.text)}</div>
      {/if}
    </div>
    {#if layout === 'media-full' && section.title}<div class="overtitle" style={titleCss(section.titleStyle)}>{section.title}</div>{/if}
  {/if}
  {#if flags.reveal || flags.locked}<span class="flag">{#if flags.locked}🔒{/if}{#if flags.reveal}{flags.reveal}{/if}</span>{/if}
</div>

<style>
  .card {
    position: absolute;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: 12px;
    box-shadow: 0 6px 24px #0003;
    overflow: hidden;
    contain: strict;
    user-select: none;
  }
  .card:not(.present) :global(*) {
    pointer-events: none;
  }
  .card.editing .edit,
  .card.editing .edit :global(*) {
    pointer-events: auto;
  }
  .card.editing {
    box-shadow: 0 0 0 3px var(--accent);
    overflow: visible;
    contain: layout style;
  }
  .ctitle.edit {
    border: none;
    outline: none;
    background: var(--bg-2);
    border-radius: 6px;
    margin: 6px 8px 0;
    padding: 6px;
    width: calc(100% - 16px);
    color: var(--text);
  }
  .ctext.edit {
    overflow: auto;
    mask-image: none;
    font-size: 16px;
  }
  .ctext.edit :global(.toolbar) {
    transform-origin: 0 0;
  }
  .card.chapter {
    border-color: var(--accent);
    background: linear-gradient(var(--accent-soft), transparent 60%), var(--surface);
  }
  .card.concealed {
    opacity: 0;
  }
  .card {
    transition: opacity 0.35s;
  }
  .flag {
    position: absolute;
    top: 6px;
    right: 6px;
    min-width: 22px;
    height: 22px;
    padding: 0 5px;
    border-radius: 11px;
    background: var(--accent);
    color: var(--accent-ink);
    font: 700 12px/22px var(--ui);
    text-align: center;
  }
  .card.sel :global(video) {
    pointer-events: auto;
  }
  .ctitle {
    flex: none;
    height: 46px;
    padding: 12px 14px 0;
    font: 650 calc(20px * var(--tscale, 1))/1.2 var(--serif);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cbody {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
  }
  .ctitle + .cbody {
    padding-top: 6px;
  }
  .cbody.side {
    flex-direction: row;
  }
  .cbody.side.rev {
    flex-direction: row-reverse;
  }
  .cbody.side .cmedia {
    width: 56%;
    flex: none;
  }
  .cmedia {
    position: relative;
    flex: none;
    display: grid;
    gap: 6px;
    min-height: 0;
  }
  .cell {
    position: relative;
    min-width: 0;
    min-height: 0;
    border-radius: 6px;
    overflow: hidden;
    background: var(--bg-2);
  }
  .cell > :global(.media) {
    position: absolute;
    inset: 0;
  }
  .cell > :global(img) {
    width: 100%;
    height: 100%;
    display: block;
  }
  .ctext {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    font-size: 16px;
    line-height: 1.6;
    mask-image: linear-gradient(#000 calc(100% - 28px), transparent);
  }
  .layout-text-only .ctext {
    mask-image: linear-gradient(#000 calc(100% - 20px), transparent);
  }
  .layout-media-full .cbody {
    padding: 0;
  }
  .layout-media-full .cell {
    border-radius: 0;
    background: #000;
  }
  .overtitle {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 40px 16px 12px;
    font: 650 calc(22px * var(--tscale, 1))/1.2 var(--serif);
    color: #fff;
    background: linear-gradient(transparent, #000c);
  }
  .badge {
    position: absolute;
    right: 8px;
    bottom: 8px;
    font-size: 12px;
    padding: 1px 8px;
    border-radius: 999px;
    background: #000a;
    color: #fff;
  }
  /* tiny (far zoomed out): just a thumbnail and the title, nothing to decode */
  .card.tiny {
    background: var(--surface-2);
  }
  .tthumb {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.85;
  }
  .ttitle {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    padding: 6% 8%;
    font: 650 clamp(20px, 9cqw, 400px) / 1.15 var(--serif);
    color: var(--text);
    overflow: hidden;
    max-height: 100%;
  }
  .card.tiny:has(.tthumb) .ttitle {
    color: #fff;
    background: linear-gradient(transparent, #000b);
  }
  .card.tiny {
    container-type: size;
  }
</style>
