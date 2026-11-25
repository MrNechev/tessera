<script lang="ts">
  /**
   * The story editor's right-hand side panel. "Photo": everything for the
   * photo or video picked in the story (look, crop & frame, caption, clip
   * range, cover, cut-out, move/replace/remove). "Outline": the sections,
   * drag to reorder.
   */
  import type { DocState } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import type { MediaRef } from '../lib/types';
  import type { PhotoAdjust } from '../../model/types';
  import { toasts } from '../lib/toast.svelte';
  import { hasLook } from '../../look/looks.svelte';
  import LookPanel from '../../look/LookPanel.svelte';
  import { openCutout } from '../../cutout/open.svelte';
  import ClipTimeline from './ClipTimeline.svelte';
  import PhotoDialog, { type PhotoEdit } from './PhotoDialog.svelte';
  import Outline from './Outline.svelte';
  import Icon from './Icon.svelte';

  let {
    ds,
    tab = $bindable('photo'),
    onjump,
    onrelink,
  }: {
    ds: DocState;
    tab?: 'photo' | 'outline';
    onjump: (sectionId: string) => void;
    onrelink: (sectionId: string, mediaIndex: number) => void;
  } = $props();

  const sel = $derived(ds.activeMedia);
  const sectionIndex = $derived(sel && ds.doc ? ds.doc.sections.findIndex((s) => s.id === sel.sectionId) : -1);
  const section = $derived(sectionIndex >= 0 ? ds.doc!.sections[sectionIndex] : undefined);
  const m = $derived(section && sel ? section.media[sel.index] : undefined);
  const r = $derived(m ? resolver.cache.get(m.libraryId + '\u0000' + m.path) : undefined);
  const f = $derived(r?.file);
  let editing = $state(false);

  // picking a photo in the story shows its options
  $effect(() => {
    if (sel) tab = 'photo';
  });

  const EDIT_KEYS = ['crop', 'adjust', 'filter', 'flipX', 'flipY', 'rotate', 'straighten', 'hideAreas', 'mask', 'frame', 'frameColor', 'shadow', 'border'] as const;

  function touch() {
    ds.touch();
  }

  function setOpt<K extends keyof MediaRef>(k: K, v: MediaRef[K]) {
    if (!m) return;
    if (v === undefined || v === false || v === '') delete m[k];
    else m[k] = v;
    touch();
  }

  function num(v: string) {
    const n = parseFloat(v);
    return isFinite(n) && n > 0 ? n : undefined;
  }

  function applyEdit(e: PhotoEdit) {
    if (!m) return;
    for (const k of EDIT_KEYS) {
      if (e[k] === undefined) delete m[k];
      else (m as any)[k] = e[k];
    }
    touch();
  }

  function setLook(adjust: PhotoAdjust | null, filter: string | null) {
    if (!m) return;
    if (adjust) m.adjust = adjust;
    else delete m.adjust;
    if (filter) m.filter = filter;
    else delete m.filter;
    touch();
  }

  function remove() {
    if (!section || !sel) return;
    section.media.splice(sel.index, 1);
    ds.activeMedia = null;
    touch();
  }

  function move(d: number) {
    if (!section || !sel) return;
    const j = sel.index + d;
    if (j < 0 || j >= section.media.length) return;
    const [x] = section.media.splice(sel.index, 1);
    section.media.splice(j, 0, x);
    ds.activeMedia = { sectionId: section.id, index: j };
    touch();
  }

  function setCover() {
    if (!ds.doc || !m) return;
    ds.doc.cover = { libraryId: m.libraryId, path: m.path };
    touch();
    toasts.show('Set as the story cover', 'ok');
  }

  function metaCaption() {
    if (!f) return;
    const parts = [];
    if (f.taken) parts.push(new Date(f.taken).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' }));
    if (f.place) parts.push(f.place);
    if (!parts.length) return toasts.show('This file has no date taken or location', 'info');
    setOpt('caption', parts.join(' · '));
  }

  /** Remove the photo's background; the transparent PNG (in Outputs) replaces it or follows it. */
  function cutOut() {
    if (!m || !f || !section) return;
    const ref = m;
    const sec = section;
    openCutout({
      source: { libraryId: ref.libraryId, path: ref.path, name: f.name },
      actions: [
        { id: 'insert', label: 'Add after it' },
        { id: 'replace', label: 'Replace with the cut-out' },
      ],
      onsaved: (res, action) => {
        const at = sec.media.indexOf(ref);
        if (at < 0) return;
        const next: MediaRef = { ...res.ref, ...(ref.caption ? { caption: ref.caption } : {}), ...(ref.fit ? { fit: ref.fit } : {}) };
        if (action === 'replace') sec.media[at] = next;
        else sec.media.splice(at + 1, 0, next);
        ds.activeMedia = { sectionId: sec.id, index: action === 'replace' ? at : at + 1 };
        touch();
      },
    });
  }
</script>

<div class="side">
  <div class="tabs" role="tablist">
    <button role="tab" aria-selected={tab === 'photo'} class:on={tab === 'photo'} onclick={() => (tab = 'photo')}><Icon name="sliders" size={14} /> Photo</button>
    <button role="tab" aria-selected={tab === 'outline'} class:on={tab === 'outline'} onclick={() => (tab = 'outline')}><Icon name="outline" size={14} /> Outline</button>
  </div>

  {#if tab === 'outline'}
    <div class="outline-wrap"><Outline {ds} {onjump} /></div>
  {:else if !m || !section || !sel}
    <div class="empty muted">
      <Icon name="image" size={26} />
      <p>Click a photo or video in the story to edit it here: filters, light and colour, crop and frame, caption, clip range.</p>
    </div>
  {:else}
    <div class="body">
      <div class="head">
        {#if f && f.kind !== 'audio'}<img class="thumb" src="/d/{f.key}/t320.webp" alt="" draggable="false" />{/if}
        <div class="who">
          <div class="name" title={m.path}>{f?.name ?? m.path.split('/').pop()}</div>
          <button class="link muted" onclick={() => onjump(section.id)}>Section {sectionIndex + 1}{section.title ? ` · ${section.title}` : ''}{section.media.length > 1 ? ` · ${sel.index + 1} of ${section.media.length}` : ''}</button>
        </div>
        <button class="icon-btn" title="Done" onclick={() => (ds.activeMedia = null)}><Icon name="x" size={15} /></button>
      </div>

      <input class="input" placeholder="Caption" value={m.caption ?? ''} oninput={(e) => setOpt('caption', e.currentTarget.value)} />

      {#if f && f.kind !== 'audio'}
        <div class="row">
          <button class="btn sm primary" onclick={() => (editing = true)}><Icon name="crop" size={14} /> Crop, turn, frame…</button>
          {#if f.kind === 'image'}<button class="btn sm" title="Remove the background (on this device)" onclick={cutOut}><Icon name="scissors" size={14} /> Cut out</button>{/if}
          {#if m.crop || hasLook(m)}<button class="btn sm ghost" onclick={() => applyEdit({})}>Undo all edits</button>{/if}
        </div>
        <LookPanel adjust={m.adjust} filter={m.filter} thumb="/d/{f.key}/t320.webp" video={f.kind !== 'image'} onchange={(a, fl) => setLook(a, fl)} />
      {/if}

      {#if f?.kind === 'video'}
        <div class="group">Clip</div>
        {#if f.duration > 0 && (r?.doc?.url || r?.present?.url)}
          <ClipTimeline file={f} src={(r!.doc?.url || r!.present?.url)!} startAt={m.startAt} endAt={m.endAt} onchange={(a, b) => { setOpt('startAt', a); setOpt('endAt', b); }} />
        {/if}
        <div class="two">
          <label>Start at (s) <input class="input" type="number" min="0" step="0.1" value={m.startAt ?? ''} onchange={(e) => setOpt('startAt', num(e.currentTarget.value))} /></label>
          <label>End at (s) <input class="input" type="number" min="0" step="0.1" value={m.endAt ?? ''} onchange={(e) => setOpt('endAt', num(e.currentTarget.value))} /></label>
        </div>
        <label class="cb"><input type="checkbox" checked={!!m.muted} onchange={(e) => setOpt('muted', e.currentTarget.checked)} /> Muted</label>
        <label class="cb"><input type="checkbox" checked={!!m.loop} onchange={(e) => setOpt('loop', e.currentTarget.checked)} /> Loop</label>
        <label class="cb" title="In the story: plays muted while visible. In presentations videos always start automatically."><input type="checkbox" checked={!!m.autoplay} onchange={(e) => setOpt('autoplay', e.currentTarget.checked)} /> Autoplay in story</label>
      {/if}

      <div class="group">In slides</div>
      <select class="input" value={m.fit ?? 'contain'} onchange={(e) => setOpt('fit', e.currentTarget.value === 'contain' ? undefined : (e.currentTarget.value as 'cover'))}>
        <option value="contain">Contain (whole image)</option>
        <option value="cover">Cover (fill, crop edges)</option>
      </select>

      {#if f && (f.taken || f.place || f.camera)}
        <div class="meta muted">
          {#if f.taken}<span><Icon name="calendar" size={12} /> {new Date(f.taken).toLocaleString()}</span>{/if}
          {#if f.place}<span><Icon name="pin" size={12} /> {f.place}</span>{/if}
          {#if f.camera}<span>{f.camera}</span>{/if}
        </div>
      {/if}

      <div class="group">This {f?.kind === 'video' ? 'video' : 'photo'}</div>
      <div class="row">
        <button class="btn sm" onclick={setCover}><Icon name="image" size={14} /> Set as cover</button>
        {#if f?.taken || f?.place}<button class="btn sm" onclick={metaCaption}><Icon name="pin" size={14} /> Date/place caption</button>{/if}
      </div>
      <div class="row">
        <button class="btn sm" disabled={sel.index === 0} onclick={() => move(-1)} title="Move before the previous one"><Icon name="chev-left" size={14} /></button>
        <button class="btn sm" disabled={sel.index === section.media.length - 1} onclick={() => move(1)} title="Move after the next one"><Icon name="chev-right" size={14} /></button>
        <button class="btn sm" onclick={() => onrelink(section.id, sel.index)}><Icon name="link" size={14} /> Replace…</button>
        <button class="btn sm danger" onclick={remove}><Icon name="trash" size={14} /> Remove</button>
      </div>
      <div class="path" title={m.path}>{m.libraryId}:{m.path}</div>
    </div>
  {/if}
</div>

{#if editing && m && f}
  <PhotoDialog
    src="/d/{f.key}/t1280.webp"
    width={f.width}
    height={f.height}
    value={m}
    mode="story"
    kind={f.kind === 'image' ? 'image' : 'video'}
    onclose={() => (editing = false)}
    onsave={(e) => {
      applyEdit(e);
      editing = false;
    }}
  />
{/if}

<style>
  .side {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-2);
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 8px 10px;
    padding: 2px;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--line);
    flex: none;
  }
  .tabs button {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 28px;
    border: none;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 12.5px;
    cursor: pointer;
  }
  .tabs button.on {
    background: var(--surface-3);
    color: var(--text);
    font-weight: 600;
  }
  .outline-wrap {
    flex: 1;
    min-height: 0;
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 40px 20px;
    text-align: center;
    font-size: 13px;
  }
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 2px 12px 24px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12.5px;
  }
  /* a scrolling flex column: keep every row at its own height */
  .body > :global(*) {
    flex-shrink: 0;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .thumb {
    width: 52px;
    height: 52px;
    object-fit: cover;
    border-radius: 8px;
    flex: none;
    background: var(--surface);
  }
  .who {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .link {
    border: none;
    background: none;
    padding: 0;
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .link:hover {
    color: var(--accent);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .group {
    margin-top: 6px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    color: var(--text-2);
  }
  label.cb {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }
  .input {
    height: 30px;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    font-size: 12px;
  }
  .path {
    margin-top: 4px;
    font: 11px var(--mono);
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
