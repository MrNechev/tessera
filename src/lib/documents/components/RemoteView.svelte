<script lang="ts">
  // Phone remote: big next/previous buttons, the current slide's title and notes, the next
  // slide, a timer and a black-screen button. Swipe left/right also works.
  import { onMount } from 'svelte';
  import { RemoteChannel } from '../lib/remote';
  import Icon from './Icon.svelte';

  let { id }: { id: string } = $props();
  let st = $state<{ index: number; total: number; title?: string; notes?: string; next?: string; chapter?: string; black?: boolean } | null>(null);
  let connected = $state(false);
  let started = $state(Date.now());
  let now = $state(Date.now());
  let ch: RemoteChannel;
  let lock: any = null;
  let x0 = 0;

  onMount(() => {
    ch = new RemoteChannel(id, (m) => {
      connected = true;
      if (m.type === 'state') st = m;
    });
    ch.send({ type: 'hello' });
    const t = setInterval(() => (now = Date.now()), 1000);
    (navigator as any).wakeLock?.request('screen').then((l: any) => (lock = l)).catch(() => {});
    document.title = 'Remote · Tessera';
    return () => {
      ch.close();
      clearInterval(t);
      lock?.release?.();
    };
  });

  const nav = (dir: number) => {
    navigator.vibrate?.(15);
    ch.send({ type: 'nav', dir });
  };
  const fmt = (ms: number) => {
    const s = Math.floor(ms / 1000);
    return `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`;
  };
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="rm" ontouchstart={(e) => (x0 = e.touches[0].clientX)} ontouchend={(e) => { const dx = e.changedTouches[0].clientX - x0; if (Math.abs(dx) > 60) nav(dx < 0 ? 1 : -1); }}>
  <header>
    <span class="timer" onclick={() => (started = Date.now())} role="button" tabindex="0" onkeydown={() => {}}>{fmt(now - started)}</span>
    <span class="grow"></span>
    <span class="count">{st ? `${st.index + 1} / ${st.total}` : connected ? '…' : 'connecting…'}</span>
  </header>
  <div class="cur">
    {#if st?.chapter}<div class="chap">{st.chapter}</div>{/if}
    <h1>{st?.title || (st ? `Slide ${st.index + 1}` : 'Waiting for the presentation…')}</h1>
    <div class="notes">{st?.notes || ''}</div>
    {#if st?.next}<div class="next"><span>Next</span> {st.next}</div>{/if}
  </div>
  <div class="btns">
    <button class="prev" onclick={() => nav(-1)} aria-label="Previous"><Icon name="chev-left" size={34} /></button>
    <button class="nxt" onclick={() => nav(1)} aria-label="Next"><Icon name="chev-right" size={42} /></button>
  </div>
  <button class="black" class:on={st?.black} onclick={() => ch.send({ type: 'black' })}>{st?.black ? 'Show the screen' : 'Black screen'}</button>
</div>

<style>
  .rm {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: #0e0d13;
    color: #eee;
    padding: env(safe-area-inset-top) 14px calc(14px + env(safe-area-inset-bottom));
    user-select: none;
    -webkit-user-select: none;
  }
  header {
    display: flex;
    align-items: center;
    padding: 12px 4px;
  }
  .grow {
    flex: 1;
  }
  .timer {
    font: 600 26px var(--mono);
  }
  .count {
    font: 18px var(--mono);
    color: #bbb;
  }
  .cur {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px;
  }
  .chap {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: #e8b04b;
  }
  h1 {
    font: 650 24px/1.2 var(--serif);
    margin: 6px 0 12px;
  }
  .notes {
    font-size: 19px;
    line-height: 1.45;
    white-space: pre-wrap;
    color: #ddd;
  }
  .next {
    margin-top: 16px;
    color: #999;
    font-size: 15px;
  }
  .next span {
    text-transform: uppercase;
    font-size: 11px;
    letter-spacing: 0.1em;
    margin-right: 6px;
  }
  .btns {
    display: grid;
    grid-template-columns: 1fr 2fr;
    gap: 10px;
    height: 34vh;
    margin-top: 10px;
  }
  .btns button {
    border: none;
    border-radius: 18px;
    background: #24222e;
    color: #fff;
    display: grid;
    place-items: center;
  }
  .btns .nxt {
    background: #e8b04b;
    color: #1b1608;
  }
  .btns button:active {
    transform: scale(0.98);
    filter: brightness(1.2);
  }
  .black {
    margin-top: 10px;
    height: 48px;
    border: 1px solid #ffffff22;
    border-radius: 12px;
    background: none;
    color: #ddd;
    font-size: 15px;
  }
  .black.on {
    background: #fff;
    color: #000;
  }
</style>
