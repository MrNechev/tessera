<script lang="ts">
  // Canvas presentation: the camera flies from frame to frame (or card to card when the
  // canvas has no frames). Media of the target step is mounted when the flight starts and
  // plays on arrival; everything else stays a still image.
  import { onMount, untrack } from 'svelte';
  import { api } from '../lib/api';
  import {
    autoPlace,
    contentRect,
    easeInOut,
    expand,
    fitView,
    flight,
    steps as stepsOf,
    toWorld,
    union,
    visibleRect,
    whenResolved,
    containsPoint,
    type View,
  } from '../lib/canvas';
  import { resolver } from '../lib/resolver.svelte';
  import { pair, RemoteChannel } from '../lib/remote';
  import QRCode from './QRCode.svelte';
  import { containsRect, isBox, arrowRect, targets } from '../lib/canvas';
  import { router } from '../lib/router.svelte';
  import type { Doc, MediaRef, Rect } from '../lib/types';
  import CanvasWorld from './CanvasWorld.svelte';
  import Icon from './Icon.svelte';

  let { id, start = 0 }: { id: string; start?: number } = $props();

  let doc = $state<Doc | null>(null);
  let error = $state('');
  let index = $state(0);
  let black = $state(false);
  let idle = $state(false);
  let overview = $state(false);
  let fullscreen = $state(!!document.fullscreenElement);
  let vw = $state(0);
  let vh = $state(0);
  let view = $state<View>({ tx: 0, ty: 0, z: 1 });
  let lod = $state(1);
  let renderRect = $state<Rect>({ x: 0, y: 0, w: 0, h: 0 });
  let mountRect = $state<Rect | null>(null);
  let playRect = $state<Rect | null>(null);
  let moving = $state(false);
  let idleTimer: ReturnType<typeof setTimeout>;
  let anim = 0;
  let chan: BroadcastChannel | null = null;
  let remote: RemoteChannel | null = null;
  let level = $state(0); // reveal level inside the current frame
  let qr = $state<{ urls: string[] } | null>(null);
  async function openQR() {
    try {
      qr = await pair(id);
    } catch {}
  }

  const st = $derived(doc ? stepsOf(doc) : []);

  // reveal steps: items inside a frame with reveal = n appear on the n-th click
  const revealOf = $derived.by(() => {
    const out = new Map<string, { id: string; n: number }[]>(); // step id → items
    if (!doc) return out;
    const tg = targets(doc);
    const all: { id: string; n: number; r: { x: number; y: number; w: number; h: number } }[] = [];
    for (const s of doc.sections) if (s.canvas && !s.canvas.hidden && s.canvas.reveal) all.push({ id: s.id, n: s.canvas.reveal, r: s.canvas });
    for (const it of doc.canvas?.items ?? []) {
      const n = (it as { reveal?: number }).reveal;
      if (n) all.push({ id: it.id, n, r: isBox(it) ? it : arrowRect(it, tg) });
    }
    for (const step of st) out.set(step.id, all.filter((a) => containsRect(step.rect, a.r)).map(({ id, n }) => ({ id, n })));
    return out;
  });
  const levels = (i: number) => [...new Set((revealOf.get(st[i]?.id) ?? []).map((x) => x.n))].sort((a, b) => a - b);
  const concealed = $derived.by(() => {
    const out = new Set<string>();
    st.forEach((step, i) => {
      const lv = levels(i);
      const shown = i < index ? Infinity : i > index ? 0 : (lv[level - 1] ?? 0);
      for (const x of revealOf.get(step.id) ?? []) if (x.n > shown) out.add(x.id);
    });
    return out;
  });

  function targetView(i: number): View {
    const s = st[i];
    return fitView(s.rect, vw, vh, s.frame ? 0.015 : 0.04, 8);
  }

  function arrive() {
    moving = false;
    lod = view.z;
    const live = visibleRect(view, vw, vh);
    renderRect = expand(live, 0.5);
    mountRect = expand(live, 0.15);
    playRect = overview ? null : live;
  }

  function flyTo(target: View, instant = false) {
    cancelAnimationFrame(anim);
    const dest = visibleRect(target, vw, vh);
    mountRect = overview ? null : expand(dest, 0.15);
    playRect = null;
    lod = target.z;
    if (instant || !vw) {
      view = target;
      arrive();
      return;
    }
    const path = flight(view, target, vw, vh);
    const mid = visibleRect(path(0.5), vw, vh);
    renderRect = union([visibleRect(view, vw, vh), mid, dest])!;
    moving = true;
    const dist = Math.abs(Math.log(target.z / view.z)) + Math.hypot(target.tx - view.tx, target.ty - view.ty) / Math.max(vw, 1);
    const ms = Math.min(1500, 650 + dist * 180);
    const t0 = performance.now();
    const step = (t: number) => {
      const k = Math.min(1, (t - t0) / ms);
      view = path(easeInOut(k));
      if (k < 1) anim = requestAnimationFrame(step);
      else arrive();
    };
    anim = requestAnimationFrame(step);
  }

  function broadcast() {
    const m = {
      type: 'state',
      index,
      sub: level,
      total: st.length,
      canvas: true,
      black,
      title: st[index]?.title ?? '',
      notes: st[index]?.notes ?? '',
      next: st[index + 1]?.title ?? '',
    };
    chan?.postMessage(m);
    remote?.send(m);
  }

  function go(i: number, instant = false, atLevel = 0) {
    if (!st.length) return;
    index = Math.max(0, Math.min(st.length - 1, i));
    level = atLevel;
    black = false;
    overview = false;
    history.replaceState(null, '', `#/present/${id}?canvas=1&s=${index}`);
    broadcast();
    flyTo(targetView(index), instant);
    // get the media of the next steps ready on the server
    const nextRefs = doc ? doc.sections.filter((s) => s.canvas && st.slice(index + 1, index + 3).some((x) => containsRect(x.rect, s.canvas!))).flatMap((s) => s.media) : [];
    if (nextRefs.length) api.prewarm(nextRefs);
  }

  function next() {
    if (overview) return go(index);
    if (level < levels(index).length) {
      level++;
      broadcast();
      return;
    }
    if (index < st.length - 1) go(index + 1);
  }
  function prev() {
    if (overview) return go(index);
    if (level > 0) {
      level--;
      broadcast();
      return;
    }
    if (index > 0) go(index - 1, false, levels(index - 1).length);
  }

  function toggleOverview() {
    if (overview) return go(index);
    const r = doc && contentRect(doc);
    if (!r) return;
    overview = true;
    flyTo(fitView(r, vw, vh, 0.04, 1));
  }

  function exit() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    router.go(`/doc/${id}/canvas`);
  }

  function toggleFullscreen() {
    if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
    else document.documentElement.requestFullscreen().catch(() => {});
  }

  function openPresenter() {
    window.open(`#/presenter/${id}?canvas=1`, `letopis-presenter-${id}`, 'width=1100,height=720');
  }

  function onKey(e: KeyboardEvent) {
    if (e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return;
    switch (e.key) {
      case 'ArrowRight':
      case 'ArrowDown':
      case 'PageDown':
      case ' ':
      case 'Enter':
        e.preventDefault();
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
        go(st.length - 1);
        break;
      case 'Escape':
        if (overview) go(index);
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
        toggleOverview();
        break;
    }
  }

  function onMove() {
    idle = false;
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => (idle = true), 2200);
  }

  function onClick(e: MouseEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('video, button, a, select, .controls, .qrbox')) return;
    if (overview) {
      // jump to the smallest step under the pointer
      const p = toWorld(view, e.clientX, e.clientY);
      let best = -1,
        area = Infinity;
      st.forEach((s, i) => {
        if (containsPoint(s.rect, p.x, p.y) && s.rect.w * s.rect.h < area) {
          best = i;
          area = s.rect.w * s.rect.h;
        }
      });
      go(best >= 0 ? best : index);
      return;
    }
    if (e.clientX < innerWidth * 0.25) prev();
    else next();
  }

  // window resized: re-fit the current step without animation
  $effect(() => {
    void vw;
    void vh;
    if (doc && st.length) untrack(() => (overview ? toggleOverview() : go(index, true)));
  });

  onMount(() => {
    api
      .getDoc(id)
      .then(async (d) => {
        const unplaced = d.sections.filter((s) => !s.canvas);
        if (unplaced.length) {
          await whenResolved(unplaced.map((s) => s.media[0]).filter(Boolean) as MediaRef[]);
          autoPlace(d);
        }
        resolver.prefetch(d.sections.flatMap((s) => s.media));
        doc = d;
        document.title = `${d.title} · Presenting`;
        go(start, true);
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
    onMove();
    return () => {
      cancelAnimationFrame(anim);
      chan?.close();
      remote?.close();
      document.removeEventListener('fullscreenchange', fs);
      clearTimeout(idleTimer);
      document.title = 'Tessera';
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="cpres" class:idle class:overview bind:clientWidth={vw} bind:clientHeight={vh} onmousemove={onMove} onclick={onClick}>
  {#if error}
    <div class="msg">{error} <button class="btn" onclick={exit}>Back</button></div>
  {:else if doc && st.length === 0}
    <div class="msg">The canvas is empty: add frames or cards first. <button class="btn" onclick={exit}>Back to the canvas</button></div>
  {:else if doc}
    <CanvasWorld {doc} {view} {renderRect} mountRect={black ? null : mountRect} playRect={black ? null : playRect} {lod} {moving} present outlineFrames={overview} {concealed} />
    {#if black}<div class="black"></div>{/if}

    <div class="controls">
      <button class="icon-btn" title="Previous (←)" onclick={prev}><Icon name="chev-left" /></button>
      <span class="count" title={st[index]?.title}>{index + 1} / {st.length}</span>
      <button class="icon-btn" title="Next (→ / Space)" onclick={next}><Icon name="chev-right" /></button>
      <span class="sep"></span>
      <button class="icon-btn" class:on={overview} title="Overview (O)" onclick={toggleOverview}><Icon name="grid" /></button>
      <button class="icon-btn" title="Speaker notes window (P)" onclick={openPresenter}><Icon name="notes" /></button>
      <button class="icon-btn" title="Phone remote (Q)" onclick={openQR}><Icon name="phone" /></button>
      <button class="icon-btn" title="Full screen (F)" onclick={toggleFullscreen}><Icon name="maximize" /></button>
      <button class="icon-btn" title="Exit (Esc)" onclick={exit}><Icon name="x" /></button>
    </div>
    {#if !fullscreen && !idle}
      <div class="hint">Press <span class="kbd">F</span> for full screen · <span class="kbd">←</span> <span class="kbd">→</span> to navigate · <span class="kbd">O</span> overview · <span class="kbd">P</span> speaker notes</div>
    {/if}
    {#if overview}<div class="hint ov">Click a frame to go there · <span class="kbd">Esc</span> back</div>{/if}
    {#if qr}
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="qrbox" onclick={(e) => e.stopPropagation()}>
        <h3>Use your phone as the remote</h3>
        <QRCode text={qr.urls[0]} size={240} />
        <p>Scan with the phone's camera (same network or Tailscale).</p>
        <button class="btn" onclick={() => (qr = null)}>Close</button>
      </div>
    {/if}
    <div class="progress" style="transform:scaleX({st.length > 1 ? index / (st.length - 1) : 1})"></div>
  {/if}
</div>

<style>
  .cpres {
    position: fixed;
    inset: 0;
    background: var(--bg);
    overflow: hidden;
    user-select: none;
  }
  .cpres.idle {
    cursor: none;
  }
  .cpres.overview {
    cursor: pointer;
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
  .idle .controls,
  .idle .hint:not(.ov) {
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
    color: #ddd;
    background: #000a;
    padding: 4px 12px;
    border-radius: 999px;
    z-index: 10;
    transition: opacity 0.3s;
  }
  .hint .kbd {
    color: #ddd;
    border-color: #ffffff33;
  }
  .hint.ov {
    top: 48px;
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
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    cursor: default;
  }
  .qrbox h3 {
    margin: 0;
  }
  .qrbox p {
    font-size: 13px;
    color: #ccc;
    margin: 0;
  }
  .msg {
    position: absolute;
    inset: 0;
    display: flex;
    gap: 14px;
    align-items: center;
    justify-content: center;
  }
</style>
