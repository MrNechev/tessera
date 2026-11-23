<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';
  import { autoPlace, steps as stepsOf, whenResolved } from '../lib/canvas';
  import type { Doc, MediaRef } from '../lib/types';
  import CanvasThumb from './CanvasThumb.svelte';
  import Icon from './Icon.svelte';
  import SlideView from './SlideView.svelte';

  let { id, canvas = false }: { id: string; canvas?: boolean } = $props();
  let doc = $state<Doc | null>(null);
  let index = $state(0);
  let sub = $state(0);
  let started = $state(Date.now());
  let now = $state(Date.now());
  let chan: BroadcastChannel;
  let pointer = $state<{ x: number; y: number } | null>(null);
  let ink = $state<Record<number, { color: string; pts: [number, number][] }[]>>({});
  let slideStart = $state(Date.now());
  let chapter = $state('');
  let rehearsing = $state(false);

  const slides = $derived(doc ? doc.sections.filter((s) => s.slide) : []);
  const cur = $derived(slides[index]);
  const nxt = $derived(slides[index + 1]);
  // canvas presentation: frames (or cards) instead of slides
  const cst = $derived(doc && canvas ? stepsOf(doc) : []);
  const total = $derived(canvas ? cst.length : slides.length);

  function fmt(ms: number) {
    const s = Math.floor(ms / 1000);
    return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
  }

  function nav(dir: number) {
    chan.postMessage({ type: 'nav', dir });
  }

  onMount(() => {
    api.getDoc(id).then(async (d) => {
      // same automatic layout as the canvas presentation (needs the media sizes)
      if (canvas && d.sections.some((s) => !s.canvas)) {
        await whenResolved(d.sections.filter((s) => !s.canvas).map((s) => s.media[0]).filter(Boolean) as MediaRef[]);
        autoPlace(d);
      }
      doc = d;
    });
    chan = new BroadcastChannel(`letopis-present-${id}`);
    chan.onmessage = (ev) => {
      const m = ev.data;
      if (m?.type === 'state') {
        index = m.index;
        sub = m.sub;
        slideStart = m.slideStart ?? Date.now();
        chapter = m.chapter ?? '';
        rehearsing = !!m.rehearsing;
      } else if (m?.type === 'pointer') pointer = m.on ? { x: m.x, y: m.y } : null;
      else if (m?.type === 'strokes') ink = { ...ink, [m.index]: m.strokes };
    };
    chan.postMessage({ type: 'hello' });
    const t = setInterval(() => (now = Date.now()), 1000);
    document.title = 'Speaker notes · Tessera';
    return () => {
      chan.close();
      clearInterval(t);
    };
  });

  function onKey(e: KeyboardEvent) {
    if (['ArrowRight', 'ArrowDown', 'PageDown', ' '].includes(e.key)) {
      e.preventDefault();
      nav(1);
    } else if (['ArrowLeft', 'ArrowUp', 'PageUp'].includes(e.key)) {
      e.preventDefault();
      nav(-1);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="pr">
  <header>
    <span class="timer" title="Time since this window opened">{fmt(now - started)}</span>
    <button class="btn sm" onclick={() => (started = Date.now())}>Reset</button>
    <span class="grow"></span>
    <span class="clock">{new Date(now).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}</span>
    <span class="count">{total ? index + 1 : 0} / {total}</span>
  </header>
  <div class="cols">
    <div class="left">
      <div class="label">Current</div>
      {#if canvas}
        <div class="box">{#if doc && cst[index]}<CanvasThumb {doc} rect={cst[index].rect} />{/if}</div>
        <div class="notes">
          <div class="label">{cst[index]?.title ?? ''} · Notes</div>
          {#if cst[index]?.notes}<p>{cst[index].notes}</p>{:else}<p class="muted">No notes for this step.</p>{/if}
        </div>
      {:else}
        <div class="box">
          {#if cur}<SlideView section={cur} {sub} still />{/if}
          <svg class="ink" viewBox="0 0 1 1" preserveAspectRatio="none">
            {#each ink[index] ?? [] as st, k (k)}<polyline points={st.pts.map((p) => p.join(',')).join(' ')} stroke={st.color} />{/each}
          </svg>
          {#if pointer}<span class="dot" style="left:{pointer.x * 100}%;top:{pointer.y * 100}%"></span>{/if}
        </div>
        <div class="timing">
          {#if chapter}<span class="chap">{chapter}</span>{/if}
          <span>on this slide <b>{fmt(now - slideStart)}</b></span>
          {#if cur?.rehearsal}<span class:over={(now - slideStart) / 1000 > cur.rehearsal}>rehearsed {fmt(cur.rehearsal * 1000)}</span>{/if}
          {#if rehearsing}<span class="rec">● rehearsing</span>{/if}
        </div>
        <div class="notes">
          <div class="label">Notes</div>
          {#if cur?.notes}<p>{cur.notes}</p>{:else}<p class="muted">No notes for this slide.</p>{/if}
        </div>
      {/if}
    </div>
    <div class="right">
      <div class="label">Next</div>
      {#if canvas}
        <div class="box small">{#if doc && cst[index + 1]}<CanvasThumb {doc} rect={cst[index + 1].rect} />{:else}<div class="end">End of presentation</div>{/if}</div>
      {:else}
        <div class="box small">{#if nxt}<SlideView section={nxt} still />{:else}<div class="end">End of presentation</div>{/if}</div>
      {/if}
      <div class="navs">
        <button class="btn" onclick={() => nav(-1)}><Icon name="chev-left" /> Previous</button>
        <button class="btn primary" onclick={() => nav(1)}>Next <Icon name="chev-right" /></button>
      </div>
      <p class="muted tip">Keep this window on your screen and the presentation on the projector/TV. Arrow keys work in both windows.</p>
    </div>
  </div>
</div>

<style>
  .pr {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: #0e0d13;
    color: #eee;
    color-scheme: dark;
  }
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    border-bottom: 1px solid #ffffff14;
  }
  .timer {
    font: 600 28px var(--mono);
  }
  .clock,
  .count {
    font: 16px var(--mono);
    color: #bbb;
  }
  .grow {
    flex: 1;
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1.6fr 1fr;
    gap: 18px;
    padding: 18px;
  }
  .left,
  .right {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
  }
  .label {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #888;
  }
  .box {
    position: relative;
    aspect-ratio: 16 / 9;
    background: #000;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid #ffffff14;
  }
  .box :global(.slide) {
    --fs: 10px;
  }
  .box.small :global(.slide) {
    --fs: 6px;
  }
  .ink {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    pointer-events: none;
  }
  .ink polyline {
    fill: none;
    stroke-width: 3px;
    vector-effect: non-scaling-stroke;
    stroke-linecap: round;
  }
  .dot {
    position: absolute;
    width: 12px;
    height: 12px;
    margin: -6px 0 0 -6px;
    border-radius: 50%;
    background: #ff2a2a;
    box-shadow: 0 0 8px 3px #ff2a2a99;
  }
  .timing {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 14px;
    color: #bbb;
  }
  .timing b {
    font-family: var(--mono);
    color: #eee;
  }
  .timing .over {
    color: #ff7a7a;
  }
  .chap {
    color: #e8b04b;
  }
  .rec {
    color: #ff5a5a;
  }
  .notes {
    flex: 1;
    overflow: auto;
    font-size: 22px;
    line-height: 1.5;
    white-space: pre-wrap;
  }
  .notes p {
    margin: 4px 0;
  }
  .end {
    display: grid;
    place-items: center;
    height: 100%;
    color: #888;
  }
  .navs {
    display: flex;
    gap: 10px;
  }
  .navs .btn {
    flex: 1;
    justify-content: center;
    height: 44px;
  }
  .tip {
    font-size: 12px;
  }
</style>
