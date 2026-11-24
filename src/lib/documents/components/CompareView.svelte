<script lang="ts">
  import { mediaStyle } from '../../look/looks.svelte';
  // Comparison layouts for the first two media of a section:
  //   slider — B over A, revealed up to a draggable divider (before/after)
  //   sync   — side by side; videos play, pause and seek together
  import { srcset, thumbUrl } from '../lib/api';
  import { resolver } from '../lib/resolver.svelte';
  import type { MediaRef, Section } from '../lib/types';
  import Icon from './Icon.svelte';

  let {
    section,
    present = false,
    active = true,
    fill = false,
    onpos,
  }: {
    section: Section;
    present?: boolean;
    /** presentation: this slide is on screen (videos run) */
    active?: boolean;
    fill?: boolean;
    /** the divider was moved (document view saves it as the default position) */
    onpos?: (p: number) => void;
  } = $props();

  const mode = $derived(section.compare?.mode ?? 'slider');
  const a = $derived(section.media[0]);
  const b = $derived(section.media[1]);
  let pos = $state(0.5);
  $effect(() => {
    pos = section.compare?.pos ?? 0.5;
  });
  let box = $state<HTMLDivElement>();
  let va = $state<HTMLVideoElement>();
  let vb = $state<HTMLVideoElement>();
  let playing = $state(false);

  const ra = $derived(a ? resolver.get(a) : undefined);
  const rb = $derived(b ? resolver.get(b) : undefined);
  const ratio = $derived(ra?.file?.width && ra.file.height ? ra.file.width / ra.file.height : 16 / 9);

  function srcOf(r: typeof ra) {
    if (!r?.file) return '';
    if (r.file.kind === 'gif') return r.gif ?? '';
    if (r.file.kind === 'video') return (present ? r.present?.url : r.doc?.url) || r.doc?.url || '';
    return '';
  }

  function drag(e: PointerEvent) {
    const el = box!;
    el.setPointerCapture(e.pointerId);
    const set = (ev: PointerEvent) => {
      const r = el.getBoundingClientRect();
      pos = Math.min(1, Math.max(0, (ev.clientX - r.left) / r.width));
    };
    set(e);
    const up = () => {
      el.removeEventListener('pointermove', set);
      el.removeEventListener('pointerup', up);
      onpos?.(Math.round(pos * 1000) / 1000);
    };
    el.addEventListener('pointermove', set);
    el.addEventListener('pointerup', up);
  }

  // keep the two videos together
  function both(fn: (v: HTMLVideoElement) => void) {
    for (const v of [va, vb]) if (v) fn(v);
  }
  function toggle() {
    if (playing) both((v) => v.pause());
    else {
      if (va && vb) vb.currentTime = va.currentTime;
      both((v) => v.play().catch(() => {}));
    }
  }
  function onSeeked() {
    if (va && vb && Math.abs(va.currentTime - vb.currentTime) > 0.15) vb.currentTime = va.currentTime;
  }
  // drift correction while playing
  $effect(() => {
    if (!playing) return;
    const t = setInterval(() => {
      if (va && vb && !va.paused && Math.abs(va.currentTime - vb.currentTime) > 0.08) vb.currentTime = va.currentTime;
    }, 1000);
    return () => clearInterval(t);
  });
  $effect(() => {
    if (!present) return;
    if (active) {
      both((v) => (v.currentTime = 0));
      both((v) => v.play().catch(() => ((v.muted = true), v.play().catch(() => {}))));
    } else both((v) => v.pause());
  });
</script>

{#snippet media(m: MediaRef, r: typeof ra, which: 'a' | 'b')}
  {#if !r?.file}
    <div class="ph"></div>
  {:else if r.file.kind === 'image'}
    <img srcset={srcset(r.derived, r.file.width)} sizes={present ? '100vw' : '900px'} alt={m.caption || r.file.name} draggable="false" style={mediaStyle(m)} />
  {:else if srcOf(r)}
    <!-- svelte-ignore a11y_media_has_caption -->
    {#if which === 'a'}
      <video bind:this={va} style={mediaStyle(m)} src={srcOf(r)} poster={thumbUrl(r.derived, 1280)} playsinline preload="metadata" loop={r.file.kind === 'gif' || m.loop} muted={r.file.kind === 'gif' || m.muted} onplay={() => (playing = true)} onpause={() => (playing = false)} onseeked={onSeeked}></video>
    {:else}
      <video bind:this={vb} style={mediaStyle(m)} src={srcOf(r)} poster={thumbUrl(r.derived, 1280)} playsinline preload="metadata" loop={r.file.kind === 'gif' || m.loop} muted></video>
    {/if}
  {:else}
    <img src={thumbUrl(r.derived, 1280)} alt="" draggable="false" style={mediaStyle(m)} />
  {/if}
{/snippet}

{#if !a || !b}
  <div class="need muted"><Icon name="compare" size={20} /> Add two images or videos to compare them.</div>
{:else if mode === 'slider'}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="cmp slider" class:fill bind:this={box} style={fill ? '' : `aspect-ratio:${ratio}`} onpointerdown={drag}>
    <div class="layer">{@render media(a, ra, 'a')}</div>
    <div class="layer top" style="clip-path:inset(0 0 0 {pos * 100}%)">{@render media(b, rb, 'b')}</div>
    <div class="divider" style="left:{pos * 100}%"><span class="knob"><Icon name="compare" size={16} /></span></div>
    {#if a.caption}<span class="lbl l">{a.caption}</span>{/if}
    {#if b.caption}<span class="lbl r">{b.caption}</span>{/if}
  </div>
{:else}
  <div class="cmp sync" class:fill>
    <figure>{@render media(a, ra, 'a')}{#if a.caption}<figcaption>{a.caption}</figcaption>{/if}</figure>
    <figure>{@render media(b, rb, 'b')}{#if b.caption}<figcaption>{b.caption}</figcaption>{/if}</figure>
  </div>
{/if}
{#if (va || vb) && a && b}
  <div class="ctl" class:onslide={present}>
    <button class="btn sm" onclick={toggle}><Icon name={playing ? 'pause' : 'play'} size={13} /> {playing ? 'Pause both' : 'Play both'}</button>
  </div>
{/if}

<style>
  .cmp {
    position: relative;
    width: 100%;
    border-radius: 8px;
    overflow: hidden;
    background: #000;
    user-select: none;
  }
  .cmp.fill {
    height: 100%;
    background: transparent;
  }
  .slider {
    cursor: ew-resize;
    touch-action: none;
  }
  .layer {
    position: absolute;
    inset: 0;
  }
  .layer :global(img),
  .layer :global(video) {
    width: 100%;
    height: 100%;
    object-fit: contain;
    display: block;
    pointer-events: none;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    box-shadow: 0 0 6px #000a;
    pointer-events: none;
  }
  .knob {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: #fff;
    color: #111;
    display: grid;
    place-items: center;
    box-shadow: 0 2px 8px #0008;
  }
  .lbl {
    position: absolute;
    bottom: 10px;
    padding: 2px 8px;
    border-radius: 999px;
    background: #000a;
    color: #fff;
    font-size: 12px;
    pointer-events: none;
  }
  .lbl.l {
    left: 10px;
  }
  .lbl.r {
    right: 10px;
  }
  .sync {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    background: transparent;
  }
  .sync figure {
    margin: 0;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .sync :global(img),
  .sync :global(video) {
    width: 100%;
    flex: 1;
    min-height: 0;
    object-fit: contain;
    background: #000;
    border-radius: 6px;
    display: block;
  }
  figcaption {
    text-align: center;
    font-size: 0.85em;
    font-style: italic;
    opacity: 0.8;
    padding-top: 4px;
  }
  .ph {
    width: 100%;
    height: 100%;
    background: var(--bg-2);
  }
  .need {
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: center;
    padding: 30px;
    border: 1px dashed var(--line-strong);
    border-radius: 8px;
  }
  .ctl {
    display: flex;
    justify-content: center;
    margin-top: 6px;
  }
  .ctl.onslide {
    position: absolute;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
  }
</style>
