// Canvas → PNG: the region is rendered offscreen with still images (no video decoding),
// then rasterised with html-to-image (same-origin thumbnails are inlined).
import { mount, tick, unmount } from 'svelte';
import CanvasThumb from '../components/CanvasThumb.svelte';
import { safeName } from './exporters';
import type { Doc, Rect } from './types';

/** `transparent`: no page colour behind the items (PNG with alpha) */
export async function exportCanvasPNG(doc: Doc, rect: Rect, name: string, opts: { transparent?: boolean } = {}) {
  const { toBlob } = await import('html-to-image');
  // output size: up to 4000 px on the long side, at least the region's own size at 1:1
  const pad = 0.03;
  const r = { x: rect.x - rect.w * pad, y: rect.y - rect.h * pad, w: rect.w * (1 + 2 * pad), h: rect.h * (1 + 2 * pad) };
  const k = Math.min(4000 / Math.max(r.w, r.h), 2);
  const W = Math.max(200, Math.round(r.w * k)),
    H = Math.max(200, Math.round(r.h * k));
  const host = document.createElement('div');
  host.style.cssText = `position:fixed;left:-${W + 100}px;top:0;width:${W}px;height:${H}px;overflow:hidden;background:${opts.transparent ? 'transparent' : 'var(--bg)'}`;
  document.body.appendChild(host);
  const comp = mount(CanvasThumb, { target: host, props: { doc, rect: r, transparent: opts.transparent } });
  try {
    await tick();
    // wait for the images to load
    for (let i = 0; i < 4; i++) {
      await new Promise((res) => setTimeout(res, 300));
      await Promise.all([...host.querySelectorAll('img')].map((im) => (im.complete ? null : im.decode().catch(() => null))));
    }
    const bg = opts.transparent ? undefined : getComputedStyle(document.body).backgroundColor;
    const blob = await toBlob(host, { width: W, height: H, pixelRatio: 1, backgroundColor: bg, cacheBust: false });
    if (!blob) throw new Error('rendering failed');
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = safeName(name) + '.png';
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
  } finally {
    unmount(comp);
    host.remove();
  }
}
