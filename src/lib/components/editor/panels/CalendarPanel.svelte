<script lang="ts">
  import { CalendarDays, CalendarRange } from '@lucide/svelte';
  import { editor } from '../../../editor/editor.svelte';
  import { createCalendar } from '../../../model/factory';
  import type { CalendarElement, CalendarStyle } from '../../../model/types';
  import { MONTH_NAMES } from '../../../render/calendar';
  import { fontInfo } from '../../../assets/fonts';
  import Segmented from '../../ui/Segmented.svelte';

  const now = new Date();
  let month = $state(now.getMonth() + 1);
  let year = $state(now.getFullYear());
  let firstDay = $state<0 | 1>(1);
  let style = $state<CalendarStyle>('clean');
  let theme = $state(0);

  const THEMES: { name: string; color: string; accent: string; weekend: string; titleFont: string; font: string; bg: string | null }[] = [
    { name: 'Classic', color: '#1d1b26', accent: '#ff5d8f', weekend: '#8a8299', titleFont: 'playfair-display', font: 'inter', bg: null },
    { name: 'Modern', color: '#1d1b26', accent: '#4f8cff', weekend: '#7f7892', titleFont: 'montserrat', font: 'montserrat', bg: null },
    { name: 'Handwritten', color: '#3b3446', accent: '#ff8a5c', weekend: '#a08c7a', titleFont: 'caveat', font: 'inter', bg: null },
    { name: 'Elegant', color: '#2b2340', accent: '#c9a227', weekend: '#8a8299', titleFont: 'cinzel', font: 'lora', bg: null },
    { name: 'Light on dark', color: '#ffffff', accent: '#ffc145', weekend: '#c9c3d6', titleFont: 'bebas-neue', font: 'inter', bg: null },
    { name: 'Card', color: '#1d1b26', accent: '#2dd4bf', weekend: '#8a8299', titleFont: 'poppins', font: 'poppins', bg: '#ffffff' },
  ];

  const years = Array.from({ length: 7 }, (_, i) => now.getFullYear() - 1 + i);

  function make(m: number, y: number, w: number, h: number): CalendarElement {
    const t = THEMES[theme];
    return createCalendar({
      x: 0,
      y: 0,
      w,
      h,
      month: m,
      year: y,
      firstDay,
      style,
      color: t.color,
      accent: t.accent,
      weekendColor: t.weekend,
      titleFont: t.titleFont,
      font: t.font,
      background: t.bg,
      shadow: t.bg ? { color: 'rgba(20,15,30,0.25)', blur: 0.012, x: 0, y: 0.004 } : null,
    });
  }

  function addMonth() {
    const S = editor.pageShort;
    editor.place(make(month, year, S * 0.6, S * 0.5));
  }

  function addYear() {
    const { width: W, height: H } = editor.doc.page;
    const cols = W >= H ? 4 : 3;
    const rows = 12 / cols;
    const m = Math.min(W, H) * 0.05;
    const gap = Math.min(W, H) * 0.03;
    const cw = (W - 2 * m - (cols - 1) * gap) / cols;
    const ch = (H - 2 * m - (rows - 1) * gap) / rows;
    const els: CalendarElement[] = [];
    for (let i = 0; i < 12; i++) {
      const el = make(i + 1, year, cw, ch);
      el.x = m + (i % cols) * (cw + gap);
      el.y = m + Math.floor(i / cols) * (ch + gap);
      el.showYear = false;
      els.push(el);
    }
    editor.addElements(els);
  }
</script>

<div class="head">
  <h3>Calendar</h3>
  <p class="hint">Add a month calendar next to your photos — or all twelve months for a wall calendar. Birthdays and holidays can be marked on each calendar in the Properties panel.</p>
</div>

<div class="body">
  <div class="row">
    <select class="select grow" bind:value={month}>
      {#each MONTH_NAMES as n, i (n)}<option value={i + 1}>{n}</option>{/each}
    </select>
    <select class="select" bind:value={year}>
      {#each years as y (y)}<option value={y}>{y}</option>{/each}
    </select>
  </div>
  <div class="row">
    <span class="label">Week starts on</span>
    <span class="spacer"></span>
    <Segmented full={false} options={[{ value: 1, label: 'Monday' }, { value: 0, label: 'Sunday' }]} value={firstDay} onchange={(v) => (firstDay = v as 0 | 1)} />
  </div>
  <span class="label">Style</span>
  <Segmented
    options={[
      { value: 'clean', label: 'Clean' },
      { value: 'lines', label: 'Lines' },
      { value: 'boxes', label: 'Boxes' },
      { value: 'dots', label: 'Dots' },
    ]}
    value={style}
    onchange={(v) => (style = v as CalendarStyle)}
  />
  <span class="label">Theme</span>
  <div class="themes">
    {#each THEMES as t, i (t.name)}
      <button class="theme" class:on={theme === i} onclick={() => (theme = i)}>
        <span class="t" style:font-family="'{fontInfo(t.titleFont).family}'">{t.name}</span>
        <span class="dot" style:background={t.accent}></span>
      </button>
    {/each}
  </div>
  <button class="btn primary block" onclick={addMonth}><CalendarDays size={16} /> Add {MONTH_NAMES[month - 1]} {year}</button>
  <button class="btn block" onclick={addYear}><CalendarRange size={16} /> Add all 12 months of {year}</button>
  <p class="hint">Tip: the <b>Templates</b> tab has ready-made photo calendars.</p>
</div>

<style>
  .head {
    padding: 16px 14px 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    font-size: 15px;
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 4px 14px 16px;
  }
  .themes {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }
  .theme {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    cursor: pointer;
    font-size: 14px;
    background: var(--panel-2);
    color: var(--text);
  }
  .theme.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  .t {
    flex: 1;
    text-align: left;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }
</style>
