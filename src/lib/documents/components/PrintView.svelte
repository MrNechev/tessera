<script lang="ts">
  import { mediaStyle } from '../../look/looks.svelte';
  // Print / PDF view: the document as pages, the slides one per 16:9 page, or the canvas
  // frames one per page. "Save as PDF" in the browser's print dialog makes the PDF.
  import { onMount, tick } from 'svelte';
  import { api, thumbUrl, fmtDuration } from '../lib/api';
  import { autoPlace, steps as stepsOf, whenResolved } from '../lib/canvas';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import { isEmptyText, toHTML } from '../lib/tiptap';
  import type { Doc, MediaRef } from '../lib/types';
  import { stickers } from '../../stores/stickers.svelte';
  import { titleCss } from '../../text/styles';
  import CanvasThumb from './CanvasThumb.svelte';
  import Icon from './Icon.svelte';
  import SlideView from './SlideView.svelte';

  let { id, mode }: { id: string; mode: 'doc' | 'slides' | 'canvas' } = $props();
  let doc = $state<Doc | null>(null);
  let ready = $state(false);
  let error = $state('');
  let pageSize = $state<'A4' | 'letter'>('A4');

  onMount(async () => {
    try {
      const d = await api.getDoc(id);
      const refs = d.sections.flatMap((s) => s.media);
      await whenResolved(refs, 5000);
      // canvas stickers need the catalogue (file names) before their images can load
      if (mode === 'canvas') {
        autoPlace(d);
        await stickers.load();
      }
      doc = d;
      document.title = `${d.title} · ${mode === 'doc' ? 'Print' : mode === 'slides' ? 'Slides' : 'Canvas'}`;
      await tick();
      await waitImages();
      // styled titles and canvas text use Tessera's fonts, loaded on first use
      await document.fonts?.ready;
      ready = true;
      setTimeout(() => window.print(), 300);
    } catch (e) {
      error = (e as Error).message;
    }
  });

  async function waitImages() {
    for (let i = 0; i < 3; i++) {
      await new Promise((r) => setTimeout(r, 400));
      const imgs = [...document.querySelectorAll<HTMLImageElement>('.print img')];
      await Promise.all(imgs.map((im) => (im.complete ? null : im.decode().catch(() => null))));
    }
  }

  function r(m: MediaRef) {
    return resolver.get(m);
  }
</script>

<svelte:head>
  {#if mode === 'doc'}
    {@html `<style>@page { size: ${pageSize}; margin: 16mm 14mm; }</style>`}
  {:else}
    {@html `<style>@page { size: 1600px 900px; margin: 0; }</style>`}
  {/if}
</svelte:head>

<div class="bar noprint">
  <button class="btn" onclick={() => router.go(`/doc/${id}`)}><Icon name="chev-left" /> Back</button>
  <b>{mode === 'doc' ? 'Document' : mode === 'slides' ? 'Slides' : 'Canvas frames'} — print or “Save as PDF”</b>
  <span class="grow"></span>
  {#if mode === 'doc'}
    <select class="input" bind:value={pageSize}><option value="A4">A4</option><option value="letter">Letter</option></select>
  {/if}
  <button class="btn primary" onclick={() => window.print()} disabled={!ready}><Icon name="file-pdf" /> {ready ? 'Print / Save as PDF' : 'Preparing…'}</button>
</div>

{#if error}
  <p class="err">{error}</p>
{:else if doc}
  <div class="print {mode}">
    {#if mode === 'doc'}
      <article>
        <h1 class="dtitle">{doc.title || 'Untitled'}</h1>
        {#if doc.tags?.length}<div class="tags">{doc.tags.join(' · ')}</div>{/if}
        {#each doc.sections as s (s.id)}
          <section class:chapter={s.chapter}>
            {#if s.title}<h2 style={titleCss(s.titleStyle)}>{s.title}</h2>{/if}
            {#if s.media.length && s.layout !== 'text-only'}
              <div class="media" class:grid={s.media.length > 1}>
                {#each s.layout === 'compare' ? s.media.slice(0, 2) : s.media as m, j (j)}
                  {@const rr = r(m)}
                  <figure>
                    {#if rr?.found}
                      <img src={thumbUrl(rr.derived, 2560)} alt={m.caption || ''} style={mediaStyle(m)} />
                      {#if rr.file?.kind === 'video'}<span class="vid">▶ video{rr.file.duration ? ` · ${fmtDuration(rr.file.duration)}` : ''}</span>{/if}
                    {:else}
                      <div class="missing">Missing: {m.path}</div>
                    {/if}
                    {#if m.caption}<figcaption>{m.caption}</figcaption>{/if}
                  </figure>
                {/each}
              </div>
            {/if}
            {#if !isEmptyText(s.text)}<div class="prose">{@html toHTML(s.text)}</div>{/if}
          </section>
        {/each}
      </article>
    {:else if mode === 'slides'}
      {#each doc.sections.filter((s) => s.slide) as s (s.id)}
        <div class="page"><SlideView section={s} still /></div>
      {/each}
    {:else}
      {#each stepsOf(doc) as st (st.id)}
        <div class="page canvas"><CanvasThumb {doc} rect={st.rect} /></div>
      {/each}
    {/if}
  </div>
{/if}

<style>
  :global(html:has(.print)),
  :global(body:has(.print)) {
    height: auto;
    overflow: visible;
  }
  :global(#app:has(.print)) {
    height: auto;
  }
  .bar {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 16px;
    background: var(--surface);
    border-bottom: 1px solid var(--line);
  }
  .grow {
    flex: 1;
  }
  .err {
    padding: 30px;
    color: var(--red);
  }
  /* document */
  .print.doc {
    background: #fff;
    color: #1d1b22;
    --text: #1d1b22;
    --text-2: #4f4b59;
    --text-3: #858091;
    --line-strong: #c7c2b5;
    --surface-3: #eee;
    --bg-2: #f5f5f5;
  }
  article {
    max-width: 780px;
    margin: 0 auto;
    padding: 30px 20px;
  }
  .dtitle {
    font: 700 34px/1.15 var(--serif);
    margin: 0 0 6px;
  }
  .tags {
    color: #777;
    font-size: 13px;
    margin-bottom: 18px;
  }
  section {
    margin: 0 0 26px;
  }
  section.chapter h2 {
    font-size: calc(28px * var(--tscale, 1));
    border-bottom: 2px solid #b9791a;
    padding-bottom: 4px;
    break-before: page;
  }
  h2 {
    font: 650 calc(21px * var(--tscale, 1)) / 1.25 var(--serif);
    margin: 0 0 10px;
    break-after: avoid;
  }
  .media {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 10px;
  }
  .media.grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  figure {
    margin: 0;
    position: relative;
    break-inside: avoid;
  }
  figure img {
    width: 100%;
    max-height: 120mm;
    object-fit: contain;
    display: block;
    border-radius: 4px;
  }
  .vid {
    position: absolute;
    top: 8px;
    left: 8px;
    background: #000b;
    color: #fff;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
  }
  figcaption {
    font-style: italic;
    font-size: 12px;
    color: #555;
    text-align: center;
    padding-top: 4px;
  }
  .missing {
    padding: 20px;
    border: 1px dashed #b9791a;
    color: #b9791a;
    font-size: 12px;
  }
  .print.doc :global(.prose) {
    color: #1d1b22;
    font-size: 15px;
  }
  .print.doc :global(.prose iframe) {
    display: none;
  }
  /* slides / canvas: one 16:9 page each */
  .page {
    position: relative;
    width: 1600px;
    height: 900px;
    margin: 20px auto;
    background: #0b0a0f;
    overflow: hidden;
    break-after: page;
    box-shadow: 0 4px 20px #0005;
  }
  .page.canvas {
    background: var(--bg);
  }
  .page :global(.slide) {
    --fs: 26px;
  }
  @media print {
    .noprint {
      display: none !important;
    }
    .page {
      margin: 0;
      box-shadow: none;
    }
    :global(body) {
      background: #fff;
    }
    :global(.toasts) {
      display: none;
    }
  }
</style>
