<script lang="ts">
  // The canvas contents in world coordinates under one CSS transform (pan/zoom).
  // Only what intersects `renderRect` is in the DOM; media is mounted inside `mountRect`
  // and plays inside `playRect` (both updated when the view settles, not on every frame).
  import { arrowPath as arrowGeom, intersects, ink, isBox, isConnector, paper, pathPoints, smoothPath, targets, zOf, type View } from '../lib/canvas';
  import { resolver } from '../lib/resolver.svelte';
  import type { ArrowItem, BoxItem, Doc, Head, IconItem, PathItem, Rect, ShapeItem, StickerItem } from '../lib/types';
  import { DEFAULT_RADIUS, outlineExtra, outlineKind, outlinePath, textBox } from '../lib/diagram/shapes';
  import { dashArray, dashOf, headsOf } from '../lib/diagram/connectors';
  import { fillOf, HEAD_SHAPES, markerId, markerScale, shapeTextColor } from '../lib/diagram/style';
  import { iconPath } from '../lib/diagram/icons';
  import { stickers } from '../../stores/stickers.svelte';
  import { lookCss } from '../../text/styles';
  import CanvasCard from './CanvasCard.svelte';

  let {
    doc,
    view,
    renderRect,
    mountRect,
    playRect,
    lod,
    moving = false,
    still = false,
    present = false,
    selected = new Set<string>(),
    editing = null,
    outlineFrames = false,
    concealed = new Set<string>(),
    editor = false,
    editingCard = null,
    oncardtext,
  }: {
    doc: Doc;
    view: View;
    renderRect: Rect;
    mountRect: Rect | null;
    playRect: Rect | null;
    lod: number;
    /** pan/zoom in progress: let the compositor scale a cached layer */
    moving?: boolean;
    still?: boolean;
    present?: boolean;
    selected?: Set<string>;
    editing?: string | null;
    /** presentation overview: show where the frames are */
    outlineFrames?: boolean;
    /** canvas presentation: items waiting for their reveal step */
    concealed?: Set<string>;
    /** the canvas editor (shows reveal badges and locks) */
    editor?: boolean;
    /** section card whose text is edited in place */
    editingCard?: string | null;
    oncardtext?: (id: string, field: 'title' | 'text', value: any) => void;
  } = $props();

  const items = $derived(doc.canvas?.items ?? []);
  const boxes = $derived(items.filter(isBox) as BoxItem[]);
  const frames = $derived(boxes.filter((b) => b.type === 'frame'));
  const shapes = $derived(boxes.filter((b): b is ShapeItem => b.type === 'rect' || b.type === 'ellipse'));
  const paths = $derived(boxes.filter((b): b is PathItem => b.type === 'path'));
  const notes = $derived(boxes.filter((b) => b.type === 'note'));
  const stickerItems = $derived(boxes.filter((b): b is StickerItem => b.type === 'sticker'));
  const icons = $derived(boxes.filter((b): b is IconItem => b.type === 'icon'));
  // the catalogue (file names) is loaded once per session; until then stickers are empty boxes
  $effect(() => {
    if (stickerItems.length && !stickers.loaded) void stickers.load();
  });
  const stickerUrl = (id: string) => (stickers.loaded ? stickers.url(id) : '');
  function stickerTransform(st: StickerItem) {
    const t: string[] = [];
    if (st.rotation) t.push(`rotate(${st.rotation}deg)`);
    if (st.flipX || st.flipY) t.push(`scale(${st.flipX ? -1 : 1},${st.flipY ? -1 : 1})`);
    return t.length ? `transform:${t.join(' ')}` : '';
  }
  const arrows = $derived(items.filter((i) => i.type === 'arrow') as ArrowItem[]);
  const cards = $derived(doc.sections.map((s, i) => ({ s, i })).filter(({ s }) => s.canvas && !s.canvas.hidden));
  const tg = $derived(targets(doc));
  const iz = $derived(1 / lod);

  const visibleCards = $derived(cards.filter(({ s }) => intersects(s.canvas!, renderRect)));

  // canvas presentation: only the first card with a video in the frame plays with sound
  const soundCard = $derived.by(() => {
    if (!present || !playRect) return null;
    for (const { s } of visibleCards) {
      if (!intersects(s.canvas!, playRect)) continue;
      if (s.media.some((m) => resolver.get(m)?.file?.kind === 'video')) return s.id;
    }
    return null;
  });

  function arrowPath(a: ArrowItem) {
    return arrowGeom(a, tg);
  }
  const op = (x: { opacity?: number }) => (x.opacity !== undefined && x.opacity < 1 ? `opacity:${x.opacity};` : '');
  // one marker per head kind, colour and line width in use
  const markers = $derived.by(() => {
    const m = new Map<string, { head: Exclude<Head, 'none'>; color: string; w: number }>();
    for (const a of arrows) {
      const h = headsOf(a);
      const w = a.width ?? 3;
      for (const head of [h.start, h.end]) if (head !== 'none') m.set(markerId(head, a.color, w), { head, color: a.color, w });
    }
    return [...m.entries()];
  });
  const flags = (x: { reveal?: number; locked?: boolean } | undefined) => x ?? {};
</script>

<div class="world" class:moving class:present class:outline={outlineFrames} style="transform:translate({view.tx}px,{view.ty}px) scale({view.z});--iz:{iz}">
  {#each frames as f (f.id)}
    {#if intersects(f, renderRect)}
      <div class="frame" class:sel={selected.has(f.id)} class:concealed={concealed.has(f.id)} data-cid={f.id} data-kind="frame" style="z-index:{zOf(f, 'frame')};left:{f.x}px;top:{f.y}px;width:{f.w}px;height:{f.h}px">
        <div class="ftitle" data-cid={f.id} data-kind="frame">{outlineFrames ? `${frames.indexOf(f) + 1}. ` : ''}{f.title || 'Frame'}</div>
      </div>
    {/if}
  {/each}

  {#each shapes as b (b.id)}
    {#if intersects(b, renderRect)}
      {@const k = outlineKind(b)}
      {@const sw = b.strokeWidth ?? 3}
      {@const tb = textBox(k, b.w, b.h)}
      {@const extra = outlineExtra(k, b.w, b.h, sw)}
      <!-- the outline is an svg path (hit-tested itself, so corners of a diamond stay clickable-through) -->
      <div
        class="shape {b.type}"
        class:concealed={concealed.has(b.id)}
        data-cid={b.id}
        data-kind="shape"
        style="z-index:{zOf(b, b.type)};left:{b.x}px;top:{b.y}px;width:{b.w}px;height:{b.h}px;font-size:{b.size || 24}px;{op(b)}"
      >
        <svg class="outline" width={b.w} height={b.h} viewBox="0 0 {b.w} {b.h}">
          <path
            d={outlinePath(k, b.w, b.h, sw, b.radius ?? DEFAULT_RADIUS)}
            fill={fillOf(b)}
            stroke={sw > 0 ? ink(b.color) : 'none'}
            stroke-width={sw}
            stroke-dasharray={dashArray(b.dash ?? 'solid', sw)}
          />
          {#if extra}<path d={extra} fill="none" stroke={ink(b.color)} stroke-width={sw} />{/if}
        </svg>
        {#if b.text && editing !== b.id}
          <div class="stext" style="left:{tb.x}px;top:{tb.y}px;width:{tb.w}px;height:{tb.h}px;color:{shapeTextColor(b)};text-align:{b.align ?? 'center'};{lookCss(b.look)}">
            <span>{b.text}</span>
          </div>
        {/if}
        {#if editor && (b.reveal || b.locked)}<span class="flag">{#if b.locked}🔒{/if}{#if b.reveal}{b.reveal}{/if}</span>{/if}
      </div>
    {/if}
  {/each}

  {#each icons as ic (ic.id)}
    {#if intersects(ic, renderRect)}
      <div
        class="icon"
        class:concealed={concealed.has(ic.id)}
        data-cid={ic.id}
        data-kind="icon"
        style="z-index:{zOf(ic, 'icon')};left:{ic.x}px;top:{ic.y}px;width:{ic.w}px;height:{ic.h}px;{op(ic)}"
      >
        <svg viewBox="0 0 24 24" width="100%" height="100%" fill="none" stroke={ink(ic.color)} stroke-width={ic.strokeWidth ?? 2} stroke-linecap="round" stroke-linejoin="round"
          ><path d={iconPath(ic.icon)} /></svg
        >
        {#if editor && (ic.reveal || ic.locked)}<span class="flag">{#if ic.locked}🔒{/if}{#if ic.reveal}{ic.reveal}{/if}</span>{/if}
      </div>
    {/if}
  {/each}

  {#each visibleCards as { s, i } (s.id)}
    <!-- media is mounted once the card is big enough on screen to be worth decoding -->
    {@const big = s.canvas!.w * lod >= 240}
    <CanvasCard
      section={s}
      index={i}
      z={zOf(s.canvas!, 'card')}
      {lod}
      {still}
      {present}
      selected={selected.has(s.id)}
      mounted={!still && big && !!mountRect && intersects(s.canvas!, mountRect)}
      playing={!still && big && !!playRect && intersects(s.canvas!, playRect)}
      soundFirst={soundCard === s.id}
      concealed={concealed.has(s.id)}
      flags={editor ? flags(s.canvas) : {}}
      editing={editingCard === s.id}
      ontext={oncardtext ? (f, v) => oncardtext(s.id, f, v) : undefined}
    />
  {/each}

  {#each notes as n (n.id)}
    {#if n.type === 'note' && intersects(n, renderRect)}
      {@const pill = n.color === 'none' && !!n.look?.background}
      <div
        class="note"
        class:free={n.color === 'none'}
        class:styled={!!n.look}
        class:concealed={concealed.has(n.id)}
        data-cid={n.id}
        data-kind="note"
        style="z-index:{zOf(n, 'note')};left:{n.x}px;top:{n.y}px;width:{n.w}px;height:{n.h}px;background:{paper(n.color)};font-size:{n.size}px;text-align:{n.align || 'left'};{lookCss(n.look, { pill })};{op(n)}"
      >
        {#if editing !== n.id}{n.text}{/if}
        {#if !n.text && editing !== n.id}<span class="ph">{n.color === 'none' ? 'Text' : 'Note'}</span>{/if}
        {#if editor && (n.reveal || n.locked)}<span class="flag">{#if n.locked}🔒{/if}{#if n.reveal}{n.reveal}{/if}</span>{/if}
      </div>
    {/if}
  {/each}

  {#each stickerItems as st (st.id)}
    {#if intersects(st, renderRect)}
      <!-- rotated on the box itself, so hit-testing (elementsFromPoint) follows the turned sticker -->
      <div
        class="sticker"
        class:concealed={concealed.has(st.id)}
        data-cid={st.id}
        data-kind="sticker"
        style="z-index:{zOf(st, 'sticker')};left:{st.x}px;top:{st.y}px;width:{st.w}px;height:{st.h}px;{stickerTransform(st)};{op(st)}"
      >
        {#if stickerUrl(st.sticker)}<img src={stickerUrl(st.sticker)} alt="" draggable="false" />{/if}
        {#if editor && (st.reveal || st.locked)}<span class="flag">{#if st.locked}🔒{/if}{#if st.reveal}{st.reveal}{/if}</span>{/if}
      </div>
    {/if}
  {/each}

  <!-- each drawing in its own svg, so it can be stacked between cards and notes -->
  {#each paths as p (p.id)}
    {#if intersects(p, renderRect)}
      {@const d = smoothPath(pathPoints(p))}
      <svg class="inks" width="1" height="1" style="z-index:{zOf(p, 'path')};{op(p)}">
        <g class="path" class:sel={selected.has(p.id)} class:hl={p.highlighter} class:concealed={concealed.has(p.id)}>
          <path class="hit" {d} data-cid={p.id} data-kind="path" style="stroke-width:max({p.width + 10}px, calc(14px * var(--iz)))" />
          <path class="stroke" {d} stroke={ink(p.color)} stroke-width={p.width} />
        </g>
      </svg>
    {/if}
  {/each}

  <svg class="arrows" width="1" height="1">
    <defs>
      {#each markers as [id, m] (id)}
        {@const hs = HEAD_SHAPES[m.head]}
        {@const k = markerScale(m.w)}
        <marker {id} viewBox="-1 -1 12 12" refX={hs.refX} refY="5" markerWidth={k * 1.2} markerHeight={k * 1.2} orient="auto-start-reverse" markerUnits="strokeWidth">
          <path d={hs.d} fill={hs.fill ? ink(m.color) : 'none'} stroke={hs.fill ? 'none' : ink(m.color)} stroke-width={hs.fill ? 0 : 1.6} stroke-linecap="round" stroke-linejoin="round" />
        </marker>
      {/each}
    </defs>
    {#each arrows as a (a.id)}
      {@const p = arrowPath(a)}
      {@const h = headsOf(a)}
      {@const w = a.width ?? 3}
      <g class="arrow" class:sel={selected.has(a.id)} class:concealed={concealed.has(a.id)} style={op(a)}>
        <path class="hit" d={p.d} data-cid={a.id} data-kind="arrow" style="stroke-width:max({w + 10}px, calc(16px * var(--iz)))" />
        <path
          class="line"
          d={p.d}
          stroke={ink(a.color)}
          style="stroke-width:{w}px"
          stroke-dasharray={dashArray(dashOf(a), w)}
          marker-end={h.end !== 'none' ? `url(#${markerId(h.end, a.color, w)})` : undefined}
          marker-start={h.start !== 'none' ? `url(#${markerId(h.start, a.color, w)})` : undefined}
        />
        {#if a.text}
          {#if isConnector(a) || a.labelAt !== undefined}
            <text class="onl" x={p.label.x} y={p.label.y} text-anchor="middle" dominant-baseline="central" fill={ink(a.color)}>{a.text}</text>
          {:else}
            <text x={p.label.x} y={p.label.y - 10} text-anchor="middle" fill={ink(a.color)}>{a.text}</text>
          {/if}
        {/if}
      </g>
    {/each}
  </svg>
</div>

<style>
  .world {
    position: absolute;
    left: 0;
    top: 0;
    width: 0;
    height: 0;
    transform-origin: 0 0;
  }
  .world.moving {
    will-change: transform;
  }
  .frame {
    position: absolute;
    border: 2px solid var(--line-strong);
    border-radius: 4px;
    background: color-mix(in srgb, var(--surface) 35%, transparent);
  }
  /* frames are the camera's stops, not something to look at while presenting */
  .present .frame {
    border-color: transparent;
    background: none;
  }
  .present .ftitle {
    display: none;
  }
  .present.outline .frame {
    border: calc(2px * var(--iz)) dashed var(--accent);
  }
  .present.outline .ftitle {
    display: block;
    color: var(--accent);
    font-size: calc(15px * var(--iz));
  }
  .frame.sel {
    border-color: var(--accent);
  }
  .ftitle {
    position: absolute;
    left: 0;
    bottom: 100%;
    max-width: 100%;
    padding-bottom: calc(4px * var(--iz));
    font-size: calc(13px * var(--iz));
    font-weight: 600;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    cursor: default;
  }
  .shape {
    position: absolute;
    color: var(--text);
    line-height: 1.25;
    pointer-events: none;
  }
  .shape .outline {
    position: absolute;
    left: 0;
    top: 0;
    overflow: visible;
  }
  .shape .outline path:first-child {
    pointer-events: all;
  }
  .stext {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    pointer-events: none;
  }
  .stext span {
    width: 100%;
  }
  .icon {
    position: absolute;
  }
  .icon svg {
    display: block;
    overflow: visible;
  }
  .note {
    position: absolute;
    padding: 0.55em 0.65em;
    color: #1d1b22;
    border-radius: 4px;
    box-shadow: 0 4px 14px #0003;
    line-height: 1.3;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    overflow: hidden;
    font-family: var(--ui);
  }
  .note.free {
    box-shadow: none;
    color: var(--text);
    padding: 0.1em 0.15em;
  }
  .note .ph {
    opacity: 0.4;
  }
  /* styled free text hugs its text: let outlines and shadows spill over the box */
  .note.free.styled {
    overflow: visible;
  }
  .sticker {
    position: absolute;
  }
  .sticker img {
    display: block;
    width: 100%;
    height: 100%;
    pointer-events: none;
    user-select: none;
  }
  .arrows,
  .inks {
    position: absolute;
    left: 0;
    top: 0;
    overflow: visible;
    pointer-events: none;
  }
  .arrows {
    z-index: 1000000;
  }
  .arrow .line {
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .arrow .hit {
    fill: none;
    stroke: transparent;
    pointer-events: stroke;
  }
  .arrow.sel .line {
    filter: drop-shadow(0 0 3px var(--accent));
  }
  .concealed {
    opacity: 0 !important;
    pointer-events: none;
  }
  .frame,
  .shape,
  .icon,
  .note,
  .sticker,
  .arrow,
  .path {
    transition: opacity 0.35s;
  }
  .flag {
    position: absolute;
    top: calc(-10px * var(--iz));
    right: calc(-10px * var(--iz));
    min-width: calc(20px * var(--iz));
    height: calc(20px * var(--iz));
    padding: 0 calc(4px * var(--iz));
    border-radius: calc(10px * var(--iz));
    background: var(--accent);
    color: var(--accent-ink);
    font: 700 calc(12px * var(--iz)) / calc(20px * var(--iz)) var(--ui);
    text-align: center;
    pointer-events: none;
  }
  .path .stroke {
    fill: none;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .path.hl .stroke {
    opacity: 0.38;
    stroke-linecap: square;
  }
  .path .hit {
    fill: none;
    stroke: transparent;
    pointer-events: stroke;
  }
  .path.sel .stroke {
    filter: drop-shadow(0 0 3px var(--accent));
  }
  .arrow text {
    font: 600 18px var(--ui);
    paint-order: stroke;
    stroke: var(--bg);
    stroke-width: 5px;
  }
</style>
