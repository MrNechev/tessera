<script lang="ts">
  // The stories calendar: a month grid with the stories (and diary days) dated on each day and
  // how many photos and videos the library has from it. Counts come from one grouped query
  // per month (/api/days), so this stays fast with tens of thousands of files.
  import { untrack } from 'svelte';
  import { api } from '../lib/api';
  import { isoDay, monthGrid, parseDay, storiesByDay, tzMinutes, dayBounds } from '../lib/diary';
  import type { DocMeta } from '../lib/types';
  import Icon from './Icon.svelte';

  let { docs, onday }: { docs: DocMeta[]; onday: (iso: string) => void } = $props();

  const today = isoDay(new Date());
  const start = parseDay(today);
  let year = $state(start.getFullYear());
  let month = $state(start.getMonth());
  let counts = $state(new Map<string, number>());
  let loading = $state(false);

  const weeks = $derived(monthGrid(year, month));
  const stories = $derived(storiesByDay(docs));
  const max = $derived(Math.max(1, ...counts.values()));
  const title = $derived(new Date(year, month, 1).toLocaleDateString(undefined, { month: 'long', year: 'numeric' }));
  // Monday-first weekday names in the user's language (2024-01-01 was a Monday)
  const weekdays = Array.from({ length: 7 }, (_, i) => new Date(2024, 0, 1 + i).toLocaleDateString(undefined, { weekday: 'short' }));

  $effect(() => {
    const w = weeks;
    const ac = new AbortController();
    untrack(() => {
      loading = true;
      const { from } = dayBounds(w[0][0]);
      const { to } = dayBounds(w.at(-1)![6]);
      // the offset in the middle of the month: right for every day but the DST switch nights
      api
        .days({ from, to, tz: tzMinutes(new Date(year, month, 15)), kind: 'image,video' }, ac.signal)
        .then((r) => (counts = new Map(r.map((d) => [d.day, d.count]))))
        .catch(() => {})
        .finally(() => (loading = false));
    });
    return () => ac.abort();
  });

  function step(n: number) {
    const d = new Date(year, month + n, 1);
    year = d.getFullYear();
    month = d.getMonth();
  }

  function goToday() {
    year = start.getFullYear();
    month = start.getMonth();
  }

  /** 0..1 on a log scale: one photo is visible, a thousand doesn't drown the rest */
  const heat = (n: number) => (n ? 0.12 + 0.5 * (Math.log(1 + n) / Math.log(1 + max)) : 0);
</script>

<div class="cal">
  <div class="bar">
    <button class="icon-btn" title="Previous month" onclick={() => step(-1)}><Icon name="chev-left" /></button>
    <h2>{title}</h2>
    <button class="icon-btn" title="Next month" onclick={() => step(1)}><Icon name="chev-right" /></button>
    <button class="btn sm" onclick={goToday}>Today</button>
    <span class="grow"></span>
    <span class="muted small">{loading ? 'Counting…' : 'Click a day for its photos and stories'}</span>
  </div>
  <div class="grid" role="grid" aria-label={title}>
    {#each weekdays as w (w)}<div class="wd muted" role="columnheader">{w}</div>{/each}
    {#each weeks as week, wi (wi)}
      {#each week as day (day)}
        {@const n = counts.get(day) ?? 0}
        {@const s = stories.get(day) ?? []}
        {@const out = parseDay(day).getMonth() !== month}
        <button
          class="day"
          class:out
          class:today={day === today}
          class:has={s.length > 0}
          role="gridcell"
          style="--heat:{heat(n)}"
          title="{parseDay(day).toLocaleDateString(undefined, { dateStyle: 'full' })}{n ? ` · ${n} photo${n === 1 ? '' : 's'}/videos` : ''}{s.length ? ` · ${s.map((d) => d.title).join(', ')}` : ''}"
          onclick={() => onday(day)}
        >
          <span class="num">{parseDay(day).getDate()}</span>
          {#if n}<span class="count"><Icon name="image" size={11} /> {n}</span>{/if}
          <span class="stories">
            {#each s.slice(0, 2) as d (d.id)}<span class="story">{d.title || 'Untitled'}</span>{/each}
            {#if s.length > 2}<span class="more muted">+{s.length - 2}</span>{/if}
          </span>
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .cal {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 12px;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
  }
  .bar h2 {
    margin: 0 4px;
    font-size: 17px;
    font-weight: 700;
    min-width: 150px;
    text-align: center;
  }
  .grow {
    flex: 1;
  }
  .small {
    font-size: 12px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, minmax(0, 1fr));
    gap: 4px;
  }
  .wd {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0 6px 2px;
  }
  .day {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    min-height: 86px;
    padding: 6px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: color-mix(in srgb, var(--accent) calc(var(--heat) * 100%), var(--surface-2));
    text-align: left;
    cursor: pointer;
    overflow: hidden;
  }
  .day:hover {
    border-color: var(--accent);
  }
  .day.out {
    opacity: 0.45;
  }
  .day.today {
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .num {
    font-weight: 700;
    font-size: 13px;
  }
  .count {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-2);
  }
  .stories {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    min-width: 0;
  }
  .story {
    font-size: 11px;
    line-height: 1.3;
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--accent-soft);
    color: var(--accent);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    font-size: 11px;
  }
  @media (max-width: 640px) {
    .day {
      min-height: 54px;
    }
    .stories {
      display: none;
    }
  }
</style>
