<script lang="ts">
  /**
   * The Paint viewport: shows the document's composite at the current zoom
   * over a checkerboard, with the selection's marching ants, the pixel grid
   * at high zoom, the brush outline and the tools' own overlays. Turns
   * pointer input into image coordinates for the tools; wheel, pinch,
   * Space-drag and the middle button pan and zoom.
   */
  import { onMount } from 'svelte';
  import { maskEdges } from '../lib/mask';
  import { toImage, zoomAt } from '../lib/geom';
  import { paint } from '../state.svelte';
  import { ALT_PICKS, SIZED, toolFor, type PointerInfo, type Tool } from '../tools.svelte';
  import { fitTo } from '../session.svelte';

  let host: HTMLDivElement | undefined = $state();
  let canvas: HTMLCanvasElement | undefined = $state();
  let vw = $state(0);
  let vh = $state(0);
  let frame = 0;
  let spaceDown = $state(false);
  let panning: { sx: number; sy: number; x: number; y: number; id: number } | null = null;
  let active: { tool: Tool; id: number } | null = null;
  let hover = $state<{ sx: number; sy: number } | null>(null);
  let ants = 0;
  let edges: { rev: number; path: Path2D | null } = { rev: -1, path: null };
  const touches = new Map<number, { x: number; y: number }>();
  let pinch: { dist: number; zoom: number; cx: number; cy: number; view: typeof paint.view } | null = null;

  const doc = $derived(paint.doc);

  export function fit(upscale = false) {
    fitTo(vw, vh, upscale);
  }

  function request() {
    if (!frame) frame = requestAnimationFrame(draw);
  }

  // Redraw when the document changes or the view moves.
  $effect(() => {
    const d = doc;
    if (!d) return;
    const off = d.onChange(() => request());
    request();
    return off;
  });
  $effect(() => {
    void paint.view;
    void paint.tool;
    void paint.opts.grid;
    void paint.crop;
    void paint.textEdit;
    request();
  });
  $effect(() => {
    if (doc && paint.fitPending && vw > 0 && vh > 0) {
      paint.fitPending = false;
      fit();
    }
  });

  // Marching ants: animate while something is selected.
  $effect(() => {
    void doc?.selRev;
    if (!doc?.selection && !doc?.floating) return;
    const t = setInterval(() => {
      ants = (ants + 1) % 16;
      request();
    }, 140);
    return () => clearInterval(t);
  });

  let checker: CanvasPattern | null = null;
  function checkerPattern(c: CanvasRenderingContext2D): CanvasPattern | null {
    if (checker) return checker;
    const t = document.createElement('canvas');
    t.width = 16;
    t.height = 16;
    const g = t.getContext('2d')!;
    g.fillStyle = '#ffffff';
    g.fillRect(0, 0, 16, 16);
    g.fillStyle = '#d9d6e0';
    g.fillRect(0, 0, 8, 8);
    g.fillRect(8, 8, 8, 8);
    checker = c.createPattern(t, 'repeat');
    return checker;
  }

  function selectionPath(): Path2D | null {
    const d = doc;
    if (!d?.selection) return null;
    if (edges.rev !== d.selRev) {
      const e = maskEdges(d.selection, d.width, d.height);
      const p = new Path2D();
      for (let i = 0; i < e.length; i += 4) {
        p.moveTo(e[i], e[i + 1]);
        p.lineTo(e[i + 2], e[i + 3]);
      }
      edges = { rev: d.selRev, path: p };
    }
    return edges.path;
  }

  function draw() {
    frame = 0;
    const c = canvas?.getContext('2d');
    const d = doc;
    if (!c || !canvas) return;
    const dpr = window.devicePixelRatio || 1;
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.clearRect(0, 0, canvas.width, canvas.height);
    if (!d) return;
    d.flushComposite();
    const v = paint.view;
    c.setTransform(dpr, 0, 0, dpr, 0, 0);
    const iw = d.width * v.zoom;
    const ih = d.height * v.zoom;
    // drop shadow + checkerboard for transparency
    c.save();
    c.shadowColor = 'rgba(0,0,0,0.35)';
    c.shadowBlur = 18;
    c.fillStyle = '#fff';
    c.fillRect(v.x, v.y, iw, ih);
    c.restore();
    const pat = checkerPattern(c);
    if (pat) {
      c.fillStyle = pat;
      c.fillRect(v.x, v.y, iw, ih);
    }
    c.imageSmoothingEnabled = v.zoom < 2;
    c.imageSmoothingQuality = 'high';
    c.drawImage(d.composite, v.x, v.y, iw, ih);
    c.imageSmoothingEnabled = true;

    // pixel grid at high zoom
    if (paint.opts.grid && v.zoom >= 10) {
      const x0 = Math.max(0, Math.floor(-v.x / v.zoom));
      const y0 = Math.max(0, Math.floor(-v.y / v.zoom));
      const x1 = Math.min(d.width, Math.ceil((vw - v.x) / v.zoom));
      const y1 = Math.min(d.height, Math.ceil((vh - v.y) / v.zoom));
      c.beginPath();
      for (let x = x0; x <= x1; x++) {
        const sx = Math.round(v.x + x * v.zoom) + 0.5;
        c.moveTo(sx, v.y + y0 * v.zoom);
        c.lineTo(sx, v.y + y1 * v.zoom);
      }
      for (let y = y0; y <= y1; y++) {
        const sy = Math.round(v.y + y * v.zoom) + 0.5;
        c.moveTo(v.x + x0 * v.zoom, sy);
        c.lineTo(v.x + x1 * v.zoom, sy);
      }
      c.strokeStyle = 'rgba(128,128,128,0.35)';
      c.lineWidth = 1;
      c.stroke();
    }

    // selection: marching ants (moving along with floating pixels)
    const path = selectionPath();
    if (path) {
      const f = d.floating;
      const ox = f ? f.x - f.maskAt.x : 0;
      const oy = f ? f.y - f.maskAt.y : 0;
      c.save();
      c.translate(v.x + ox * v.zoom, v.y + oy * v.zoom);
      c.scale(v.zoom, v.zoom);
      c.lineWidth = 1 / v.zoom;
      c.strokeStyle = '#ffffff';
      c.stroke(path);
      c.setLineDash([4 / v.zoom, 4 / v.zoom]);
      c.lineDashOffset = -ants / v.zoom;
      c.strokeStyle = '#111111';
      c.stroke(path);
      c.restore();
    }

    toolFor(paint.tool).overlay?.(c, v);

    // brush outline
    if (hover && SIZED.includes(paint.tool) && !spaceDown) {
      const r = (paint.size / 2) * v.zoom;
      c.save();
      c.beginPath();
      if (paint.tool === 'pencil') {
        const p = toImage(v, hover.sx, hover.sy);
        const s = paint.size;
        const x = Math.floor(p.x) - Math.floor(s / 2);
        const y = Math.floor(p.y) - Math.floor(s / 2);
        c.rect(v.x + x * v.zoom, v.y + y * v.zoom, s * v.zoom, s * v.zoom);
      } else c.arc(hover.sx, hover.sy, Math.max(1, r), 0, Math.PI * 2);
      c.lineWidth = 1;
      c.strokeStyle = 'rgba(255,255,255,0.9)';
      c.stroke();
      c.setLineDash([3, 3]);
      c.strokeStyle = 'rgba(0,0,0,0.8)';
      c.stroke();
      c.restore();
    }
  }

  function resize() {
    if (!host || !canvas) return;
    const r = host.getBoundingClientRect();
    vw = r.width;
    vh = r.height;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(r.width * dpr));
    canvas.height = Math.max(1, Math.round(r.height * dpr));
    canvas.style.width = `${r.width}px`;
    canvas.style.height = `${r.height}px`;
    request();
  }

  onMount(() => {
    const ro = new ResizeObserver(resize);
    ro.observe(host!);
    resize();
    return () => {
      ro.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  });

  function info(e: PointerEvent): PointerInfo {
    const r = host!.getBoundingClientRect();
    const sx = e.clientX - r.left;
    const sy = e.clientY - r.top;
    const p = toImage(paint.view, sx, sy);
    return {
      x: p.x,
      y: p.y,
      sx,
      sy,
      pressure: e.pressure || 0.5,
      pen: e.pointerType === 'pen',
      button: e.button,
      shift: e.shiftKey,
      alt: e.altKey,
      ctrl: e.ctrlKey || e.metaKey,
    };
  }

  function coalesced(e: PointerEvent): PointerInfo[] {
    const list = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : [];
    return (list.length ? list : [e]).map((x) => info(x as PointerEvent));
  }

  function onDown(e: PointerEvent) {
    if (!doc) return;
    host!.focus({ preventScroll: true });
    if (e.pointerType === 'touch') {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (touches.size === 2) {
        // second finger: cancel the stroke, start pinching
        if (active) {
          active.tool.cancel?.();
          active = null;
        }
        const [a, b] = [...touches.values()];
        const r = host!.getBoundingClientRect();
        pinch = { dist: Math.hypot(a.x - b.x, a.y - b.y), zoom: paint.view.zoom, cx: (a.x + b.x) / 2 - r.left, cy: (a.y + b.y) / 2 - r.top, view: paint.view };
        return;
      }
    }
    const p = info(e);
    if (e.button === 1 || (e.button === 0 && spaceDown) || (paint.tool === 'hand' && e.button === 0)) {
      e.preventDefault();
      panning = { sx: p.sx, sy: p.sy, x: paint.view.x, y: paint.view.y, id: e.pointerId };
      host!.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button !== 0 && e.button !== 2) return;
    e.preventDefault();
    // Alt+click picks a colour with the painting tools
    let tool = toolFor(paint.tool);
    if (e.altKey && ALT_PICKS.includes(paint.tool)) tool = toolFor('picker');
    active = { tool, id: e.pointerId };
    host!.setPointerCapture(e.pointerId);
    tool.down(p);
    request();
  }

  function onMove(e: PointerEvent) {
    if (e.pointerType === 'touch' && touches.has(e.pointerId)) {
      touches.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (pinch && touches.size === 2) {
        const [a, b] = [...touches.values()];
        const r = host!.getBoundingClientRect();
        const dist = Math.hypot(a.x - b.x, a.y - b.y);
        const cx = (a.x + b.x) / 2 - r.left;
        const cy = (a.y + b.y) / 2 - r.top;
        const z = zoomAt(pinch.view, (pinch.zoom * dist) / Math.max(1, pinch.dist), pinch.cx, pinch.cy);
        paint.view = { ...z, x: z.x + cx - pinch.cx, y: z.y + cy - pinch.cy };
        return;
      }
    }
    const p = info(e);
    hover = { sx: p.sx, sy: p.sy };
    if (panning && panning.id === e.pointerId) {
      paint.view = { ...paint.view, x: panning.x + p.sx - panning.sx, y: panning.y + p.sy - panning.sy };
      return;
    }
    if (active && active.id === e.pointerId) active.tool.move(p, coalesced(e));
    else toolFor(paint.tool).hover?.(p);
    if (SIZED.includes(paint.tool) || paint.tool === 'clone') request();
  }

  function onUp(e: PointerEvent) {
    touches.delete(e.pointerId);
    if (touches.size < 2) pinch = null;
    if (panning && panning.id === e.pointerId) {
      panning = null;
      return;
    }
    if (active && active.id === e.pointerId) {
      const t = active.tool;
      active = null;
      t.up(info(e));
      request();
    }
  }

  function onWheel(e: WheelEvent) {
    if (!doc) return;
    e.preventDefault();
    const r = host!.getBoundingClientRect();
    const sx = e.clientX - r.left;
    const sy = e.clientY - r.top;
    const scale = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    if (e.ctrlKey || e.metaKey) {
      // Ctrl+wheel, and touchpad pinch (reported as Ctrl+wheel)
      const f = Math.exp((-e.deltaY * scale) / 300);
      paint.view = zoomAt(paint.view, paint.view.zoom * f, sx, sy);
    } else {
      const dx = e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX;
      const dy = e.shiftKey && !e.deltaX ? 0 : e.deltaY;
      paint.view = { ...paint.view, x: paint.view.x - dx * scale, y: paint.view.y - dy * scale };
    }
  }

  function onKey(e: KeyboardEvent, down: boolean) {
    if (e.code !== 'Space') return;
    const t = e.target as HTMLElement | null;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
    if (down) e.preventDefault();
    spaceDown = down;
  }

  const cursor = $derived(
    spaceDown || paint.tool === 'hand'
      ? panning
        ? 'grabbing'
        : 'grab'
      : SIZED.includes(paint.tool)
        ? 'none'
        : paint.tool === 'move'
          ? 'move'
          : paint.tool === 'text'
            ? 'text'
            : paint.tool === 'picker' || paint.tool === 'fill'
              ? 'crosshair'
              : 'crosshair',
  );
</script>

<svelte:window onkeydown={(e) => onKey(e, true)} onkeyup={(e) => onKey(e, false)} onblur={() => (spaceDown = false)} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="paint-viewport"
  bind:this={host}
  tabindex="0"
  role="application"
  aria-label="Drawing canvas"
  style:cursor
  onpointerdown={onDown}
  onpointermove={onMove}
  onpointerup={onUp}
  onpointercancel={onUp}
  onpointerleave={() => {
    hover = null;
    request();
  }}
  onwheel={onWheel}
  oncontextmenu={(e) => e.preventDefault()}
>
  <canvas bind:this={canvas}></canvas>
</div>

<style>
  .paint-viewport {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--canvas-bg);
    touch-action: none;
    outline: none;
  }
  canvas {
    display: block;
  }
</style>
