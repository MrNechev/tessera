import type { Fill } from '../model/types';

type Ctx = CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D;

/** Resolve a Fill into a canvas paint for the box (x, y, w, h). */
export function paintFor(ctx: Ctx, fill: Fill, x: number, y: number, w: number, h: number): string | CanvasGradient {
  switch (fill.kind) {
    case 'solid':
      return fill.color;
    case 'linear': {
      const a = (fill.angle * Math.PI) / 180;
      const cx = x + w / 2;
      const cy = y + h / 2;
      const half = (Math.abs(w * Math.cos(a)) + Math.abs(h * Math.sin(a))) / 2;
      const g = ctx.createLinearGradient(cx - Math.cos(a) * half, cy - Math.sin(a) * half, cx + Math.cos(a) * half, cy + Math.sin(a) * half);
      for (const s of fill.stops) g.addColorStop(clamp01(s.offset), s.color);
      return g;
    }
    case 'radial': {
      const cx = x + (fill.cx ?? 0.5) * w;
      const cy = y + (fill.cy ?? 0.5) * h;
      const r = Math.max(
        Math.hypot(cx - x, cy - y),
        Math.hypot(cx - x - w, cy - y),
        Math.hypot(cx - x, cy - y - h),
        Math.hypot(cx - x - w, cy - y - h),
      );
      const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
      for (const s of fill.stops) g.addColorStop(clamp01(s.offset), s.color);
      return g;
    }
  }
}

function clamp01(v: number): number {
  return Math.max(0, Math.min(1, v));
}

/** CSS `background` equivalent of a fill, for UI swatches. */
export function cssForFill(fill: Fill): string {
  switch (fill.kind) {
    case 'solid':
      return fill.color;
    case 'linear':
      return `linear-gradient(${fill.angle + 90}deg, ${fill.stops.map((s) => `${s.color} ${s.offset * 100}%`).join(', ')})`;
    case 'radial':
      return `radial-gradient(circle at ${(fill.cx ?? 0.5) * 100}% ${(fill.cy ?? 0.5) * 100}%, ${fill.stops
        .map((s) => `${s.color} ${s.offset * 100}%`)
        .join(', ')})`;
  }
}

/** A representative solid colour for a fill (e.g. to pick contrasting UI colours). */
export function dominantColor(fill: Fill): string {
  if (fill.kind === 'solid') return fill.color;
  return fill.stops[Math.floor(fill.stops.length / 2)]?.color ?? '#ffffff';
}

export function parseHex(color: string): [number, number, number] | null {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})([0-9a-f]{2})?$/i.exec(color.trim());
  if (!m) return null;
  let hex = m[1];
  if (hex.length === 3) hex = hex.split('').map((c) => c + c).join('');
  return [parseInt(hex.slice(0, 2), 16), parseInt(hex.slice(2, 4), 16), parseInt(hex.slice(4, 6), 16)];
}

export function luminance(color: string): number {
  const rgb = parseHex(color);
  if (!rgb) return 0.5;
  const [r, g, b] = rgb.map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function withAlpha(color: string, alpha: number): string {
  const rgb = parseHex(color);
  if (!rgb) return color;
  return `rgba(${rgb[0]},${rgb[1]},${rgb[2]},${alpha})`;
}
