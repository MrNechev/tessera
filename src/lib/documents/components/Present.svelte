<script lang="ts">
  import { onMount } from 'svelte';
  import { api, ApiError } from '../lib/api';
  import { pair, RemoteChannel } from '../lib/remote';
  import { resolver } from '../lib/resolver.svelte';
  import { router } from '../lib/router.svelte';
  import { toasts } from '../lib/toast.svelte';
  import type { Doc, Section } from '../lib/types';
  import Icon from './Icon.svelte';
  import QRCode from './QRCode.svelte';
  import SlideView from './SlideView.svelte';

  let { id, start = 0 }: { id: string; start?: number } = $props();

  let doc = $state<Doc | null>(null);
  let error = $state('');
  let index = $state(0);
  let sub = $state(0);
  let black = $state(false);
  let idle = $state(false);
  let fullscreen = $state(!!document.fullscreenElement);
  let overview = $state(false);
  let transition = $state<'fade' | 'slide' | 'zoom' | 'none'>('fade');
  let idleTimer: ReturnType<typeof setTimeout>;
  let chan: BroadcastChannel | null = null;
  let remote: RemoteChannel | null = null;

  // pointer tools
  let tool = $state<'none' | 'laser' | 'pen'>('none');
  let laser = $state<{ x: number; y: number } | null>(null);
  type Stroke = { color: string; pts: [number, number][] };
  let strokes = $state<Record<number, Stroke[]>>({});
  let drawing: Stroke | null = null;
  let penColor = $state('#ff3b3b');

  // timing
  let autoPaused = $state(false);
  let slideStart = $state(Date.now());
  let now = $state(Date.now());
  let rehearsing = $state(false);
  const rehearsal = new Map<string, number>();

  // music
  let music = $state<HTMLAudioElement>();
  let musicOn = $state(true);
  let musicBlocked = $state(false);
  let ducked = $state(false);

  // phone remote
  let qr = $state<{ urls: string[] } | null>(null);

  const slides = $derived<Section[]>(doc ? doc.sections.filter((s) => s.slide) : []);
  const current = $derived(slides[index]);
  const isCarousel = (s?: Section) => !!s && s.layout === 'gallery' && s.gallery?.mode === 'carousel' && s.media.length > 1;
  const settings = $derived(doc?.settings ?? {});
  // chapter of each slide: the nearest chapter section at or before it in the document
  const chapters = $derived.by(() => {
    const out = new Map<string, string>();
    let ch = '';
    for (const s of doc?.sections ?? []) {
      if (s.chapter) ch = s.title || 'Chapter';
      out.set(s.id, ch);
    }
    return out;
  });
  const advanceSecs = $derived(current ? (current.advance ?? settings.autoAdvance ?? 0) : 0);
  const left = $derived(advanceSecs && !autoPaused && !overview && !black ? Math.max(0, advanceSecs - (now - slideStart) / 1000) : 0);
  const musicSrc = $derived.by(() => {
    const m = settings.music;
    if (!m) return '';
    const r = resolver.get(m);
    return r?.found ? (r.doc?.url ?? r.original ?? '') : '';
  });

  try {
    const t = localStorage.getItem('letopis.transition');
    if (t === 'fade' || t === 'slide' || t === 'zoom' || t === 'none') transition = t;
  } catch {}

  function setTransition(t: typeof transition) {
    transition = t;
    try {
      localStorage.setItem('letopis.transition', t);
    } catch {}
  }

  function stateMsg() {
    const nxt = slides[index + 1];
    return {
      type: 'state',
      index,
      sub,
      total: slides.length,
      black,
      title: current?.title ?? '',
      notes: current?.notes ?? '',
      next: nxt ? nxt.title || `Slide ${index + 2}` : '',
      chapter: current ? chapters.get(current.id) : '',
      rehearsing,
      slideStart,
    };
  }

  function broadcast() {
    const m = stateMsg();
    chan?.postMessage(m);
    remote?.send(m);
  }

  function recordRehearsal() {
    if (!rehearsing || !current) return;
    rehearsal.set(current.id, (rehearsal.get(current.id) ?? 0) + (Date.now() - slideStart) / 1000);
  }

  function go(i: number, s = 0) {
    if (!slides.length) return;
    recordRehearsal();
    index = Math.max(0, Math.min(slides.length - 1, i));
    sub = s;
    black = false;
    slideStart = Date.now();
    history.replaceState(null, '', `#/present/${id}?s=${index}`);
    broadcast();
    // get the next slides ready: client-side metadata and server-side full-size files
    const upcoming = slides.slice(index + 1, index + 3).flatMap((x) => x.media);
    if (upcoming.length) {
      resolver.prefetch(upcoming);
      api.prewarm(upcoming);
    }
  }

  function next() {
    if (isCarousel(current) && sub < current!.media.length - 1) {
      sub++;
      slideStart = Date.now();
      broadcast();
      return;
    }
    if (index < slides.length - 1) go(index + 1);
    else if (settings.loop) go(0);
    else if (rehearsing) stopRehearsal();
  }

  function prev() {
    if (isCarousel(current) && sub > 0) {
      sub--;
      broadcast();
      return;
    }
    if (index > 0) {
      const p = slides[index - 1];
      go(index - 1, isCarousel(p) ? p.media.length - 1 : 0);
    }
  }

  function chapterJump(dir: number) {
    const ch = (i: number) => chapters.get(slides[i]?.id) ?? '';
    const curCh = ch(index);
    let i = index;
    if (dir > 0) {
      while (i < slides.length - 1 && ch(i) === curCh) i++;
    } else {
      // to the start of this chapter, or of the previous one if already there
      while (i > 0 && ch(i - 1) === curCh) i--;
      if (i === index && i > 0) {
        const prevCh = ch(i - 1);
        i--;
        while (i > 0 && ch(i - 1) === prevCh) i--;
      }
    }
    go(i);
  }

  async function exit() {
    if (rehearsing) await stopRehearsal();
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    router.go(`/doc/${id}`);
  }

  function toggleFullscreen() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else document.documentElement.requestFullscreen().catch(() => {});
  }

  function openPresenter() {
    window.open(`#/presenter/${id}`, `letopis-presenter-${id}`, 'width=1100,height=720');
  }

  async function openQR() {
    try {
      qr = await pair(id);
    } catch (e) {
      toasts.error((e as Error).message);
    }
  }

  function startRehearsal() {
    rehearsal.clear();
    rehearsing = true;
    go(0);
    toasts.show('Rehearsing: the time on each slide is recorded. Press R again or finish the last slide to save.', 'info', 5000);
  }

  async function stopRehearsal() {
    if (!rehearsing) return;
    recordRehearsal();
    rehearsing = false;
    broadcast();
    // save the timings into the document (on top of the latest version)
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        const d = await api.getDoc(id);
        for (const s of d.sections) {
          const t = rehearsal.get(s.id);
          if (t) s.rehearsal = Math.round(t);
        }
        await api.saveDoc(id, d.version, d);
        doc = d;
        toasts.show('Rehearsal timings saved — the speaker window shows them next time', 'ok');
        return;
      } catch (e) {
        if (!(e instanceof ApiError && e.status === 409)) {
          toasts.error('Could not save the rehearsal: ' + (e as Error).message);
          return;
        }
      }
    }
  }

  function setTool(t: typeof tool) {
    tool = tool === t ? 'none' : t;
    if (tool !== 'laser') {
      laser = null;
      chan?.postMessage({ type: 'pointer', on: false });
    }
  }

  function clearInk() {
    strokes = { ...strokes, [index]: [] };
    chan?.postMessage({ type: 'strokes', index, strokes: [] });
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    if (e.shiftKey && (e.key === 'ArrowRight' || e.key === 'ArrowLeft')) {
      e.preventDefault();
      chapterJump(e.key === 'ArrowRight' ? 1 : -1);
      return;
    }
    switch (e.key) {
      case 'ArrowRight':
      case 'ArrowDown':
      case 'PageDown':
      case ' ':
      case 'Enter':
        e.preventDefault();
        if (overview) overview = false;
        next();
        break;
      case 'ArrowLeft':
      case 'ArrowUp':
      case 'PageUp':
      case 'Backspace':
        e.preventDefault();
        prev();
        break;
      case 'Home':
        go(0);
        break;
      case 'End':
        go(slides.length - 1);
        break;
      case 'Escape':
        if (qr) qr = null;
        else if (tool !== 'none') setTool('none');
        else if (overview) overview = false;
        else if (!document.fullscreenElement) exit();
        break;
      case 'f':
      case 'F':
        toggleFullscreen();
        break;
      case 'b':
      case 'B':
      case '.':
        black = !black;
        broadcast();
        break;
      case 'p':
      case 'P':
        openPresenter();
        break;
      case 'q':
      case 'Q':
        qr ? (qr = null) : openQR();
        break;
      case 'o':
      case 'O':
      case 'g':
        overview = !overview;
        break;
      case 'l':
      case 'L':
        setTool('laser');
        break;
      case 'd':
      case 'D':
        setTool('pen');
        break;
      case 'c':
      case 'C':
        clearInk();
        break;
      case 'm':
      case 'M':
        musicOn = !musicOn;
        break;
      case 's':
      case 'S':
        autoPaused = !autoPaused;
        slideStart = Date.now();
        break;
      case 'r':
      case 'R':
        rehearsing ? stopRehearsal() : startRehearsal();
        break;
    }
  }

  function onMove(e: MouseEvent) {
    idle = false;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (idle = true), 2200);
    if (tool === 'laser') {
      laser = { x: e.clientX / innerWidth, y: e.clientY / innerHeight };
      chan?.postMessage({ type: 'pointer', on: true, ...laser });
    }
    if (tool === 'pen' && drawing) {
      drawing.pts.push([e.clientX / innerWidth, e.clientY / innerHeight]);
      strokes = { ...strokes };
    }
  }

  function onDown(e: PointerEvent) {
    if (tool !== 'pen' || (e.target as HTMLElement).closest('.controls, button, video, .qrbox')) return;
    drawing = { color: penColor, pts: [[e.clientX / innerWidth, e.clientY / innerHeight]] };
    strokes = { ...strokes, [index]: [...(strokes[index] ?? []), drawing] };
  }

  function onUp() {
    if (drawing) {
      chan?.postMessage({ type: 'strokes', index, strokes: $state.snapshot(strokes[index] ?? []) });
      drawing = null;
    }
  }

  function onClick(e: MouseEvent) {
    const t = e.target as HTMLElement;
    if (tool === 'pen' || t.closest('video, button, a, select, .controls, .overview, .qrbox, .cmp')) return;
    if (e.clientX < innerWidth * 0.25) prev();
    else next();
  }

  // auto-advance
  $effect(() => {
    if (!left && advanceSecs && !autoPaused && !overview && !black && slides.length && now - slideStart > 500) next();
  });

  // music: loop across slides, quieter while a video with sound plays on the slide
  $effect(() => {
    const a = music;
    if (!a) return;
    const vol = settings.musicVolume ?? 0.6;
    a.volume = ducked ? vol * 0.15 : vol;
    if (musicOn && musicSrc && !black) a.play().then(() => (musicBlocked = false)).catch(() => (musicBlocked = true));
    else a.pause();
  });

  function checkDuck() {
    const vids = [...document.querySelectorAll<HTMLVideoElement>('.present .frame.cur video')];
    ducked = vids.some((v) => !v.paused && !v.muted && v.volume > 0);
  }

  onMount(() => {
    api
      .getDoc(id)
      .then((d) => {
        doc = d;
        resolver.prefetch(d.sections.filter((s) => s.slide).flatMap((s) => s.media));
        if (d.settings?.music) resolver.prefetch([d.settings.music]);
        go(start);
        document.title = `${d.title} · Presenting`;
      })
      .catch((e) => (error = (e as Error).message));
    chan = new BroadcastChannel(`letopis-present-${id}`);
    const onCmd = (m: any) => {
      if (m?.type === 'nav') m.dir > 0 ? next() : prev();
      else if (m?.type === 'goto') go(m.index);
      else if (m?.type === 'hello') broadcast();
      else if (m?.type === 'black') {
        black = !black;
        broadcast();
      }
    };
    chan.onmessage = (ev) => onCmd(ev.data);
    remote = new RemoteChannel(id, onCmd);
    const fs = () => (fullscreen = !!document.fullscreenElement);
    document.addEventListener('fullscreenchange', fs);
    const media = () => checkDuck();
    for (const ev of ['play', 'pause', 'volumechange', 'ended']) document.addEventListener(ev, media, true);
    const tick = setInterval(() => (now = Date.now()), 250);
    onMove(new MouseEvent('mousemove'));
    return () => {
      chan?.close();
      remote?.close();
      clearInterval(tick);
      document.removeEventListener('fullscreenchange', fs);
      for (const ev of ['play', 'pause', 'volumechange', 'ended']) document.removeEventListener(ev, media, true);
      clearTimeout(idleTimer);
      document.title = 'Tessera';
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="present t-{transition}"
  class:idle
  class:laser={tool === 'laser'}
  class:pen={tool === 'pen'}
  onmousemove={onMove}
  onclick={onClick}
  onpointerdown={onDown}
  onpointerup={onUp}
>
  {#if musicSrc}<audio bind:this={music} src={musicSrc} loop preload="auto"></audio>{/if}
  {#if error}
    <div class="msg">{error} <button class="btn" onclick={exit}>Back</button></div>
  {:else if doc && slides.length === 0}
    <div class="msg">No sections are marked for the presentation. <button class="btn" onclick={exit}>Back to the editor</button></div>
  {:else if doc}
    {#each slides as s, i (s.id)}
      {#if Math.abs(i - index) <= 1}
        <div class="frame" class:cur={i === index} class:past={i < index} class:future={i > index} aria-hidden={i !== index}>
          <SlideView
            section={s}
            active={i === index && !black && !overview}
            sub={i === index ? sub : 0}
            onended={settings.advanceOnVideoEnd && i === index ? () => next() : undefined}
          />
        </div>
      {/if}
    {/each}
    {#if black}<div class="black"></div>{/if}

    <svg class="ink" viewBox="0 0 1 1" preserveAspectRatio="none">
      {#each strokes[index] ?? [] as st, k (k)}
        <polyline points={st.pts.map((p) => p.join(',')).join(' ')} stroke={st.color} />
      {/each}
    </svg>
    {#if laser && tool === 'laser'}<div class="dot" style="left:{laser.x * 100}%;top:{laser.y * 100}%"></div>{/if}

    {#if overview}
      <div class="overview">
        {#each slides as s, i (s.id)}
          {#if (chapters.get(s.id) ?? '') !== (i ? chapters.get(slides[i - 1].id) ?? '' : '') && chapters.get(s.id)}
            <div class="ovch">{chapters.get(s.id)}</div>
          {/if}
          <button class="ov" class:on={i === index} onclick={() => { go(i); overview = false; }}>
            <div class="ovs"><SlideView section={s} still /></div>
            <span>{i + 1}. {s.title || ''}</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="controls">
      <button class="icon-btn" title="Previous (←)" onclick={prev}><Icon name="chev-left" /></button>
      <span class="count" title={current ? chapters.get(current.id) : ''}>{index + 1} / {slides.length}{isCarousel(current) ? ` · ${sub + 1}/${current!.media.length}` : ''}</span>
      {#if advanceSecs}
        <button class="icon-btn" title={autoPaused ? 'Resume the timed slideshow (S)' : `Next slide in ${Math.ceil(left)} s — pause (S)`} onclick={() => { autoPaused = !autoPaused; slideStart = Date.now(); }}>
          {#if autoPaused}<Icon name="play" />{:else}<span class="ring" style="--p:{1 - left / advanceSecs}"></span>{/if}
        </button>
      {/if}
      {#if rehearsing}<span class="rec" title="Rehearsing (R to stop and save)">● {Math.floor((now - slideStart) / 1000)} s</span>{/if}
      <button class="icon-btn" title="Next (→ / Space)" onclick={next}><Icon name="chev-right" /></button>
      <span class="sep"></span>
      <select class="input" value={transition} onchange={(e) => setTransition(e.currentTarget.value as typeof transition)} title="Transition">
        <option value="fade">Fade</option>
        <option value="slide">Slide</option>
        <option value="zoom">Zoom</option>
        <option value="none">None</option>
      </select>
      <button class="icon-btn" title="Overview (O)" onclick={() => (overview = !overview)}><Icon name="grid" /></button>
      <button class="icon-btn" class:on={tool === 'laser'} title="Laser pointer (L)" onclick={() => setTool('laser')}><Icon name="pointer2" /></button>
      <button class="icon-btn" class:on={tool === 'pen'} title="Draw on the slide (D) · C clears" onclick={() => setTool('pen')}><Icon name="pen" /></button>
      {#if tool === 'pen'}
        {#each ['#ff3b3b', '#ffd23b', '#3bd1ff', '#ffffff'] as c (c)}
          <button class="pc" class:on={penColor === c} style="background:{c}" onclick={() => (penColor = c)} aria-label="Pen colour"></button>
        {/each}
        <button class="icon-btn" title="Clear drawings (C)" onclick={clearInk}><Icon name="eraser" /></button>
      {/if}
      {#if musicSrc}<button class="icon-btn" class:on={musicOn && !musicBlocked} title={musicBlocked ? 'Click to start the music' : 'Music on/off (M)'} onclick={() => { musicOn = musicBlocked ? true : !musicOn; music?.play().catch(() => {}); }}><Icon name="music" /></button>{/if}
      <button class="icon-btn" class:on={rehearsing} title="Rehearse: record the time per slide (R)" onclick={() => (rehearsing ? stopRehearsal() : startRehearsal())}><Icon name="timer" /></button>
      <button class="icon-btn" title="Phone remote (Q)" onclick={openQR}><Icon name="phone" /></button>
      <button class="icon-btn" title="Speaker notes window (P)" onclick={openPresenter}><Icon name="notes" /></button>
      <button class="icon-btn" title="Full screen (F)" onclick={toggleFullscreen}><Icon name="maximize" /></button>
      <button class="icon-btn" title="Exit (Esc)" onclick={exit}><Icon name="x" /></button>
    </div>
    {#if !fullscreen && !idle}
      <div class="hint">Press <span class="kbd">F</span> for full screen · <span class="kbd">←</span> <span class="kbd">→</span> to navigate · <span class="kbd">P</span> speaker notes</div>
    {/if}
    {#if qr}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="qrbox" onclick={(e) => e.stopPropagation()}>
        <h3>Use your phone as the remote</h3>
        <QRCode text={qr.urls[0]} size={260} />
        <p>Scan with the phone's camera. The phone must be on the same network (or on Tailscale).</p>
        <details>
          <summary>Other addresses</summary>
          {#each qr.urls as u (u)}<div class="url">{u}</div>{/each}
        </details>
        <p class="small">The code works for 15 minutes; the phone can only control this presentation.</p>
        <button class="btn" onclick={() => (qr = null)}>Close</button>
      </div>
    {/if}
    <div class="progress" style="transform:scaleX({slides.length > 1 ? index / (slides.length - 1) : 1})"></div>
  {/if}
</div>

<style>
  .present {
    position: fixed;
    inset: 0;
    background: #0b0a0f;
    overflow: hidden;
    color-scheme: dark;
    user-select: none;
  }
  .present.idle {
    cursor: none;
  }
  .frame {
    position: absolute;
    inset: 0;
    transition:
      opacity 0.45s ease,
      transform 0.55s cubic-bezier(0.2, 0.7, 0.2, 1);
    will-change: opacity, transform;
  }
  .frame:not(.cur) {
    opacity: 0;
    pointer-events: none;
  }
  .t-slide .frame.past {
    transform: translateX(-6%);
  }
  .t-slide .frame.future {
    transform: translateX(6%);
  }
  .t-zoom .frame.past {
    transform: scale(1.06);
  }
  .t-zoom .frame.future {
    transform: scale(0.94);
  }
  .t-none .frame {
    transition: none;
  }
  .present.laser,
  .present.pen {
    cursor: crosshair;
  }
  .present.laser {
    cursor: none;
  }
  .ink {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    z-index: 7;
    pointer-events: none;
  }
  .ink polyline {
    fill: none;
    stroke-width: 4px;
    vector-effect: non-scaling-stroke;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .dot {
    position: absolute;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border-radius: 50%;
    background: #ff2a2a;
    box-shadow: 0 0 10px 4px #ff2a2a99, 0 0 2px #fff inset;
    z-index: 8;
    pointer-events: none;
  }
  .pc {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid transparent;
    cursor: pointer;
    padding: 0;
  }
  .pc.on {
    border-color: #fff;
  }
  .ring {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: conic-gradient(var(--accent) calc(var(--p) * 360deg), #ffffff33 0);
    display: inline-block;
  }
  .rec {
    color: #ff5a5a;
    font: 12px var(--mono);
    margin: 0 4px;
  }
  .qrbox {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    z-index: 20;
    background: #1b1a22;
    border: 1px solid #ffffff22;
    border-radius: 16px;
    padding: 20px 24px;
    color: #eee;
    text-align: center;
    max-width: 420px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    cursor: default;
  }
  .qrbox h3 {
    margin: 0 0 6px;
  }
  .qrbox p {
    margin: 4px 0;
    font-size: 13px;
    color: #ccc;
  }
  .qrbox .small {
    font-size: 11px;
    color: #999;
  }
  .qrbox details {
    font-size: 12px;
    color: #bbb;
    max-width: 100%;
  }
  .qrbox .url {
    font: 11px var(--mono);
    word-break: break-all;
    user-select: all;
  }
  .ovch {
    grid-column: 1 / -1;
    font: 650 18px var(--serif);
    color: #e8b04b;
    margin-top: 10px;
  }
  .black {
    position: absolute;
    inset: 0;
    background: #000;
    z-index: 5;
  }
  .controls {
    position: absolute;
    bottom: 18px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px;
    background: #1b1a22e6;
    border: 1px solid #ffffff1a;
    border-radius: 12px;
    z-index: 10;
    transition: opacity 0.3s;
    color: #eee;
  }
  .controls .icon-btn {
    color: #ddd;
  }
  .controls .icon-btn:hover {
    background: #ffffff1a;
  }
  .controls select {
    height: 28px;
    background: #0006;
    border-color: #ffffff22;
    font-size: 12px;
  }
  .idle .controls,
  .idle .hint {
    opacity: 0;
  }
  .count {
    font: 12px var(--mono);
    min-width: 64px;
    text-align: center;
  }
  .sep {
    width: 1px;
    height: 18px;
    background: #ffffff22;
    margin: 0 4px;
  }
  .hint {
    position: absolute;
    top: 14px;
    left: 50%;
    transform: translateX(-50%);
    font-size: 12px;
    color: #bbb;
    background: #0008;
    padding: 4px 12px;
    border-radius: 999px;
    z-index: 10;
    transition: opacity 0.3s;
  }
  .hint .kbd {
    color: #ddd;
    border-color: #ffffff33;
  }
  .progress {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent);
    transform-origin: left;
    transition: transform 0.4s;
    opacity: 0.7;
    z-index: 6;
  }
  .msg {
    position: absolute;
    inset: 0;
    display: flex;
    gap: 14px;
    align-items: center;
    justify-content: center;
    color: #ddd;
  }
  .overview {
    position: absolute;
    inset: 0;
    z-index: 8;
    background: #0b0a0ff2;
    overflow: auto;
    padding: 40px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 18px;
    align-content: start;
  }
  .ov {
    background: none;
    border: none;
    color: #ccc;
    cursor: pointer;
    text-align: left;
    font-size: 12px;
    padding: 0;
  }
  .ovs {
    position: relative;
    aspect-ratio: 16 / 9;
    border-radius: 8px;
    overflow: hidden;
    background: #16151c;
    border: 2px solid transparent;
    margin-bottom: 5px;
  }
  .ovs :global(.slide) {
    --fs: 6px;
    padding: 8px;
    gap: 4px;
  }
  .ov.on .ovs {
    border-color: var(--accent);
  }
</style>
