import { FONTS, type FontInfo } from './fontList';

export { FONTS, FONT_CATEGORIES, type FontInfo, type FontCategory } from './fontList';

const byId = new Map(FONTS.map((f) => [f.id, f]));

export function fontInfo(id: string): FontInfo {
  return byId.get(id) ?? byId.get('inter')!;
}

/** Closest available weight for a font. */
export function resolveWeight(id: string, weight: number): number {
  const f = fontInfo(id);
  return f.weights.reduce((best, w) => (Math.abs(w - weight) < Math.abs(best - weight) ? w : best), f.weights[0]);
}

export function cssFont(id: string, weight: number, size: number, italic = false): string {
  const f = fontInfo(id);
  // Generic fallback keeps metrics sane while the real face loads.
  return `${italic ? 'italic ' : ''}${resolveWeight(id, weight)} ${size}px "${f.family}", sans-serif`;
}

type Listener = () => void;
const loaded = new Set<string>();
const pending = new Map<string, Promise<void>>();
const listeners = new Set<Listener>();

let generation = 0;
/** Bumped every time a font face finishes loading. */
export function fontGeneration(): number {
  return generation;
}

export function onFontsLoaded(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Make sure a face is loaded; resolves immediately when it already is. */
export function ensureFont(id: string, weight = 400): Promise<void> {
  if (typeof document === 'undefined' || !('fonts' in document)) return Promise.resolve();
  const w = resolveWeight(id, weight);
  const key = `${id}:${w}`;
  if (loaded.has(key)) return Promise.resolve();
  let p = pending.get(key);
  if (!p) {
    const f = fontInfo(id);
    p = document.fonts
      .load(`${w} 48px "${f.family}"`)
      .then(() => {
        loaded.add(key);
        pending.delete(key);
        generation++;
        listeners.forEach((l) => l());
      })
      .catch(() => {
        pending.delete(key);
      });
    pending.set(key, p);
  }
  return p;
}

export function isFontLoaded(id: string, weight = 400): boolean {
  return loaded.has(`${id}:${resolveWeight(id, weight)}`);
}
