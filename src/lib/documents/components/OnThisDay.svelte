<script lang="ts">
  // "On this day": photos and videos taken on today's date in earlier years, and stories dated
  // on it. Hidden when there is nothing to show. Used on the Stories page and on Home.
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { dayBounds, isoDay, takenMs, tzMinutes } from '../lib/diary';
  import { openStoryAt } from '../lib/journal';
  import type { DocMeta, FileItem } from '../lib/types';
  import DayDialog from './DayDialog.svelte';
  import Icon from './Icon.svelte';

  /** the stories (loaded here when the host has none) */
  let { docs: given = null }: { docs?: DocMeta[] | null } = $props();

  const PER_YEAR = 8;
  const today = isoDay(new Date());
  const md = today.slice(5);
  const year = Number(today.slice(0, 4));
  let loaded = $state<DocMeta[] | null>(null);
  let files = $state<FileItem[]>([]);
  /** files per earlier year (exact, from /api/days; the thumbnails are only a sample) */
  let totals = $state(new Map<number, number>());
  let open = $state<string | null>(null);

  const docs = $derived(given ?? loaded ?? []);
  const years = $derived.by(() => {
    const m = new Map<number, { day: string; files: FileItem[]; total: number; stories: { d: DocMeta; day: string }[] }>();
    const at = (y: number) => {
      if (!m.has(y)) m.set(y, { day: `${y}-${md}`, files: [], total: 0, stories: [] });
      return m.get(y)!;
    };
    for (const f of files) {
      const y = new Date(takenMs(f)).getFullYear();
      const g = at(y);
      g.total = totals.get(y) ?? g.total + 1;
      if (g.files.length < PER_YEAR) g.files.push(f);
    }
    for (const d of docs)
      for (const day of d.dates ?? [])
        if (day.slice(5) === md && day < today) at(Number(day.slice(0, 4))).stories.push({ d, day });
    return [...m.entries()].sort((a, b) => b[0] - a[0]);
  });

  onMount(() => {
    if (!given) api.docs().then((d) => (loaded = d)).catch(() => {});
    void load().catch(() => {});
  });

  async function load() {
    // earlier years only: everything with today's month and day, taken before today
    const q = { md, tz: tzMinutes(), to: dayBounds(today).from, kind: 'image,video' };
    const [days, r] = await Promise.all([api.days(q), api.files({ ...q, sort: 'taken', desc: true, limit: 200 })]);
    totals = new Map(days.map((d) => [Number(d.day.slice(0, 4)), d.count]));
    files = r.items;
    // a busy recent year can fill the whole sample; fetch a few photos of the older years too
    const seen = new Set(r.items.map((f) => isoDay(takenMs(f))));
    const missing = days.filter((d) => !seen.has(d.day)).slice(-12);
    if (!missing.length) return;
    const more = await Promise.all(
      missing.map((d) => api.files({ ...dayBounds(d.day), kind: 'image,video', sort: 'taken', limit: PER_YEAR }).then((x) => x.items).catch(() => [])),
    );
    files = [...r.items, ...more.flat()];
  }

  const ago = (y: number) => (year - y === 1 ? 'A year ago' : `${year - y} years ago`);
</script>

{#if years.length}
  <section class="otd">
    <div class="head">
      <Icon name="calendar" size={16} />
      <h3>On this day</h3>
      <span class="muted">{new Date().toLocaleDateString(undefined, { day: 'numeric', month: 'long' })} in earlier years</span>
    </div>
    <div class="strip">
      {#each years as [y, g] (y)}
        <div class="year">
          <button class="label" onclick={() => (open = g.day)} title="Photos and stories of {g.day}">
            <b>{ago(y)}</b> <span class="muted">{y}{g.total ? ` · ${g.total}` : ''}</span>
          </button>
          <div class="row">
            {#each g.stories as s (s.d.id)}
              <button class="story" onclick={() => openStoryAt(s.d, s.day)} title="Open the story"><Icon name="file" size={14} /> <span>{s.d.title || 'Untitled'}</span></button>
            {/each}
            {#each g.files as f (f.id)}
              <button class="th" onclick={() => (open = isoDay(takenMs(f)))} title={f.place || f.name}>
                <img src="/d/{f.key}/t320.webp" alt="" loading="lazy" decoding="async" />
                {#if f.kind === 'video'}<span class="vid"><Icon name="play" size={10} /></span>{/if}
              </button>
            {/each}
            {#if g.total > g.files.length}<button class="th more" onclick={() => (open = g.day)}>+{g.total - g.files.length}</button>{/if}
          </div>
        </div>
      {/each}
    </div>
  </section>
{/if}

{#if open}<DayDialog day={open} {docs} onclose={() => (open = null)} />{/if}

<style>
  .otd {
    margin-bottom: 20px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
    color: var(--text-2);
  }
  .head h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 700;
    color: var(--text);
  }
  .head .muted {
    font-size: 12.5px;
  }
  .strip {
    display: flex;
    gap: 18px;
    overflow-x: auto;
    padding-bottom: 6px;
  }
  .year {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .label {
    align-self: flex-start;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    font-size: 12.5px;
  }
  .label:hover b {
    color: var(--accent);
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .th {
    position: relative;
    flex: none;
    width: 92px;
    height: 92px;
    padding: 0;
    border: 1px solid var(--line);
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
    transition: transform 0.15s;
  }
  .th:hover img {
    transform: scale(1.05);
  }
  .th.more {
    font-weight: 700;
    color: var(--text-2);
  }
  .vid {
    position: absolute;
    left: 4px;
    bottom: 4px;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #000a;
    color: #fff;
  }
  .story {
    flex: none;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: center;
    gap: 4px;
    width: 120px;
    height: 92px;
    padding: 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--accent-soft);
    color: var(--accent);
    cursor: pointer;
    font-size: 12px;
    text-align: center;
  }
  .story span {
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    line-clamp: 3;
    -webkit-box-orient: vertical;
  }
</style>
