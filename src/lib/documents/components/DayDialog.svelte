<script lang="ts">
  // One day: the photos and videos the library has from it, the stories about it, and what to
  // do with them — write the diary entry (with the picked photos), draft a story, make a
  // collage, or browse the day in the Library.
  import { onMount } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import { openLibraryDays } from '../../commands';
  import { api, fmtDuration } from '../lib/api';
  import { dayBounds, findDiary, fmtDayLong, isoDay, pickBest, storiesByDay, takenMs } from '../lib/diary';
  import { collageOf, openDiaryDay, openStoryAt } from '../lib/journal';
  import { toasts } from '../lib/toast.svelte';
  import type { DocMeta, FileItem } from '../lib/types';
  import Icon from './Icon.svelte';
  import StoryStarter from './StoryStarter.svelte';

  let { day, docs, onclose }: { day: string; docs: DocMeta[]; onclose: () => void } = $props();

  const LIMIT = 400;
  let files = $state<FileItem[] | null>(null);
  let total = $state(0);
  let busy = $state(false);
  let starter = $state(false);
  const picked = new SvelteSet<number>();

  const isToday = $derived(day === isoDay(new Date()));
  const stories = $derived(storiesByDay(docs).get(day) ?? []);
  const diary = $derived(findDiary(docs));
  const inDiary = $derived(!!diary?.dates?.includes(day));
  const chosen = $derived((files ?? []).filter((f) => picked.has(f.id)));

  onMount(() => {
    api
      .files({ ...dayBounds(day), kind: 'image,video', sort: 'taken', limit: LIMIT, phash: true })
      .then((r) => {
        files = r.items;
        total = r.total;
        // a new entry starts with a few good, varied photos picked; an existing one with none
        if (!inDiary) for (const f of pickBest(r.items, 6)) picked.add(f.id);
      })
      .catch((e) => {
        files = [];
        toasts.error((e as Error).message);
      });
    const k = (e: KeyboardEvent) => e.key === 'Escape' && !starter && onclose();
    addEventListener('keydown', k);
    return () => removeEventListener('keydown', k);
  });

  async function run(fn: () => Promise<unknown>) {
    if (busy) return;
    busy = true;
    try {
      await fn();
      onclose();
    } catch (e) {
      toasts.error((e as Error).message);
    } finally {
      busy = false;
    }
  }

  function toggle(f: FileItem) {
    if (picked.has(f.id)) picked.delete(f.id);
    else picked.add(f.id);
  }

  const time = (f: FileItem) => (f.taken ? new Date(takenMs(f)).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' }) : '');
</script>

{#if starter}
  <StoryStarter from={day} to={day} onclose={() => (starter = false)} ondone={onclose} />
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="backdrop" onclick={onclose}>
    <div class="dialog dd" onclick={(e) => e.stopPropagation()}>
      <header>
        <h2>{isToday ? 'Today · ' : ''}{fmtDayLong(day)}</h2>
        <button class="icon-btn" onclick={onclose} aria-label="Close"><Icon name="x" /></button>
      </header>
      <div class="body">
        {#if stories.length}
          <div class="stories">
            {#each stories as d (d.id)}
              <button class="chip story" onclick={() => run(() => openStoryAt(d, day))}><Icon name={d.id === diary?.id ? 'calendar' : 'file'} size={12} /> {d.title || 'Untitled'}</button>
            {/each}
          </div>
        {/if}
        {#if files === null}
          <p class="muted">Looking for photos from this day…</p>
        {:else if !files.length}
          <p class="muted">The library has no photos or videos from this day.</p>
        {:else}
          <div class="pickbar">
            <span class="muted">{total} photo{total === 1 ? '' : 's'} and videos{total > files.length ? ` (showing the first ${files.length})` : ''} · {picked.size} picked</span>
            <span class="grow"></span>
            <button class="btn sm ghost" onclick={() => files?.forEach((f) => picked.add(f.id))}>Pick all</button>
            <button class="btn sm ghost" disabled={!picked.size} onclick={() => picked.clear()}>None</button>
          </div>
          <div class="thumbs">
            {#each files as f (f.id)}
              <button class="th" class:on={picked.has(f.id)} title="{f.name}{time(f) ? ' · ' + time(f) : ''}{f.place ? ' · ' + f.place : ''}" onclick={() => toggle(f)}>
                <img src="/d/{f.key}/t320.webp" alt="" loading="lazy" decoding="async" />
                {#if f.kind === 'video'}<span class="badge"><Icon name="video" size={11} /> {fmtDuration(f.duration)}</span>{/if}
                {#if picked.has(f.id)}<span class="tick"><Icon name="check" size={13} /></span>{/if}
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <footer>
        <button class="btn primary" disabled={busy} onclick={() => run(() => openDiaryDay(day, chosen))}>
          <Icon name="notes" size={15} />
          {inDiary ? 'Open the diary entry' : isToday ? "Write today's entry" : 'Write a diary entry'}{chosen.length ? ` with ${chosen.length} photo${chosen.length === 1 ? '' : 's'}` : ''}
        </button>
        <button class="btn" disabled={busy || !files?.length} onclick={() => (starter = true)} title="Chapters, moments, the best photos and captions, drafted for you to edit"><Icon name="chapter" size={15} /> Draft a story</button>
        <button class="btn" disabled={busy || !(chosen.length ? chosen : (files ?? [])).some((f) => f.kind === 'image')} onclick={() => run(() => collageOf((chosen.length ? chosen : files!).slice(0, 60), fmtDayLong(day)))}>
          <Icon name="layout" size={15} /> Collage
        </button>
        <span class="grow"></span>
        <button class="btn ghost" disabled={!files?.length} onclick={() => { openLibraryDays(files![0].libraryId, day); onclose(); }}><Icon name="image" size={15} /> Show in Library</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .dd {
    width: min(920px, 100%);
  }
  .stories {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .story {
    border: none;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .story:hover {
    color: var(--accent);
  }
  .pickbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .grow {
    flex: 1;
  }
  .thumbs {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(110px, 1fr));
    gap: 6px;
  }
  .th {
    position: relative;
    aspect-ratio: 1;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 8px;
    overflow: hidden;
    background: var(--bg-2);
    cursor: pointer;
  }
  .th img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
    opacity: 0.75;
  }
  .th.on {
    border-color: var(--accent);
  }
  .th.on img {
    opacity: 1;
  }
  .badge {
    position: absolute;
    left: 4px;
    bottom: 4px;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 1px 5px;
    border-radius: 4px;
    background: #000a;
    color: #fff;
    font-size: 10.5px;
  }
  .tick {
    position: absolute;
    right: 4px;
    top: 4px;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-ink);
  }
  footer {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 12px 18px;
    border-top: 1px solid var(--line);
  }
</style>
