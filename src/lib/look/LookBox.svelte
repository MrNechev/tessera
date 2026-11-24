<script lang="ts" module>
  import type { FrameStyle } from './looks.svelte';

  /** Frame margins as fractions of the framed photo's width. */
  const FRAME: Record<FrameStyle, { l: number; r: number; t: number; b: number; color: string }> = {
    polaroid: { l: 0.05, r: 0.05, t: 0.05, b: 0.2, color: '#ffffff' },
    instant: { l: 0.06, r: 0.06, t: 0.06, b: 0.24, color: '#fbfaf5' },
    mat: { l: 0.08, r: 0.08, t: 0.08, b: 0.08, color: '#ffffff' },
    film: { l: 0, r: 0, t: 0.1, b: 0.1, color: '#141218' },
    tape: { l: 0, r: 0, t: 0, b: 0, color: '#f7a8c4' },
  };

  export function frameMargins(style: FrameStyle | undefined) {
    return style ? FRAME[style] : { l: 0, r: 0, t: 0, b: 0, color: '' };
  }

  /** Aspect (w/h) of the framed photo, given the photo's own aspect. */
  export function framedAspect(aspect: number, style: FrameStyle | undefined): number {
    const m = frameMargins(style);
    return 1 / ((1 - m.l - m.r) / aspect + m.t + m.b);
  }
</script>

<script lang="ts">
  /**
   * Shows one story photo or video with its look's geometry: crop, quarter
   * turns, straighten, flips, mask shape, frame, border and shadow. Works in
   * any box: in the story (`fill` off: as wide as the column) or filling a
   * slide or card (`fill` on: contained or covering, via container units).
   * The media element itself comes from the parent through `media`, which
   * gets the style that places the crop.
   */
  import type { Snippet } from 'svelte';
  import { lookTransform, maskImage, type Crop, type Look } from './looks.svelte';

  interface Props {
    look: Look & { crop?: Crop };
    /** Width/height of the cropped, rotated media. */
    aspect: number;
    fill?: boolean;
    cover?: boolean;
    /** CSS filter (SVG look) for the media, or '' when the pixels already carry it. */
    filter?: string;
    media: Snippet<[string]>;
  }
  let { look, aspect, fill = false, cover = false, filter = '', media }: Props = $props();

  const m = $derived(frameMargins(look.frame));
  const outer = $derived(framedAspect(aspect, look.frame));
  const quarter = $derived(look.rotate === 90 || look.rotate === 270);
  /** Aspect of the unrotated crop (the box the media is clipped to). */
  const cropAspect = $derived(quarter ? 1 / aspect : aspect);
  const c = $derived(look.crop && look.crop.w > 0 && look.crop.h > 0 ? look.crop : { x: 0, y: 0, w: 1, h: 1 });
  const place = $derived(
    `position:absolute;left:${(-c.x / c.w) * 100}%;top:${(-c.y / c.h) * 100}%;width:${100 / c.w}%;height:${100 / c.h}%;max-width:none;object-fit:fill`,
  );
  const mask = $derived(look.mask && look.mask !== 'rect' ? maskImage(look.mask, aspect) : '');
  const transform = $derived(lookTransform(look, cropAspect));
  const outerSize = $derived(
    !fill ? 'width:100%' : cover ? `width:max(100cqw, calc(100cqh * ${outer}))` : `width:min(100cqw, calc(100cqh * ${outer}))`,
  );
</script>

<div
  class="lb-outer frame-{look.frame ?? 'none'}"
  class:fill
  class:shadow={look.shadow}
  style="aspect-ratio:{outer};{outerSize};--frame:{look.frameColor || m.color}"
>
  <div
    class="lb-inner"
    style="left:{m.l * 100}%;right:{m.r * 100}%;top:{m.t * outer * 100}%;bottom:{m.b * outer * 100}%;{mask ? `mask-image:url(${mask});-webkit-mask-image:url(${mask});` : ''}"
  >
    <div
      class="lb-rot"
      style="width:{quarter ? '100cqh' : '100cqw'};height:{quarter ? '100cqw' : '100cqh'};transform:translate(-50%,-50%) {transform};{filter ? `filter:${filter}` : ''}"
    >
      {@render media(place)}
    </div>
    {#if look.border && look.border.width > 0 && !mask}
      <div class="lb-border" style="border:calc({look.border.width} * min(100cqw, 100cqh)) solid {look.border.color}"></div>
    {/if}
  </div>
</div>

<style>
  .lb-outer {
    position: relative;
    background: var(--frame);
  }
  .lb-outer.frame-none,
  .lb-outer.frame-tape {
    background: transparent;
  }
  .lb-outer.fill {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
  }
  .lb-outer.shadow {
    filter: drop-shadow(0 6px 14px rgba(0, 0, 0, 0.35));
  }
  .lb-inner {
    position: absolute;
    overflow: hidden;
    container-type: size;
    mask-size: 100% 100%;
    -webkit-mask-size: 100% 100%;
  }
  .lb-rot {
    position: absolute;
    left: 50%;
    top: 50%;
    overflow: hidden;
  }
  .lb-rot > :global(img),
  .lb-rot > :global(video) {
    display: block;
  }
  .lb-border {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  /* film: sprocket holes in the top and bottom bands */
  .frame-film::before,
  .frame-film::after {
    content: '';
    position: absolute;
    left: 2%;
    right: 2%;
    height: 3.2%;
    background: repeating-linear-gradient(90deg, transparent 0 3%, rgba(255, 255, 255, 0.82) 3% 6%);
    border-radius: 2px;
    z-index: 1;
  }
  .frame-film::before {
    top: 3%;
  }
  .frame-film::after {
    bottom: 3%;
  }
  /* tape: a strip across the top edge */
  .frame-tape::before {
    content: '';
    position: absolute;
    left: 35%;
    width: 30%;
    top: -2%;
    aspect-ratio: 4 / 1;
    background: var(--frame);
    opacity: 0.85;
    transform: rotate(-3deg);
    z-index: 2;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  }
</style>
