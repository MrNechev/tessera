<script lang="ts">
  import { thumbUrl } from '../lib/api';
  import { resolver } from '../lib/resolver.svelte';
  import { isEmptyText, toHTML } from '../lib/tiptap';
  import type { Section } from '../lib/types';
  import MediaView from './MediaView.svelte';
  import CompareView from './CompareView.svelte';

  let {
    section,
    active = false,
    sub = 0,
    still = false,
    onended,
  }: {
    section: Section;
    active?: boolean;
    /** carousel position for gallery carousels */
    sub?: number;
    /** still: thumbnails only, no playback (presenter preview) */
    still?: boolean;
    onended?: () => void;
  } = $props();

  const hasText = $derived(!isEmptyText(section.text));
  const carousel = $derived(section.layout === 'gallery' && section.gallery?.mode === 'carousel');
  const media = $derived(carousel ? section.media.slice(sub, sub + 1) : section.media);
  const layout = $derived(section.chapter ? 'chapter' : section.media.length === 0 ? 'text-only' : section.layout);
  const gridCols = $derived(
    layout === 'gallery' && !carousel
      ? section.gallery?.columns || Math.ceil(Math.sqrt(media.length))
      : media.length > 1
        ? Math.min(media.length, 2)
        : 1,
  );
  const gridRows = $derived(Math.ceil(media.length / gridCols));
</script>

<div class="slide layout-{layout}" class:has-title={!!section.title} class:has-text={hasText}>
  {#if section.title && layout !== 'media-full'}
    <h1 class="stitle">{section.title}</h1>
  {/if}
  {#if layout === 'chapter'}
    <div class="chapmark">Chapter</div>
  {:else if layout === 'compare'}
    <div class="smedia cmp">
      {#if still}
        {@const r = resolver.get(section.media[0])}
        {#if r?.found}<img src={thumbUrl(r.derived, 1280)} alt="" style="object-fit:contain;width:100%;height:100%" />{/if}
      {:else}
        <CompareView {section} present fill {active} />
      {/if}
    </div>
  {:else if layout !== 'text-only'}
    <div class="smedia" style="grid-template-columns:repeat({gridCols},minmax(0,1fr));grid-template-rows:repeat({gridRows},minmax(0,1fr))">
      {#each media as m, i (i + m.path)}
        <figure class="cell">
          {#if still}
            {@const r = resolver.get(m)}
            {#if r?.found}
              <img src={thumbUrl(r.derived, 1280)} alt="" style:object-fit={m.fit || 'contain'} />
            {/if}
          {:else}
            <MediaView ref={m} mode="present" fill {active} sizes="100vw" onended={media.length === 1 ? onended : undefined} />
          {/if}
          {#if m.caption}<figcaption>{m.caption}</figcaption>{/if}
        </figure>
      {/each}
    </div>
  {/if}
  {#if hasText || (layout === 'media-full' && section.title)}
    <div class="stext">
      {#if layout === 'media-full' && section.title}<h1 class="stitle">{section.title}</h1>{/if}
      {#if hasText}<div class="prose">{@html toHTML(section.text)}</div>{/if}
    </div>
  {/if}
</div>

<style>
  .slide {
    position: absolute;
    inset: 0;
    display: grid;
    gap: 2.4vmin;
    padding: 4.5vmin 5vmin;
    color: #f2eff7;
    --fs: clamp(15px, 2.35vmin, 44px);
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas: 'title' 'media' 'text';
  }
  .stitle {
    grid-area: title;
    margin: 0;
    font: 650 calc(var(--fs) * 2) / 1.15 var(--serif);
    letter-spacing: -0.01em;
  }
  .smedia {
    grid-area: media;
    display: grid;
    gap: 1.4vmin;
    min-height: 0;
    min-width: 0;
  }
  .cell {
    margin: 0;
    min-height: 0;
    min-width: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }
  .cell > :global(.media),
  .cell > img {
    flex: 1;
    min-height: 0;
  }
  .cell > img {
    width: 100%;
    height: 100%;
    display: block;
  }
  figcaption {
    text-align: center;
    font-style: italic;
    color: #cfcbdb;
    font-size: calc(var(--fs) * 0.8);
    padding-top: 0.6vmin;
  }
  .stext {
    grid-area: text;
    max-height: 32vh;
    overflow: hidden;
  }
  .stext :global(.prose) {
    font-size: var(--fs);
    color: inherit;
  }
  .stext :global(.prose blockquote) {
    color: #cfcbdb;
  }

  .smedia.cmp {
    position: relative;
    display: block;
  }
  /* chapter: a title slide */
  .layout-chapter {
    grid-template-rows: auto auto auto;
    grid-template-areas: 'mark' 'title' 'text';
    align-content: center;
    justify-items: center;
    text-align: center;
    background: radial-gradient(ellipse at center, #e8b04b1f, transparent 70%);
  }
  .chapmark {
    grid-area: mark;
    text-transform: uppercase;
    letter-spacing: 0.3em;
    font-size: calc(var(--fs) * 0.8);
    color: #e8b04b;
  }
  .layout-chapter .stitle {
    font-size: calc(var(--fs) * 3.2);
  }
  .layout-chapter .stext {
    max-width: 60ch;
  }
  /* text only: big centered type */
  .layout-text-only {
    grid-template-rows: auto auto;
    grid-template-areas: 'title' 'text';
    align-content: center;
    justify-items: center;
    padding: 8vmin 10vmin;
  }
  .layout-text-only .stitle {
    font-size: calc(var(--fs) * 2.6);
    text-align: center;
  }
  .layout-text-only .stext {
    max-height: none;
    max-width: 62ch;
    width: 100%;
  }
  .layout-text-only .stext :global(.prose) {
    font-size: calc(var(--fs) * 1.25);
  }

  /* side by side */
  .layout-media-left,
  .layout-media-right {
    grid-template-columns: minmax(0, 1.35fr) minmax(0, 1fr);
    grid-template-rows: auto minmax(0, 1fr);
    grid-template-areas: 'title title' 'media text';
    column-gap: 4vmin;
  }
  .layout-media-right {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.35fr);
    grid-template-areas: 'title title' 'text media';
  }
  .layout-media-left .stext,
  .layout-media-right .stext {
    max-height: none;
    align-self: center;
  }
  .layout-media-left:not(.has-text),
  .layout-media-right:not(.has-text) {
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas: 'title' 'media';
  }

  /* full bleed media, text over a gradient */
  .layout-media-full {
    padding: 0;
    grid-template-rows: minmax(0, 1fr);
    grid-template-areas: 'media';
  }
  .layout-media-full .smedia {
    gap: 0;
  }
  .layout-media-full .stext {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    max-height: 45vh;
    padding: 10vmin 6vmin 5vmin;
    background: linear-gradient(transparent, #000d 55%);
    pointer-events: none;
  }
  /* full-bleed: captions top right, clear of the title/text at the bottom */
  .layout-media-full figcaption {
    position: absolute;
    top: 2.5vmin;
    right: 3vmin;
    left: auto;
    max-width: 50%;
    text-align: right;
    color: #fff;
    text-shadow: 0 1px 4px #000c;
  }
</style>
