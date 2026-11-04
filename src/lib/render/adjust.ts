/** Photo colour adjustments and one-click filter presets. */
import type { PhotoAdjust } from '../model/types';
import { neutralAdjust } from '../model/factory';
import { ctx2d, makeCanvas, type AnyCanvas } from './patterns';

export interface FilterPreset {
  id: string;
  name: string;
  adjust: Partial<PhotoAdjust>;
}

export const FILTER_PRESETS: FilterPreset[] = [
  { id: 'none', name: 'Original', adjust: {} },
  { id: 'vivid', name: 'Vivid', adjust: { saturation: 0.35, contrast: 0.12 } },
  { id: 'warm', name: 'Warm', adjust: { warmth: 0.45, saturation: 0.08 } },
  { id: 'cool', name: 'Cool', adjust: { warmth: -0.4 } },
  { id: 'bw', name: 'B&W', adjust: { grayscale: 1, contrast: 0.12 } },
  { id: 'noir', name: 'Noir', adjust: { grayscale: 1, contrast: 0.45, brightness: -0.05, vignette: 0.45 } },
  { id: 'sepia', name: 'Sepia', adjust: { sepia: 0.85, contrast: 0.05 } },
  { id: 'vintage', name: 'Vintage', adjust: { sepia: 0.35, fade: 0.35, warmth: 0.25, vignette: 0.35, saturation: -0.15 } },
  { id: 'faded', name: 'Faded', adjust: { fade: 0.55, saturation: -0.2, contrast: -0.1 } },
  { id: 'dreamy', name: 'Dreamy', adjust: { brightness: 0.12, fade: 0.3, saturation: 0.1, warmth: 0.15 } },
  { id: 'retro', name: 'Retro', adjust: { warmth: 0.3, contrast: 0.2, fade: 0.2, saturation: -0.05, vignette: 0.25 } },
  { id: 'rose', name: 'Rosé', adjust: { warmth: 0.2, saturation: 0.05, fade: 0.15, brightness: 0.05 } },
];

export function presetAdjust(id: string): PhotoAdjust {
  const preset = FILTER_PRESETS.find((p) => p.id === id);
  return { ...neutralAdjust(), ...(preset?.adjust ?? {}) };
}

/** True if the adjustment changes pixels (vignette is drawn separately). */
export function hasPixelAdjust(a: PhotoAdjust | null | undefined): a is PhotoAdjust {
  if (!a) return false;
  return (
    a.brightness !== 0 ||
    a.contrast !== 0 ||
    a.saturation !== 0 ||
    a.warmth !== 0 ||
    a.fade !== 0 ||
    a.grayscale !== 0 ||
    a.sepia !== 0
  );
}

export function adjustKey(a: PhotoAdjust): string {
  return [a.brightness, a.contrast, a.saturation, a.warmth, a.fade, a.grayscale, a.sepia].map((v) => v.toFixed(3)).join(',');
}

/** Apply `a` to RGBA pixels in place. */
export function applyAdjustPixels(d: Uint8ClampedArray, a: PhotoAdjust): void {
  const bright = 1 + a.brightness * 0.6;
  const contrast = 1 + a.contrast;
  const sat = 1 + a.saturation;
  const warm = a.warmth * 0.09;
  const fadeLift = a.fade * 0.14;
  const fadeScale = 1 - a.fade * 0.22;
  const g = a.grayscale;
  const sp = a.sepia;
  for (let i = 0; i < d.length; i += 4) {
    let r = d[i] / 255;
    let gg = d[i + 1] / 255;
    let b = d[i + 2] / 255;
    r *= bright;
    gg *= bright;
    b *= bright;
    r = (r - 0.5) * contrast + 0.5;
    gg = (gg - 0.5) * contrast + 0.5;
    b = (b - 0.5) * contrast + 0.5;
    let lum = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
    r = lum + (r - lum) * sat;
    gg = lum + (gg - lum) * sat;
    b = lum + (b - lum) * sat;
    r += warm;
    gg += warm * 0.25;
    b -= warm;
    if (g > 0) {
      lum = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
      r += (lum - r) * g;
      gg += (lum - gg) * g;
      b += (lum - b) * g;
    }
    if (sp > 0) {
      const sr = 0.393 * r + 0.769 * gg + 0.189 * b;
      const sg = 0.349 * r + 0.686 * gg + 0.168 * b;
      const sb = 0.272 * r + 0.534 * gg + 0.131 * b;
      r += (sr - r) * sp;
      gg += (sg - gg) * sp;
      b += (sb - b) * sp;
    }
    r = r * fadeScale + fadeLift;
    gg = gg * fadeScale + fadeLift;
    b = b * fadeScale + fadeLift;
    d[i] = r * 255;
    d[i + 1] = gg * 255;
    d[i + 2] = b * 255;
  }
}

/** LRU cache of adjusted bitmaps, keyed by source + adjustment. */
export class AdjustCache {
  private map = new Map<string, AnyCanvas>();
  constructor(private limit = 12) {}

  get(sourceKey: string, source: CanvasImageSource, w: number, h: number, a: PhotoAdjust): AnyCanvas {
    const key = `${sourceKey}|${w}x${h}|${adjustKey(a)}`;
    const hit = this.map.get(key);
    if (hit) {
      this.map.delete(key);
      this.map.set(key, hit);
      return hit;
    }
    const canvas = makeCanvas(w, h);
    const c = ctx2d(canvas);
    c.drawImage(source, 0, 0, w, h);
    const img = c.getImageData(0, 0, w, h);
    applyAdjustPixels(img.data, a);
    c.putImageData(img, 0, 0);
    this.map.set(key, canvas);
    while (this.map.size > this.limit) {
      const first = this.map.keys().next().value as string;
      this.map.delete(first);
    }
    return canvas;
  }

  clear(): void {
    this.map.clear();
  }
}
