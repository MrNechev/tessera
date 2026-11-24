<script lang="ts">
  import { untrack } from 'svelte';
  import type { DocState } from '../lib/docstate.svelte';
  import { dnd, hasMedia, readDrop } from '../lib/dnd.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { isEmptyText, toHTML } from '../lib/tiptap';
  import type { Layout, MediaRef, Section } from '../lib/types';
  import Icon from './Icon.svelte';
  import MediaView from './MediaView.svelte';
  import RichText from './RichText.svelte';
  import ClipTimeline from './ClipTimeline.svelte';
  import CompareView from './CompareView.svelte';
  import PhotoDialog, { type PhotoEdit } from './PhotoDialog.svelte';
  import { hasLook } from '../../look/looks.svelte';
  import { toasts } from '../lib/toast.svelte';

  let {
    section,
    index,
    total,
    ds,
    onrelink,
    onpresent,
  }: {
    section: Section;
    index: number;
    total: number;
    ds: DocState;
    onrelink: (sectionId: string, mediaIndex: number) => void;
    onpresent: (index: number) => void;
  } = $props();

  const LAYOUTS: { id: Layout; label: string; icon: string }[] = [
    { id: 'media-top', label: 'Media above text', icon: 'layout' },
    { id: 'media-left', label: 'Media left, text right', icon: 'panel-left' },
    { id: 'media-right', label: 'Text left, media right', icon: 'panel-right' },
    { id: 'media-full', label: 'Full-width media', icon: 'maximize' },
    { id: 'gallery', label: 'Gallery', icon: 'grid' },
    { id: 'compare', label: 'Compare two (before/after slider, or side by side in sync)', icon: 'compare' },
    { id: 'text-only', label: 'Text only', icon: 'notes' },
  ];

  let menu = $state(false);
  let optsFor = $state<number | null>(null);
  let dropping = $state(false);
  let showNotes = $state(untrack(() => !!section.notes));
  let carousel = $state(0);
  let cropFor = $state<number | null>(null);
  const isCompare = $derived(section.layout === 'compare');
  const chapterCount = $derived.by(() => {
    if (!section.chapter || !ds.doc) return 0;
    let n = 0;
    for (let i = index + 1; i < ds.doc.sections.length && !ds.doc.sections[i].chapter; i++) n++;
    return n;
  });

  function gps() {
    for (const m of section.media) {
      const f = resolver.cache.get(m.libraryId + '\u0000' + m.path)?.file;
      if (f?.hasGps && f.lat !== undefined && f.lon !== undefined) return { lat: f.lat, lon: f.lon, label: f.place ?? '' };
    }
    return null;
  }

  function setCover(m: MediaRef) {
    if (!ds.doc) return;
    ds.doc.cover = { libraryId: m.libraryId, path: m.path };
    optsFor = null;
    touch();
    toasts.show('Set as the document cover', 'ok');
  }

  function metaCaption(i: number) {
    const f = resolver.cache.get(section.media[i].libraryId + '\u0000' + section.media[i].path)?.file;
    if (!f) return;
    const parts = [];
    if (f.taken) parts.push(new Date(f.taken).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' }));
    if (f.place) parts.push(f.place);
    if (!parts.length) return toasts.show('This file has no date taken or location', 'info');
    setOpt(i, 'caption', parts.join(' · '));
  }

  function toggleChapter() {
    section.chapter = !section.chapter || undefined;
    if (section.chapter && !section.media.length) section.layout = 'text-only';
    menu = false;
    touch();
  }

  function slideTiming() {
    menu = false;
    const v = prompt('Seconds on screen in timed slideshows (empty = use the document setting)', section.advance ? String(section.advance) : '');
    if (v === null) return;
    const n = parseFloat(v);
    if (isFinite(n) && n > 0) section.advance = n;
    else delete section.advance;
    touch();
  }

  const active = $derived(ds.activeSection === section.id);
  const cols = $derived(section.gallery?.columns || (section.media.length >= 6 ? 3 : 2));
  const isCarousel = $derived(section.layout === 'gallery' && section.gallery?.mode === 'carousel');
  const sizes = $derived(
    section.layout === 'media-left' || section.layout === 'media-right'
      ? '(max-width: 1100px) 50vw, 520px'
      : section.layout === 'gallery' && !isCarousel
        ? `(max-width: 1100px) ${Math.round(100 / cols)}vw, ${Math.round(1000 / cols)}px`
        : section.layout === 'media-full'
          ? '(max-width: 1300px) 100vw, 1240px'
          : '(max-width: 1000px) 100vw, 920px',
  );

  function touch() {
    ds.touch();
  }

  function setLayout(l: Layout) {
    section.layout = l;
    if (l === 'gallery' && !section.gallery) section.gallery = { mode: 'grid', columns: 0 };
    if (l === 'compare' && !section.compare) section.compare = { mode: 'slider', pos: 0.5 };
    touch();
  }

  function removeMedia(i: number) {
    section.media.splice(i, 1);
    optsFor = null;
    if (carousel >= section.media.length) carousel = Math.max(0, section.media.length - 1);
    touch();
  }

  function moveMedia(i: number, d: number) {
    const j = i + d;
    if (j < 0 || j >= section.media.length) return;
    const [m] = section.media.splice(i, 1);
    section.media.splice(j, 0, m);
    optsFor = j;
    touch();
  }

  function relinkTo(i: number, to: { libraryId: string; path: string }) {
    section.media[i].libraryId = to.libraryId;
    section.media[i].path = to.path;
    resolver.request(to);
    touch();
  }

  function onDragOver(e: DragEvent) {
    if (!hasMedia(e)) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer!.dropEffect = 'copy';
    dropping = true;
  }

  function onDrop(e: DragEvent) {
    dropping = false;
    const items = readDrop(e);
    if (!items) return;
    e.preventDefault();
    e.stopPropagation();
    dnd.dragging = false;
    ds.addMedia(section.id, items.map((x) => ({ libraryId: x.libraryId, path: x.path })));
  }

  function num(v: string) {
    const n = parseFloat(v);
    return isFinite(n) && n > 0 ? n : undefined;
  }

  function applyEdit(i: number, e: PhotoEdit) {
    const m = section.media[i];
    for (const k of ['crop', 'adjust', 'filter', 'flipX', 'flipY'] as const) {
      if (e[k] === undefined) delete m[k];
      else (m as any)[k] = e[k];
    }
    touch();
  }

  function setOpt<K extends keyof MediaRef>(i: number, k: K, v: MediaRef[K]) {
    const m = section.media[i];
    if (v === undefined || v === false || v === '') delete m[k];
    else m[k] = v;
    touch();
  }

  const excerpt = $derived(
    section.collapsed ? toHTML(section.text).replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 140) : '',
  );
</script>

<section
  class="section layout-{section.layout}"
  class:chapter={section.chapter}
  class:active
  class:collapsed={section.collapsed}
  class:noslide={!section.slide}
  data-section-id={section.id}
>
  <header class="sh">
    <span class="drag-handle" title="Drag to reorder"><Icon name="grip" /></span>
    <span class="num">{index + 1}</span>
    {#if section.chapter}<span class="chap" title="Chapter: groups the sections below it until the next chapter"><Icon name="chapter" size={14} /> Chapter</span>{/if}
    <input
      class="title"
      placeholder={section.chapter ? 'Chapter title' : 'Section title'}
      bind:value={section.title}
      oninput={touch}
      onfocus={() => (ds.activeSection = section.id)}
    />
    <div class="tools">
      {#each section.chapter ? [] : LAYOUTS as l (l.id)}
        <button class="icon-btn" class:on={section.layout === l.id} title={l.label} onclick={() => setLayout(l.id)}><Icon name={l.icon} size={16} /></button>
      {/each}
      <span class="sep"></span>
      <button
        class="icon-btn"
        class:on={section.slide}
        title={section.slide ? 'Included in the presentation (click to exclude)' : 'Not in the presentation (click to include)'}
        onclick={() => { section.slide = !section.slide; touch(); }}><Icon name={section.slide ? 'eye' : 'eye-off'} size={16} /></button
      >
      <button class="icon-btn" class:on={showNotes} title="Speaker notes" onclick={() => (showNotes = !showNotes)}><Icon name="notes" size={16} /></button>
      <button class="icon-btn" title="Present from here" onclick={() => onpresent(index)}><Icon name="present" size={16} /></button>
      <button class="icon-btn" title={section.collapsed ? 'Expand' : 'Collapse'} onclick={() => { section.collapsed = !section.collapsed; touch(); }}>
        <Icon name={section.collapsed ? 'expand' : 'collapse'} size={16} />
      </button>
      <div class="menuwrap">
        <button class="icon-btn" title="More" onclick={() => (menu = !menu)}><Icon name="more" size={16} /></button>
        {#if menu}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <div class="menu-backdrop" onclick={() => (menu = false)}></div>
          <div class="menu">
            <button disabled={index === 0} onclick={() => { ds.moveSection(index, index - 1); menu = false; }}><Icon name="arrow-up" size={15} /> Move up <span class="kbd">Alt ↑</span></button>
            <button disabled={index === total - 1} onclick={() => { ds.moveSection(index, index + 1); menu = false; }}><Icon name="arrow-down" size={15} /> Move down <span class="kbd">Alt ↓</span></button>
            <button onclick={() => { ds.addSection(index + 1); menu = false; }}><Icon name="plus" size={15} /> Add section below</button>
            <button onclick={toggleChapter}><Icon name="chapter" size={15} /> {section.chapter ? 'Make a normal section' : 'Make this a chapter'}</button>
            <button onclick={slideTiming}><Icon name="timer" size={15} /> Slide timing{section.advance ? ` (${section.advance} s)` : '…'}</button>
            <button onclick={() => { ds.duplicateSection(section.id); menu = false; }}><Icon name="copy" size={15} /> Duplicate</button>
            <button class="danger" onclick={() => { if (confirm('Delete this section?')) ds.removeSection(section.id); menu = false; }}><Icon name="trash" size={15} /> Delete</button>
          </div>
        {/if}
      </div>
    </div>
  </header>

  {#if section.collapsed}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="summary muted" onclick={() => { section.collapsed = false; touch(); }}>
      {#if section.chapter}{chapterCount} section{chapterCount === 1 ? '' : 's'} in this chapter — click to expand{:else}{section.media.length} media{excerpt ? ' · ' + excerpt : ''}{/if}
    </div>
  {:else}
    {#if isCompare}
      <div class="gallery-opts">
        <button class="seg" class:on={(section.compare?.mode ?? 'slider') === 'slider'} onclick={() => { section.compare = { ...section.compare, mode: 'slider' }; touch(); }}>Before/after slider</button>
        <button class="seg" class:on={section.compare?.mode === 'sync'} onclick={() => { section.compare = { ...section.compare, mode: 'sync' }; touch(); }}>Side by side, in sync</button>
        <span class="muted">uses the first two media</span>
      </div>
      <div class="cmpwrap">
        <CompareView {section} onpos={(p) => { section.compare = { ...section.compare, pos: p }; touch(); }} />
      </div>
    {/if}
    {#if section.layout === 'gallery'}
      <div class="gallery-opts">
        <button class="seg" class:on={!isCarousel} onclick={() => { section.gallery = { ...section.gallery, mode: 'grid' }; touch(); }}>Grid</button>
        <button class="seg" class:on={isCarousel} onclick={() => { section.gallery = { ...section.gallery, mode: 'carousel' }; touch(); }}>Carousel</button>
        {#if !isCarousel}
          <span class="muted">columns</span>
          {#each [2, 3, 4] as n (n)}
            <button class="seg" class:on={cols === n} onclick={() => { section.gallery = { ...section.gallery, columns: n }; touch(); }}>{n}</button>
          {/each}
        {/if}
      </div>
    {/if}

    <div class="body">
      {#if section.layout !== 'text-only'}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="mediazone"
          class:dropping
          class:dragging={dnd.dragging}
          class:carousel={isCarousel}
          class:compact={isCompare}
          style={section.layout === 'gallery' && !isCarousel ? `grid-template-columns:repeat(${cols},1fr)` : ''}
          ondragover={onDragOver}
          ondragleave={() => (dropping = false)}
          ondrop={onDrop}
        >
          {#each section.media as m, i (i + '|' + m.libraryId + '|' + m.path)}
            {#if !isCarousel || i === carousel}
              <figure class="item" class:multi={section.media.length > 1 && section.layout !== 'gallery'}>
                <div class="mwrap">
                  <MediaView
                    ref={m}
                    {sizes}
                    instance={section.id + ':' + i}
                    onrelink={() => onrelink(section.id, i)}
                    relinkTo={(to) => relinkTo(i, to)}
                    onremove={() => removeMedia(i)}
                  />
                  <div class="mtools">
                    <button class="icon-btn" title="Media options" onclick={() => (optsFor = optsFor === i ? null : i)}><Icon name="settings" size={15} /></button>
                    <button class="icon-btn" title="Remove" onclick={() => removeMedia(i)}><Icon name="x" size={15} /></button>
                  </div>
                  {#if isCarousel && section.media.length > 1}
                    <button class="nav prev" onclick={() => (carousel = (carousel - 1 + section.media.length) % section.media.length)} aria-label="Previous"><Icon name="chev-left" /></button>
                    <button class="nav next" onclick={() => (carousel = (carousel + 1) % section.media.length)} aria-label="Next"><Icon name="chev-right" /></button>
                  {/if}
                </div>
                {#if optsFor === i}
                  {@const r = resolver.cache.get(m.libraryId + '\u0000' + m.path)}
                  <div class="opts">
                    <div class="path" title={m.path}>{m.libraryId}:{m.path}</div>
                    {#if r?.file?.kind === 'video' && r.file.duration > 0 && (r.doc?.url || r.present?.url)}
                      <ClipTimeline file={r.file} src={(r.doc?.url || r.present?.url)!} startAt={m.startAt} endAt={m.endAt} onchange={(a, b) => { setOpt(i, 'startAt', a); setOpt(i, 'endAt', b); }} />
                    {/if}
                    {#if r?.file?.kind === 'video'}
                      <label>Start at (s) <input class="input" type="number" min="0" step="0.1" value={m.startAt ?? ''} onchange={(e) => setOpt(i, 'startAt', num(e.currentTarget.value))} /></label>
                      <label>End at (s) <input class="input" type="number" min="0" step="0.1" value={m.endAt ?? ''} onchange={(e) => setOpt(i, 'endAt', num(e.currentTarget.value))} /></label>
                      <label class="cb"><input type="checkbox" checked={!!m.muted} onchange={(e) => setOpt(i, 'muted', e.currentTarget.checked)} /> Muted</label>
                      <label class="cb"><input type="checkbox" checked={!!m.loop} onchange={(e) => setOpt(i, 'loop', e.currentTarget.checked)} /> Loop</label>
                      <label class="cb" title="In the document: plays muted while visible. In presentations videos always start automatically."><input type="checkbox" checked={!!m.autoplay} onchange={(e) => setOpt(i, 'autoplay', e.currentTarget.checked)} /> Autoplay in document</label>
                    {/if}
                    <label>Fit in slides
                      <select class="input" value={m.fit ?? 'contain'} onchange={(e) => setOpt(i, 'fit', e.currentTarget.value === 'contain' ? undefined : (e.currentTarget.value as 'cover'))}>
                        <option value="contain">Contain (whole image)</option>
                        <option value="cover">Cover (fill, crop edges)</option>
                      </select>
                    </label>
                    {#if r?.file && (r.file.taken || r.file.place || r.file.camera)}
                      <div class="meta muted">
                        {#if r.file.taken}<span><Icon name="calendar" size={12} /> {new Date(r.file.taken).toLocaleString()}</span>{/if}
                        {#if r.file.place}<span><Icon name="pin" size={12} /> {r.file.place}</span>{/if}
                        {#if r.file.camera}<span>{r.file.camera}</span>{/if}
                      </div>
                    {/if}
                    <div class="row">
                      {#if r?.file && r.file.kind !== 'audio'}<button class="btn sm" onclick={() => (cropFor = i)}><Icon name="sliders" size={14} /> {m.crop || hasLook(m) ? 'Edit photo…' : 'Crop, filters…'}</button>{/if}
                      {#if m.crop || hasLook(m)}<button class="btn sm ghost" onclick={() => applyEdit(i, {})}>Undo edits</button>{/if}
                      <button class="btn sm" onclick={() => setCover(m)}><Icon name="image" size={14} /> Set as cover</button>
                      {#if r?.file?.taken || r?.file?.place}<button class="btn sm" onclick={() => metaCaption(i)}><Icon name="pin" size={14} /> Date/place caption</button>{/if}
                    </div>
                    <div class="row">
                      <button class="btn sm" disabled={i === 0} onclick={() => moveMedia(i, -1)}><Icon name="chev-left" size={14} /> Move</button>
                      <button class="btn sm" disabled={i === section.media.length - 1} onclick={() => moveMedia(i, 1)}>Move <Icon name="chev-right" size={14} /></button>
                      <button class="btn sm" onclick={() => { onrelink(section.id, i); optsFor = null; }}><Icon name="link" size={14} /> Replace…</button>
                      <button class="btn sm danger" onclick={() => removeMedia(i)}><Icon name="trash" size={14} /> Remove</button>
                    </div>
                  </div>
                {/if}
                <input
                  class="caption"
                  placeholder="Add a caption"
                  value={m.caption ?? ''}
                  oninput={(e) => setOpt(i, 'caption', e.currentTarget.value)}
                  onfocus={() => (ds.activeSection = section.id)}
                />
              </figure>
            {/if}
          {/each}
          {#if isCarousel && section.media.length > 1}
            <div class="dots">
              {#each section.media as _, i (i)}<button class:on={i === carousel} onclick={() => (carousel = i)} aria-label="Item {i + 1}"></button>{/each}
            </div>
          {/if}
          {#if section.media.length === 0}
            <div class="empty-media">
              <Icon name="image" size={22} />
              <span>Drop images or videos here, or click items in the library while this section is selected.</span>
            </div>
          {/if}
        </div>
      {/if}

      {#if section.layout !== 'media-full' || !isEmptyText(section.text) || active}
        <div class="text">
          <RichText
            {gps}
            content={section.text}
            onchange={(j) => { section.text = j; touch(); }}
            onfocus={() => (ds.activeSection = section.id)}
            placeholder={section.layout === 'text-only' ? 'Write something…' : 'Write about this…'}
          />
        </div>
      {/if}
    </div>

    {#if showNotes}
      <div class="notes">
        <label for="notes-{section.id}"><Icon name="notes" size={14} /> Speaker notes <span class="muted">(only visible in the presenter view)</span></label>
        <textarea id="notes-{section.id}" class="input" rows="3" bind:value={section.notes} oninput={touch} placeholder="Notes for yourself while presenting…"></textarea>
      </div>
    {/if}
  {/if}
</section>

{#if cropFor !== null}
  {@const m = section.media[cropFor]}
  {@const r = m ? resolver.cache.get(m.libraryId + '\u0000' + m.path) : undefined}
  {#if m && r?.file}
    <PhotoDialog file={r.file} mref={m} onclose={() => (cropFor = null)} onsave={(e) => { applyEdit(cropFor!, e); cropFor = null; }} />
  {/if}
{/if}

<style>
  .section.chapter {
    background: linear-gradient(var(--accent-soft), transparent 70%), var(--surface);
    border-color: var(--accent);
  }
  .section.chapter .title {
    font-size: 26px;
  }
  .chap {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
    white-space: nowrap;
  }
  .cmpwrap {
    margin-bottom: 10px;
  }
  .mediazone.compact {
    display: grid !important;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px;
  }
  .opts .meta {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 12px;
  }
  .section {
    position: relative;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 10px 18px 18px;
    content-visibility: auto;
    contain-intrinsic-size: auto 600px;
    transition: border-color 0.15s;
  }
  .section.active {
    border-color: var(--line-strong);
    box-shadow: 0 0 0 1px var(--accent-soft);
  }
  .section.noslide {
    border-style: dashed;
  }
  .sh {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 -8px 8px;
  }
  .drag-handle {
    cursor: grab;
    color: var(--text-3);
    display: grid;
    place-items: center;
    width: 22px;
    height: 30px;
    border-radius: 6px;
  }
  .drag-handle:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .num {
    font: 600 12px var(--mono);
    color: var(--text-3);
    min-width: 18px;
  }
  .title {
    flex: 1;
    min-width: 80px;
    border: none;
    background: none;
    outline: none;
    font: 650 19px var(--serif);
    padding: 4px 0;
  }
  .title::placeholder {
    color: var(--text-3);
    font-weight: 500;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 1px;
    opacity: 0.35;
    transition: opacity 0.15s;
  }
  .section:hover .tools,
  .section.active .tools {
    opacity: 1;
  }
  .tools .icon-btn {
    width: 28px;
    height: 28px;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: var(--line-strong);
    margin: 0 4px;
  }
  .menuwrap {
    position: relative;
  }
  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9;
  }
  .menu {
    position: absolute;
    right: 0;
    top: 32px;
    z-index: 10;
    min-width: 210px;
    padding: 4px;
    background: var(--surface-2);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    box-shadow: var(--shadow);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    background: none;
    border-radius: 6px;
    cursor: pointer;
    text-align: left;
  }
  .menu button:hover:not(:disabled) {
    background: var(--surface-3);
  }
  .menu button:disabled {
    opacity: 0.4;
  }
  .menu .kbd {
    margin-left: auto;
  }
  .menu .danger {
    color: var(--red);
  }
  .summary {
    cursor: pointer;
    padding: 0 4px;
    font-size: 13px;
  }
  .collapsed {
    padding-bottom: 10px;
  }
  .gallery-opts {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-bottom: 10px;
    font-size: 12px;
  }
  .seg {
    height: 24px;
    padding: 0 9px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: none;
    color: var(--text-2);
    cursor: pointer;
    font-size: 12px;
  }
  .seg.on {
    background: var(--surface-3);
    color: var(--text);
    border-color: var(--line-strong);
  }
  .body {
    display: grid;
    gap: 16px;
  }
  .layout-media-left .body {
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 1fr);
    align-items: start;
  }
  .layout-media-right .body {
    grid-template-columns: minmax(0, 1fr) minmax(0, 1.15fr);
    align-items: start;
  }
  .layout-media-right .mediazone {
    order: 2;
  }
  .layout-media-full .mediazone {
    margin: 0 -18px;
  }
  .layout-media-full .mediazone :global(.media) {
    border-radius: 0;
  }
  @media (max-width: 900px) {
    .layout-media-left .body,
    .layout-media-right .body {
      grid-template-columns: 1fr;
    }
  }
  .mediazone {
    display: grid;
    gap: 10px;
    border-radius: 10px;
    min-width: 0;
    position: relative;
  }
  .mediazone.dragging {
    outline: 2px dashed var(--line-strong);
    outline-offset: 4px;
  }
  .mediazone.dropping {
    outline: 2px dashed var(--accent);
    background: var(--accent-soft);
  }
  .layout-media-top .mediazone:has(.multi),
  .layout-media-full .mediazone:has(.multi) {
    grid-template-columns: repeat(2, 1fr);
  }
  .item {
    margin: 0;
    min-width: 0;
  }
  .mwrap {
    position: relative;
  }
  .layout-gallery .mediazone:not(.carousel) :global(.media) {
    aspect-ratio: 1 !important;
  }
  .mtools {
    position: absolute;
    top: 6px;
    right: 6px;
    display: flex;
    gap: 2px;
    opacity: 0;
    transition: opacity 0.12s;
    background: #000a;
    border-radius: 8px;
  }
  .mtools .icon-btn {
    color: #fff;
    width: 28px;
    height: 28px;
  }
  .mtools .icon-btn:hover {
    background: #fff2;
  }
  .mwrap:hover .mtools {
    opacity: 1;
  }
  .nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: none;
    background: #000a;
    color: #fff;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .nav.prev {
    left: 8px;
  }
  .nav.next {
    right: 8px;
  }
  .dots {
    display: flex;
    gap: 6px;
    justify-content: center;
  }
  .dots button {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: none;
    background: var(--line-strong);
    cursor: pointer;
    padding: 0;
  }
  .dots button.on {
    background: var(--accent);
  }
  .caption {
    width: 100%;
    border: none;
    background: none;
    outline: none;
    font-size: 13px;
    color: var(--text-2);
    padding: 6px 2px 0;
    text-align: center;
    font-style: italic;
  }
  .caption:placeholder-shown {
    opacity: 0;
  }
  .item:hover .caption:placeholder-shown,
  .caption:focus {
    opacity: 1;
  }
  .opts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 8px 12px;
    padding: 12px;
    margin-top: 8px;
    background: var(--bg-2);
    border: 1px solid var(--line);
    border-radius: 10px;
    font-size: 12.5px;
  }
  .opts label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    color: var(--text-2);
  }
  .opts label.cb {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .opts .input {
    height: 28px;
  }
  .opts .path {
    grid-column: 1 / -1;
    font: 11.5px var(--mono);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .opts .row {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .empty-media {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 22px;
    border: 1.5px dashed var(--line-strong);
    border-radius: 10px;
    color: var(--text-3);
    font-size: 13px;
  }
  .text {
    min-width: 0;
  }
  .notes {
    margin-top: 14px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .notes label {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
</style>
