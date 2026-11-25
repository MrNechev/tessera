<script lang="ts" module>
  // Remembered playback positions so a video that is unmounted (scrolled far away) and
  // mounted again continues where it was.
  const positions = new Map<string, number>();
</script>

<script lang="ts">
  import { api, fmtDuration, srcset, thumbUrl } from '../lib/api';
  import { resolver } from '../lib/resolver.svelte';
  import type { MediaRef, Resolved } from '../lib/types';
  import { viewport } from '../lib/viewport';
  import { autoplay, stopped } from '../lib/playback';
  import Icon from './Icon.svelte';
  import { fullAdjust, lookFilter, lookTransform, lookedUrl, needsPixels, shownAspect } from '../../look/looks.svelte';
  import LookBox, { framedAspect } from '../../look/LookBox.svelte';

  let {
    ref,
    mode = 'doc',
    sizes = '(max-width: 900px) 100vw, 860px',
    active = true,
    fill = false,
    instance = '',
    onrelink,
    relinkTo,
    onremove,
    onended,
    near: nearProp,
    controls = true,
    forceMute = false,
  }: {
    ref: MediaRef;
    /**
     * doc: lazily mounted by viewport; present: mounted by the slide (active = current slide);
     * canvas: mounted while `near`, plays like the document while `active` (visible)
     */
    mode?: 'doc' | 'present' | 'canvas';
    sizes?: string;
    active?: boolean;
    /** fill the parent box (presentation, gallery) instead of keeping the media's aspect ratio */
    fill?: boolean;
    instance?: string;
    onrelink?: () => void;
    relinkTo?: (to: { libraryId: string; path: string }) => void;
    onremove?: () => void;
    onended?: () => void;
    /** canvas / canvas presentation: the parent decides when the media is mounted */
    near?: boolean;
    controls?: boolean;
    /** play without sound (canvas presentation: only one video per frame gets sound) */
    forceMute?: boolean;
  } = $props();

  const r: Resolved | undefined = $derived(resolver.get(ref));
  const f = $derived(r?.file);
  const crop = $derived(ref.crop && ref.crop.w > 0 && ref.crop.h > 0 && (ref.crop.w < 0.999 || ref.crop.h < 0.999) ? ref.crop : null);
  const ratio = $derived(f && f.width && f.height ? shownAspect(f.width, f.height, crop, ref.rotate) : 16 / 9);
  // Crops, turns, masks and frames go through LookBox (works in any box and in every
  // browser); plain media keep the simple, cheaper markup.
  const geo = $derived(
    !!(crop || ref.rotate || ref.straighten || ref.frame || (ref.mask && ref.mask !== 'rect') || ref.border || ref.shadow),
  );
  // Looks that need real pixels (images only): the thumbnail is processed once.
  const pixels = $derived(f?.kind === 'image' && needsPixels(ref));
  let pixSrc = $state('');
  $effect(() => {
    if (!pixels || !r?.derived || !mounted) {
      pixSrc = '';
      return;
    }
    let live = true;
    void lookedUrl(thumbUrl(r.derived, mode === 'present' || fill ? 2560 : 1280), ref)
      .then((u) => live && (pixSrc = u))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  });
  /** The SVG part of the look; with processed pixels only the vignette is left. */
  const svgFilter = $derived(
    pixSrc ? lookFilter(ref.adjust?.vignette ? { ...fullAdjust(undefined), vignette: ref.adjust.vignette } : undefined) : lookFilter(ref.adjust),
  );
  const look = $derived([svgFilter ? `filter:${svgFilter}` : '', lookTransform({ flipX: ref.flipX, flipY: ref.flipY }, ratio) ? `transform:${lookTransform({ flipX: ref.flipX, flipY: ref.flipY }, ratio)}` : ''].filter(Boolean).join(';'));
  const elStyle = $derived(look || undefined);
  const fitMode = $derived(ref.fit || (fill ? 'contain' : undefined));
  /** In the story, --media-h (the editor's media size slider) caps the height. */
  const boxStyle = $derived.by(() => {
    const A = geo ? framedAspect(ratio, ref.frame) : ratio;
    const capW = mode === 'doc' ? `calc(var(--media-h, 100000px) * ${A})` : '';
    const mw = maxW && capW ? `min(${maxW}px, ${capW})` : maxW ? `${maxW}px` : capW;
    return `aspect-ratio:${A}${mw ? `;max-width:${mw};margin-inline:auto` : ''}`;
  });
  // never blow small images/GIFs (icons, spacers from saved web pages) up past their natural size
  const maxW = $derived(
    !fill && f && (f.kind === 'image' || f.kind === 'gif') && f.width > 0 && f.width < 720 ? Math.max(f.width, 96) : 0,
  );
  const src = $derived(mode === 'present' && nearProp === undefined ? r?.present : r?.doc);
  const proxy = $derived(src?.proxy);
  const needsProxy = $derived(!!proxy && proxy.status !== 'ready');
  const posKey = $derived(instance + '|' + ref.libraryId + '|' + ref.path);

  let near = $state(false);
  let visible = $state(false);
  let video = $state<HTMLVideoElement>();
  let gifFallback = $state(false);
  let requested = false;
  let suggestions = $state<{ libraryId: string; path: string }[] | null>(null);

  const mounted = $derived(nearProp !== undefined ? nearProp : mode === 'present' ? true : near);
  const playVisible = $derived(mode === 'canvas' ? active : visible);

  // Ask the server for the proxy as soon as the media is near the viewport.
  $effect(() => {
    if (mounted && f && proxy && (proxy.status === 'none' || proxy.status === 'failed') && !requested) {
      requested = true;
      if (proxy.status === 'none') {
        api.proxy(f.key, proxy.variant).then(() => resolver.markQueued(ref)).catch(() => {});
      }
    }
  });

  // Doc mode: GIF-videos and autoplay media play only while visible. Everything pauses
  // when it scrolls out of view (decoder time goes to what you are looking at).
  $effect(() => {
    const v = video;
    if (!v || mode === 'present') return;
    if (!playVisible) {
      if (!v.paused) v.pause();
      stopped(v);
    } else if (f?.kind === 'gif' || ref.autoplay) {
      autoplay(v).catch(() => {});
    }
    return () => stopped(v);
  });

  // Presentation: play on enter (with sound unless muted), pause and rewind on leave.
  $effect(() => {
    const v = video;
    if (!v || mode !== 'present') return;
    if (active) {
      if (f?.kind === 'video') {
        try {
          v.currentTime = ref.startAt || 0;
        } catch {}
      }
      if (f?.kind === 'gif' || ref.autoplay !== false) v.play().catch(() => {
        // autoplay with sound blocked: retry muted
        v.muted = true;
        v.play().catch(() => {});
      });
    } else {
      v.pause();
    }
  });

  function onLoadedMeta() {
    if (!video) return;
    const saved = positions.get(posKey);
    if (saved && mode !== 'present') video.currentTime = saved;
    else if (ref.startAt && video.currentTime < ref.startAt) video.currentTime = ref.startAt;
  }

  function onTimeUpdate() {
    if (!video) return;
    if (mode !== 'present') positions.set(posKey, video.currentTime);
    if (ref.endAt && video.currentTime >= ref.endAt) {
      if (ref.loop) video.currentTime = ref.startAt || 0;
      else {
        video.pause();
        onended?.();
      }
    }
  }

  async function findSuggestions() {
    const name = ref.path.split('/').pop()!;
    const res = await api.files({ q: name, limit: 10 });
    suggestions = res.items.filter((x) => x.name === name).map((x) => ({ libraryId: x.libraryId, path: x.path }));
  }

  $effect(() => {
    if (r && !r.found && suggestions === null && mode === 'doc') findSuggestions().catch(() => (suggestions = []));
  });

  function retry() {
    if (!f || !proxy) return;
    api.proxy(f.key, proxy.variant, true).then(() => resolver.markQueued(ref));
  }
</script>

{#snippet imageEl(style: string | undefined, fit: string | undefined)}
  <img
    src={pixSrc || undefined}
    srcset={pixSrc ? undefined : srcset(r!.derived, f!.width)}
    {sizes}
    width={f!.width || undefined}
    height={f!.height || undefined}
    alt={ref.caption || f!.name}
    loading={mode === 'doc' ? 'lazy' : 'eager'}
    draggable="false"
    decoding="async"
    {style}
    style:object-fit={fit}
  />
{/snippet}
{#snippet gifEl(style: string | undefined, fit: string | undefined)}
  {#if gifFallback}
    <img src={r!.original} alt={ref.caption || f!.name} decoding="async" {style} style:object-fit={fit} />
  {:else}
    <video
      bind:this={video}
      src={r!.gif}
      poster={thumbUrl(r!.derived, 1280)}
      loop
      muted
      playsinline
      preload="auto"
      disablepictureinpicture
      onerror={() => (gifFallback = true)}
      {style}
      style:object-fit={fit}
    ></video>
  {/if}
{/snippet}
{#snippet videoEl(style: string | undefined, fit: string | undefined)}
  <!-- svelte-ignore a11y_media_has_caption -->
  <video
    bind:this={video}
    src={src!.url}
    poster={thumbUrl(r!.derived, 1280)}
    {controls}
    playsinline
    preload={mode === 'present' ? 'auto' : 'metadata'}
    muted={ref.muted || forceMute || (mode !== 'present' && ref.autoplay)}
    loop={ref.loop}
    onloadedmetadata={onLoadedMeta}
    ontimeupdate={onTimeUpdate}
    onended={() => onended?.()}
    {style}
    style:object-fit={fit}
  ></video>
{/snippet}
{#snippet framed(el: typeof imageEl)}
  <LookBox look={ref} aspect={ratio} {fill} cover={ref.fit === 'cover'} filter={svgFilter}>
    {#snippet media(place: string)}{@render el(place, 'fill')}{/snippet}
  </LookBox>
{/snippet}

<div
  class="media"
  class:fill
  class:cropped={!!crop}
  class:geo
  class:missing={r && !r.found}
  style={fill ? '' : boxStyle}
  use:viewport={{ enabled: mode === 'doc', onNear: (v) => (near = v), onVisible: (v) => (visible = v) }}
>
  {#if !r}
    <div class="ph"></div>
  {:else if !r.found}
    <div class="miss">
      <Icon name="alert" size={22} />
      <div class="mt">Missing media</div>
      <code>{ref.libraryId}:{ref.path}</code>
      {#if suggestions?.length}
        <div class="sugg">
          Found a file with the same name:
          {#each suggestions as s (s.path)}
            <button class="btn sm" onclick={() => relinkTo?.(s)}><Icon name="link" size={13} /> {s.path}</button>
          {/each}
        </div>
      {/if}
      {#if mode === 'doc'}
        <div class="row">
          {#if onrelink}<button class="btn sm" onclick={onrelink}><Icon name="link" size={14} /> Relink…</button>{/if}
          {#if onremove}<button class="btn sm danger" onclick={onremove}><Icon name="trash" size={14} /> Remove</button>{/if}
        </div>
      {/if}
    </div>
  {:else if !mounted}
    <img class="poster" src={thumbUrl(r.derived, 320)} alt="" decoding="async" draggable="false" style={geo ? undefined : look || undefined} />
  {:else if f?.kind === 'image'}
    {#if geo}{@render framed(imageEl)}{:else}{@render imageEl(elStyle, fitMode)}{/if}
  {:else if f?.kind === 'gif'}
    {#if geo}{@render framed(gifEl)}{:else}{@render gifEl(elStyle, fitMode)}{/if}
  {:else if src?.url}
    {#if geo}{@render framed(videoEl)}{:else}{@render videoEl(elStyle, fitMode)}{/if}
    {#if needsProxy && mode === 'doc' && proxy?.variant === 'preview'}
      <span class="tag" title="A lighter preview copy is being made for smoother scrolling; playing the original meanwhile">preview {proxy.status === 'running' ? Math.round((proxy.progress ?? 0) * 100) + '%' : 'queued'}</span>
    {/if}
  {:else}
    <img class="poster dim" src={thumbUrl(r.derived, 1280)} alt="" decoding="async" />
    <div class="conv">
      {#if proxy?.status === 'failed'}
        <Icon name="alert" />
        <div>Conversion failed</div>
        <small>{proxy.error}</small>
        <button class="btn sm" onclick={retry}>Retry</button>
      {:else if proxy}
        <div class="spinner"></div>
        <div>Making a browser-compatible copy{proxy.status === 'running' ? ` · ${Math.round((proxy.progress ?? 0) * 100)}%` : ' · queued'}</div>
        <small>{f?.vcodec?.toUpperCase()} {f?.duration ? '· ' + fmtDuration(f.duration) : ''}</small>
      {:else}
        <Icon name="alert" />
        <div>Can't read this video</div>
        <small>{f?.probeError}</small>
      {/if}
    </div>
  {/if}
</div>

<style>
  .media {
    position: relative;
    width: 100%;
    background: var(--bg-2);
    border-radius: 8px;
    overflow: hidden;
    contain: layout paint;
  }
  .media.fill {
    height: 100%;
    background: transparent;
    border-radius: 0;
  }

  .media.geo {
    background: transparent;
    overflow: visible;
    contain: none;
  }
  .media.geo.fill {
    container-type: size;
    overflow: hidden;
  }
  .media.missing {
    aspect-ratio: auto !important;
    min-height: 160px;
  }
  .media > img,
  .media > video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .media.fill > img,
  .media.fill > video {
    object-fit: contain;
  }
  .media > video {
    background: #000;
  }
  .media.fill > video {
    background: transparent;
  }
  .poster.dim {
    opacity: 0.35;
    filter: blur(2px);
  }
  .ph {
    position: absolute;
    inset: 0;
  }
  .conv {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
    padding: 16px;
    color: var(--text);
    text-shadow: 0 1px 3px #000;
  }
  .conv small {
    color: var(--text-2);
    max-width: 90%;
  }
  .spinner {
    width: 26px;
    height: 26px;
    border: 3px solid #fff3;
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .tag {
    position: absolute;
    top: 8px;
    left: 8px;
    font-size: 11px;
    padding: 1px 7px;
    border-radius: 999px;
    background: #000a;
    color: #fff;
    pointer-events: none;
  }
  .miss {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 22px;
    min-height: 160px;
    text-align: center;
    color: var(--accent);
    border: 1px dashed var(--accent);
    border-radius: 8px;
    background: var(--accent-soft);
  }
  .mt {
    font-weight: 600;
  }
  .miss code {
    font: 12px var(--mono);
    color: var(--text-2);
    word-break: break-all;
  }
  .sugg {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--text-2);
    align-items: center;
  }
  .row {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }
</style>
