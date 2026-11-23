<script lang="ts">
  // Document settings: tags, cover image, presentation music and slideshow timing, plus
  // a few whole-document tools (sort by date taken, date/place captions).
  import { onMount } from 'svelte';
  import { api, fmtDate, fmtDuration } from '../lib/api';
  import type { DocState } from '../lib/docstate.svelte';
  import { resolver } from '../lib/resolver.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { FileItem, MediaRef } from '../lib/types';
  import Icon from './Icon.svelte';

  let { ds, onclose }: { ds: DocState; onclose: () => void } = $props();
  const doc = $derived(ds.doc!);

  let tagInput = $state('');
  let allTags = $state<string[]>([]);
  let musicQ = $state('');
  let musicHits = $state<FileItem[]>([]);
  let musicTimer: ReturnType<typeof setTimeout>;

  onMount(() => {
    api.docs().then((ds) => (allTags = [...new Set(ds.flatMap((d) => d.tags ?? []))].sort())).catch(() => {});
    const k = (e: KeyboardEvent) => e.key === 'Escape' && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  const settings = () => (doc.settings ??= {});

  function addTag(t: string) {
    t = t.trim().replace(/,$/, '');
    if (!t) return;
    const tags = doc.tags ?? [];
    if (!tags.some((x) => x.toLowerCase() === t.toLowerCase())) doc.tags = [...tags, t];
    tagInput = '';
    ds.touch();
  }
  function removeTag(t: string) {
    doc.tags = (doc.tags ?? []).filter((x) => x !== t);
    ds.touch();
  }

  // every image/video in the document is a cover candidate
  const candidates = $derived(doc.sections.flatMap((s) => s.media).filter((m, i, a) => a.findIndex((x) => x.path === m.path && x.libraryId === m.libraryId) === i).slice(0, 60));
  const coverKey = $derived(doc.cover ? doc.cover.libraryId + doc.cover.path : '');

  function setCover(m: MediaRef | null) {
    if (m) doc.cover = { libraryId: m.libraryId, path: m.path };
    else delete doc.cover;
    ds.touch();
  }

  $effect(() => {
    const q = musicQ.trim();
    clearTimeout(musicTimer);
    musicTimer = setTimeout(() => {
      api.files({ kind: 'audio', q, limit: 30, sort: 'name' }).then((r) => (musicHits = r.items)).catch(() => {});
    }, 200);
  });

  function setMusic(f: FileItem | null) {
    if (f) settings().music = { libraryId: f.libraryId, path: f.path };
    else delete settings().music;
    ds.touch();
  }

  function num(v: string) {
    const n = parseFloat(v);
    return isFinite(n) && n > 0 ? n : undefined;
  }

  // ---------- tools ----------
  async function sortByDate() {
    const refs = doc.sections.flatMap((s) => s.media);
    const res = await api.resolve(refs.map((m) => ({ libraryId: m.libraryId, path: m.path })));
    const when = new Map(res.map((r) => [r.libraryId + '\u0000' + r.path, r.file ? r.file.taken || r.file.mtime : 0]));
    const date = (s: (typeof doc.sections)[number]) => {
      const t = s.media.map((m) => when.get(m.libraryId + '\u0000' + m.path) || 0).filter(Boolean);
      return t.length ? Math.min(...t) : Infinity;
    };
    if (!confirm('Sort the sections by the date their first photo or video was taken? Sections without media keep their place relative to the one before them.')) return;
    // sections without media stick to the section before them
    const groups: (typeof doc.sections)[] = [];
    for (const s of doc.sections) {
      if (s.media.length || !groups.length) groups.push([s]);
      else groups[groups.length - 1].push(s);
    }
    groups.sort((a, b) => date(a[0]) - date(b[0]));
    doc.sections = groups.flat();
    ds.revision++;
    ds.touch();
    toasts.show('Sections sorted by date taken', 'ok');
  }

  async function addCaptions(kind: 'date' | 'place' | 'both') {
    const refs = doc.sections.flatMap((s) => s.media);
    const res = await api.resolve(refs.map((m) => ({ libraryId: m.libraryId, path: m.path })));
    const by = new Map(res.map((r) => [r.libraryId + '\u0000' + r.path, r.file]));
    let n = 0;
    for (const s of doc.sections)
      for (const m of s.media) {
        if (m.caption) continue;
        const f = by.get(m.libraryId + '\u0000' + m.path);
        if (!f) continue;
        const parts = [];
        if (kind !== 'place' && f.taken) parts.push(new Date(f.taken).toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' }));
        if (kind !== 'date' && f.place) parts.push(f.place);
        if (parts.length) {
          m.caption = parts.join(' · ');
          n++;
        }
      }
    if (n) {
      ds.touch();
      toasts.show(`Added ${n} caption${n === 1 ? '' : 's'}`, 'ok');
    } else toasts.show('No media without a caption has a date/place to use', 'info');
  }

  const rehearsed = $derived(doc.sections.filter((s) => s.slide && s.rehearsal).reduce((a, s) => a + (s.rehearsal ?? 0), 0));
  function clearRehearsal() {
    for (const s of doc.sections) delete s.rehearsal;
    ds.touch();
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onclose}>
  <div class="dialog" onclick={(e) => e.stopPropagation()}>
    <header>
      <h2>Document settings</h2>
      <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
    </header>
    <div class="body">
      <section>
        <h3><Icon name="tag" size={15} /> Tags</h3>
        <div class="tags">
          {#each doc.tags ?? [] as t (t)}
            <span class="chip">{t} <button onclick={() => removeTag(t)} aria-label="Remove tag"><Icon name="x" size={11} /></button></span>
          {/each}
          <input
            class="input tagin"
            list="alltags"
            placeholder="Add a tag…"
            bind:value={tagInput}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ',') { e.preventDefault(); addTag(tagInput); } }}
            onblur={() => addTag(tagInput)}
          />
          <datalist id="alltags">{#each allTags as t (t)}<option value={t}></option>{/each}</datalist>
        </div>
        <p class="muted small">Tags show on the start page, filter the document list and work in the command palette (Ctrl+K).</p>
      </section>

      <section>
        <h3><Icon name="image" size={15} /> Cover</h3>
        <div class="covers">
          <button class="cv auto" class:on={!doc.cover} onclick={() => setCover(null)}>Automatic<br /><span class="muted">first media</span></button>
          {#each candidates as m (m.libraryId + m.path)}
            {@const r = resolver.get(m)}
            <button class="cv" class:on={coverKey === m.libraryId + m.path} onclick={() => setCover(m)} title={m.path}>
              {#if r?.found}<img src="{r.derived}t320.webp" alt="" loading="lazy" />{/if}
            </button>
          {/each}
        </div>
      </section>

      <section>
        <h3><Icon name="present" size={15} /> Presentation</h3>
        <div class="grid2">
          <label>Advance automatically after
            <span class="row"><input class="input num" type="number" min="0" step="1" value={doc.settings?.autoAdvance ?? ''} placeholder="off" onchange={(e) => { const v = num(e.currentTarget.value); if (v) settings().autoAdvance = v; else delete settings().autoAdvance; ds.touch(); }} /> seconds</span>
          </label>
          <label class="cb"><input type="checkbox" checked={!!doc.settings?.advanceOnVideoEnd} onchange={(e) => { settings().advanceOnVideoEnd = e.currentTarget.checked || undefined; ds.touch(); }} /> Next slide when a slide's video ends</label>
          <label class="cb"><input type="checkbox" checked={!!doc.settings?.loop} onchange={(e) => { settings().loop = e.currentTarget.checked || undefined; ds.touch(); }} /> Loop (start over after the last slide)</label>
        </div>
        <p class="muted small">Single slides can have their own time in the section's slide options. Timed slideshows run unattended on a TV.</p>

        <h4><Icon name="music" size={14} /> Background music</h4>
        {#if doc.settings?.music}
          <div class="music">
            <Icon name="music" />
            <span class="p">{doc.settings.music.path}</span>
            <label>volume <input type="range" min="0.05" max="1" step="0.05" value={doc.settings.musicVolume ?? 0.6} oninput={(e) => { settings().musicVolume = Number(e.currentTarget.value); ds.touch(); }} /></label>
            <button class="btn sm" onclick={() => setMusic(null)}>Remove</button>
          </div>
        {/if}
        <input class="input wide" placeholder="Search audio files in the library (mp3, m4a, flac, ogg…)" bind:value={musicQ} />
        <div class="hits">
          {#each musicHits as f (f.id)}
            <button class="hit" onclick={() => setMusic(f)}><Icon name="music" size={13} /> <span>{f.path}</span> <span class="muted">{fmtDuration(f.duration)}</span></button>
          {:else}
            <span class="muted small">No audio files found{musicQ ? ' for this search' : ' in the libraries'}.</span>
          {/each}
        </div>
        <p class="muted small">Music plays across all slides and gets quieter while a video with sound plays.</p>

        {#if rehearsed}
          <h4><Icon name="timer" size={14} /> Rehearsal</h4>
          <p>Last rehearsal: <b>{fmtDuration(rehearsed)}</b> in total. <button class="btn sm" onclick={clearRehearsal}>Clear timings</button></p>
        {/if}
      </section>

      <section>
        <h3><Icon name="calendar" size={15} /> Tools</h3>
        <div class="tools">
          <button class="btn sm" onclick={sortByDate}><Icon name="calendar" size={14} /> Sort sections by date taken</button>
          <button class="btn sm" onclick={() => addCaptions('both')}><Icon name="pin" size={14} /> Add date &amp; place captions</button>
          <button class="btn sm" onclick={() => addCaptions('date')}>Dates only</button>
          <button class="btn sm" onclick={() => addCaptions('place')}>Places only</button>
        </div>
        <p class="muted small">Captions are only added where there is none. Places come from the photos' GPS data (looked up offline). Created {fmtDate(doc.createdAt)}.</p>
      </section>
    </div>
  </div>
</div>

<style>
  section + section {
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid var(--line);
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
    font-size: 14px;
  }
  h4 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 14px 0 6px;
    font-size: 13px;
  }
  .small {
    font-size: 12px;
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .tags .chip button {
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    color: inherit;
    display: grid;
  }
  .tagin {
    width: 180px;
    height: 28px;
  }
  .covers {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(78px, 1fr));
    gap: 6px;
  }
  .cv {
    aspect-ratio: 4 / 3;
    border: 2px solid transparent;
    border-radius: 8px;
    padding: 0;
    overflow: hidden;
    background: var(--bg-2);
    cursor: pointer;
    font-size: 11px;
  }
  .cv img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cv.on {
    border-color: var(--accent);
  }
  .grid2 {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .row {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-left: 8px;
  }
  .num {
    width: 80px;
    height: 28px;
  }
  .cb {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .wide {
    width: 100%;
  }
  .music {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    margin-bottom: 6px;
    border-radius: 8px;
    background: var(--accent-soft);
  }
  .music .p {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .music label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .hits {
    max-height: 160px;
    overflow-y: auto;
    margin-top: 4px;
  }
  .hit {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 6px;
    border: none;
    background: none;
    border-radius: 6px;
    text-align: left;
    cursor: pointer;
    font-size: 12.5px;
  }
  .hit:hover {
    background: var(--surface-2);
  }
  .hit span:nth-child(2) {
    flex: 1;
    overflow-wrap: anywhere;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
</style>
