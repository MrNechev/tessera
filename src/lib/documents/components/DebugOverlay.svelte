<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '../lib/api';

  let { onclose }: { onclose: () => void } = $props();
  let videos = $state(0);
  let playing = $state(0);
  let images = $state(0);
  let fps = $state(0);
  let worst = $state(0);
  let queues = $state<{ name: string; running: number; queuedHigh: number; queuedLow: number }[]>([]);
  let hit = $state('');
  let proto = $state('');

  onMount(() => {
    let frames = 0;
    let last = performance.now();
    let prev = last;
    let longest = 0;
    let raf = 0;
    const loop = (t: number) => {
      frames++;
      longest = Math.max(longest, t - prev);
      prev = t;
      if (t - last >= 1000) {
        fps = Math.round((frames * 1000) / (t - last));
        worst = Math.round(longest);
        frames = 0;
        longest = 0;
        last = t;
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    const dom = setInterval(() => {
      const vs = document.querySelectorAll('video');
      videos = vs.length;
      playing = [...vs].filter((v) => !v.paused && !v.ended).length;
      images = document.querySelectorAll('img').length;
    }, 500);
    const srv = setInterval(async () => {
      try {
        const s = await api.status();
        queues = s.pools;
        hit = `thumbs ${(s.cache.thumbs.hitRate * 100).toFixed(0)}% · gif ${(s.cache.gif.hitRate * 100).toFixed(0)}%`;
        proto = s.request.proto;
      } catch {}
    }, 2000);
    return () => {
      cancelAnimationFrame(raf);
      clearInterval(dom);
      clearInterval(srv);
    };
  });
</script>

<div class="dbg">
  <div class="h"><b>Performance</b><button onclick={onclose} aria-label="Close">×</button></div>
  <div class:bad={fps < 50}>FPS {fps} <span class="m">(worst frame {worst} ms)</span></div>
  <div>video elements {videos} · playing {playing}</div>
  <div>img elements {images}</div>
  <div class="m">{proto}</div>
  {#each queues as q (q.name)}
    <div class="m">{q.name}: {q.running} run · {q.queuedHigh} now · {q.queuedLow} bg</div>
  {/each}
  {#if hit}<div class="m">cache {hit}</div>{/if}
</div>

<style>
  .dbg {
    position: fixed;
    right: 12px;
    bottom: 12px;
    z-index: 900;
    font: 11.5px/1.5 var(--mono);
    background: #000c;
    color: #cfe;
    padding: 8px 10px;
    border-radius: 8px;
    pointer-events: auto;
    min-width: 230px;
  }
  .h {
    display: flex;
    justify-content: space-between;
  }
  .h button {
    background: none;
    border: none;
    color: #cfe;
    cursor: pointer;
    font-size: 14px;
  }
  .m {
    color: #8a9;
  }
  .bad {
    color: #f99;
  }
</style>
