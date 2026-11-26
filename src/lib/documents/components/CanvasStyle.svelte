<script lang="ts">
  // Style panel of the canvas selection (right side): line and fill colours (palette,
  // custom, recent), line width, dashes, corners, opacity, connector route and arrowheads,
  // copy / paste style. Every change applies to the whole selection.
  import ColorInput from '../../components/ui/ColorInput.svelte';
  import { ink, PALETTE } from '../lib/canvas';
  import type { CanvasEditor } from '../lib/canvasEditor.svelte';
  import { dashOf, headsOf, HEADS } from '../lib/diagram/connectors';
  import { DEFAULT_RADIUS } from '../lib/diagram/shapes';
  import { HEAD_SHAPES } from '../lib/diagram/style';
  import type { ArrowItem, CanvasItem, Dash, Head, Route, ShapeItem } from '../lib/types';
  import Icon from './Icon.svelte';

  let { ed, onclose }: { ed: CanvasEditor; onclose: () => void } = $props();

  const items = $derived(ed.selItems);
  const isShape = (i: CanvasItem): i is ShapeItem => i.type === 'rect' || i.type === 'ellipse';
  const shapes = $derived(items.filter(isShape));
  const arrows = $derived(items.filter((i): i is ArrowItem => i.type === 'arrow'));
  const lined = $derived(items.filter((i) => isShape(i) || i.type === 'arrow' || i.type === 'path' || i.type === 'icon'));
  const filled = $derived(items.filter((i) => isShape(i) || i.type === 'note'));
  const rects = $derived(shapes.filter((s) => s.type === 'rect' && !s.shape));
  const first = $derived(lined[0] as (CanvasItem & { color: string }) | undefined);
  const firstFill = $derived(filled[0]);

  const WIDTHS = [1, 2, 3, 5, 8];
  const DASHES: { id: Dash; label: string }[] = [
    { id: 'solid', label: 'Solid' },
    { id: 'dashed', label: 'Dashed' },
    { id: 'dotted', label: 'Dotted' },
  ];
  const ROUTES: { id: Route; icon: string; label: string }[] = [
    { id: 'straight', icon: 'line', label: 'Straight' },
    { id: 'curved', icon: 'curve', label: 'Curved' },
    { id: 'elbow', icon: 'elbow', label: 'Elbow (right angles, goes around shapes)' },
  ];

  /** '#rrggbb' of a stored colour (for the custom colour field) */
  function hexOf(c: string | undefined, kind: 'ink' | 'paper'): string {
    if (!c) return '#000000';
    if (c.startsWith('#')) return c.slice(0, 7);
    if (c === 'ink') {
      // the theme's text colour
      const v = ed.vp ? getComputedStyle(ed.vp).getPropertyValue('--text').trim() : '';
      return /^#[0-9a-f]{6}$/i.test(v) ? v : '#000000';
    }
    const p = PALETTE.find((x) => x.key === c);
    return p ? (kind === 'ink' ? p.ink : p.paper) : '#000000';
  }

  const fillKey = (i: CanvasItem | undefined) => (!i ? undefined : i.type === 'note' ? i.color : isShape(i) ? (i.fillColor ?? (i.fill ? 'tint' : 'none')) : undefined);
  const widthOf = (i: CanvasItem | undefined) =>
    !i ? 3 : isShape(i) ? (i.strokeWidth ?? 3) : i.type === 'arrow' ? (i.width ?? 3) : i.type === 'path' ? i.width : i.type === 'icon' ? ((i.strokeWidth ?? 2) * 3) / 2 : 3;
  const dashOfItem = (i: CanvasItem | undefined): Dash => (!i ? 'solid' : isShape(i) ? (i.dash ?? 'solid') : i.type === 'arrow' ? dashOf(i) : 'solid');
  const opacity = $derived(((items[0] as { opacity?: number } | undefined)?.opacity ?? 1) * 100);

  function setLine(c: string) {
    ed.restyle((i) => {
      if (isShape(i) || i.type === 'arrow' || i.type === 'path' || i.type === 'icon') i.color = c;
    });
    ed.pushRecent(c);
  }
  function setFill(c: string) {
    ed.restyle((i) => {
      if (isShape(i)) {
        delete i.fill;
        if (c === 'none') i.fillColor = 'none';
        else i.fillColor = c;
      } else if (i.type === 'note') i.color = c;
    });
    ed.pushRecent(c);
    ed.fitNotes(items.map((i) => i.id));
  }
  function setWidth(w: number) {
    ed.restyle((i) => {
      if (isShape(i)) i.strokeWidth = w;
      else if (i.type === 'arrow') {
        if (w === 3) delete i.width;
        else i.width = w;
      } else if (i.type === 'path') i.width = w;
      else if (i.type === 'icon') i.strokeWidth = Math.round(((w * 2) / 3) * 100) / 100;
    });
  }
  function setDash(d: Dash) {
    ed.restyle((i) => {
      if (isShape(i)) {
        if (d === 'solid') delete i.dash;
        else i.dash = d;
      } else if (i.type === 'arrow') {
        delete i.dashed;
        if (d === 'solid') delete i.dash;
        else i.dash = d;
      }
    });
  }
  function setRadius(r: number) {
    ed.restyle((i) => {
      if (isShape(i) && i.type === 'rect' && !i.shape) i.radius = r;
    });
  }
  function setOpacity(o: number) {
    ed.restyle((i) => {
      const x = i as { opacity?: number };
      if (o >= 100) delete x.opacity;
      else x.opacity = Math.max(5, o) / 100;
    });
  }
  function setRoute(r: Route) {
    ed.setRoute(r);
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      if (r === 'straight') delete i.route;
      else {
        i.route = r;
        delete i.curve;
      }
    });
  }
  function setHead(which: 'start' | 'end', h: Head) {
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      const cur = headsOf(i);
      i.start = which === 'start' ? h : cur.start;
      i.end = which === 'end' ? h : cur.end;
      delete i.head;
    });
  }
  function swapEnds() {
    ed.restyle((i) => {
      if (i.type !== 'arrow') return;
      const h = headsOf(i);
      [i.from, i.to] = [i.to, i.from];
      i.start = h.end;
      i.end = h.start;
      delete i.head;
      if (i.labelAt !== undefined) i.labelAt = 1 - i.labelAt;
      if (i.curve) i.curve = -i.curve;
    });
  }
</script>

{#snippet head(h: Head)}
  {@const s = h === 'none' ? null : HEAD_SHAPES[h]}
  <svg width="26" height="14" viewBox="0 0 26 14" aria-hidden="true">
    <line x1="1" y1="7" x2={s ? 14 + s.refX : 25} y2="7" stroke="currentColor" stroke-width="2" />
    {#if s}
      <g transform="translate(14 2)">
        <path d={s.d} fill={s.fill ? 'currentColor' : 'none'} stroke={s.fill ? 'none' : 'currentColor'} stroke-width="1.6" />
      </g>
    {/if}
  </svg>
{/snippet}

<div class="cs">
  <header>
    <b>Style</b>
    <span class="grow"></span>
    <button class="icon-btn" title="Copy style (Ctrl+Alt+C)" onclick={() => ed.copyStyle()}><Icon name="brush" /></button>
    <button class="icon-btn" title="Paste style (Ctrl+Alt+V)" disabled={!ed.styleClip} onclick={() => ed.pasteStyle()}><Icon name="paste" /></button>
    <button class="icon-btn" title="Close" onclick={onclose}><Icon name="x" /></button>
  </header>

  {#if !lined.length && !filled.length && !items.length}
    <p class="muted">Select shapes, arrows, notes, drawings or icons to style them.</p>
  {/if}

  {#if lined.length}
    <div class="lbl">Line colour</div>
    <div class="sws">
      <button class="sw" class:on={first?.color === 'ink'} title="Default (text colour)" style="background:var(--text)" aria-label="Default colour" onclick={() => setLine('ink')}></button>
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={first?.color === p.key} title={p.label} style="background:{p.ink}" aria-label={p.label} onclick={() => setLine(p.key)}></button>
      {/each}
    </div>
    <div class="row">
      <div class="grow"><ColorInput label="Custom" value={hexOf(first?.color, 'ink')} onchange={setLine} /></div>
    </div>
  {/if}

  {#if filled.length}
    {@const fk = fillKey(firstFill)}
    <div class="lbl">Fill</div>
    <div class="sws">
      <button class="sw none" class:on={fk === 'none'} title="No fill" aria-label="No fill" onclick={() => setFill('none')}></button>
      {#each PALETTE as p (p.key)}
        <button class="sw" class:on={fk === p.key} title={p.label} style="background:{p.paper}" aria-label={p.label} onclick={() => setFill(p.key)}></button>
      {/each}
    </div>
    <div class="row">
      <div class="grow"><ColorInput label="Custom" value={fk && fk !== 'none' && fk !== 'tint' ? hexOf(fk, 'paper') : '#ffffff'} onchange={setFill} /></div>
    </div>
  {/if}

  {#if ed.recentColors.length && (lined.length || filled.length)}
    <div class="lbl">Recent colours</div>
    <div class="sws recent">
      {#each ed.recentColors as c (c)}
        <span class="pair">
          {#if lined.length}<button class="sw" style="background:{c}" title="Line {c}" aria-label="Line colour {c}" onclick={() => setLine(c)}></button>{/if}
          {#if filled.length}<button class="sw sq" style="background:{c}" title="Fill {c}" aria-label="Fill colour {c}" onclick={() => setFill(c)}></button>{/if}
        </span>
      {/each}
    </div>
  {/if}

  {#if lined.length}
    {@const w = widthOf(lined[0])}
    <div class="lbl">Line width</div>
    <div class="row">
      {#each WIDTHS as v (v)}
        <button class="seg wd" class:on={Math.abs(w - v) < 0.3} title="{v} px" onclick={() => setWidth(v)}><span style="height:{v}px;background:{ink(first?.color ?? 'ink')}"></span></button>
      {/each}
    </div>
    {#if shapes.length || arrows.length}
      {@const d = dashOfItem(shapes[0] ?? arrows[0])}
      <div class="row">
        {#each DASHES as x (x.id)}
          <button class="seg" class:on={d === x.id} onclick={() => setDash(x.id)}>
            <svg width="30" height="8" aria-hidden="true"><line x1="2" y1="4" x2="28" y2="4" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-dasharray={x.id === 'dashed' ? '6 4' : x.id === 'dotted' ? '0.1 5' : undefined} /></svg>
            {x.label}
          </button>
        {/each}
      </div>
    {/if}
  {/if}

  {#if rects.length}
    {@const r = rects[0].radius ?? DEFAULT_RADIUS}
    <div class="lbl">Corners <span class="val">{Math.round(r)}</span></div>
    <input type="range" min="0" max="80" step="1" value={r} style:--fill="{(r / 80) * 100}%" oninput={(e) => setRadius(+e.currentTarget.value)} />
  {/if}

  {#if arrows.length}
    {@const a = arrows[0]}
    {@const h = headsOf(a)}
    <div class="lbl">Connector</div>
    <div class="row">
      {#each ROUTES as r (r.id)}
        <button class="seg" class:on={(a.route ?? 'straight') === r.id} title={r.label} onclick={() => setRoute(r.id)}><Icon name={r.icon} size={14} /> {r.id === 'elbow' ? 'Elbow' : r.label}</button>
      {/each}
    </div>
    <div class="heads">
      <span class="hl">Start</span>
      {#each HEADS as x (x.id)}
        <button class="seg hd" class:on={h.start === x.id} title={x.label} onclick={() => setHead('start', x.id)}><span class="flip">{@render head(x.id)}</span></button>
      {/each}
      <span class="hl">End</span>
      {#each HEADS as x (x.id)}
        <button class="seg hd" class:on={h.end === x.id} title={x.label} onclick={() => setHead('end', x.id)}>{@render head(x.id)}</button>
      {/each}
    </div>
    <div class="row">
      <button class="btn sm ghost" title="Swap the two ends" onclick={swapEnds}><Icon name="replace" size={14} /> Swap ends</button>
    </div>
    <p class="tip">Drag an end onto a blue dot to fix it to that side. With a label, drag the label along the line.</p>
  {/if}

  {#if items.length}
    <div class="lbl">Opacity <span class="val">{Math.round(opacity)}%</span></div>
    <input type="range" min="5" max="100" step="5" value={opacity} style:--fill="{opacity}%" oninput={(e) => setOpacity(+e.currentTarget.value)} />
  {/if}
</div>

<style>
  .cs {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 2px;
    margin-bottom: 2px;
  }
  .grow {
    flex: 1;
    min-width: 0;
  }
  .lbl {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-3);
    margin-top: 6px;
    display: flex;
    justify-content: space-between;
  }
  .val {
    text-transform: none;
    letter-spacing: 0;
    color: var(--text-2);
  }
  .sws {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .pair {
    display: inline-flex;
    gap: 2px;
  }
  .sw.sq {
    border-radius: 5px;
  }
  .row {
    display: flex;
    gap: 4px;
    align-items: center;
    flex-wrap: wrap;
  }
  .seg {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .wd {
    width: 38px;
    justify-content: center;
  }
  .wd span {
    display: block;
    width: 22px;
    border-radius: 2px;
  }
  .heads {
    display: grid;
    grid-template-columns: 34px repeat(6, 1fr);
    gap: 3px;
    align-items: center;
  }
  .hl {
    font-size: 11.5px;
    color: var(--text-2);
  }
  .hd {
    padding: 0;
    min-width: 0;
    justify-content: center;
  }
  .flip {
    display: inline-flex;
    transform: scaleX(-1);
  }
  .tip {
    font-size: 11.5px;
    color: var(--text-3);
    margin: 2px 0 0;
    line-height: 1.35;
  }
  input[type='range'] {
    width: 100%;
  }
</style>
