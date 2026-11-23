// Command palette registry (Ctrl+K). Views register providers that return their current
// commands (sections, frames, actions); the palette adds documents, folders and search.
export interface Command {
  id: string;
  title: string;
  subtitle?: string;
  group: string;
  icon?: string;
  keywords?: string;
  shortcut?: string;
  run: () => void | Promise<void>;
}

class Palette {
  open = $state(false);
  private providers = new Map<string, () => Command[]>();
  /** set by the library panel while it is on screen */
  openFolder: ((libraryId: string, path: string) => void) | null = null;

  register(name: string, fn: () => Command[]) {
    this.providers.set(name, fn);
    return () => {
      if (this.providers.get(name) === fn) this.providers.delete(name);
    };
  }

  commands(): Command[] {
    const out: Command[] = [];
    for (const fn of this.providers.values()) {
      try {
        out.push(...fn());
      } catch {
        /* a provider whose view is going away */
      }
    }
    return out;
  }
}

export const palette = new Palette();

/** Fuzzy score: substring matches first (earlier = better), then in-order letter matches. */
export function score(text: string, q: string): number {
  if (!q) return 1;
  const t = text.toLowerCase();
  const i = t.indexOf(q);
  if (i >= 0) return 1000 - i * 2 - t.length * 0.01 + (i === 0 || /\W/.test(t[i - 1]) ? 50 : 0);
  let ti = 0,
    s = 0,
    run = 0;
  for (const ch of q) {
    const j = t.indexOf(ch, ti);
    if (j < 0) return -1;
    run = j === ti ? run + 1 : 0;
    s += 10 + run * 5 - (j - ti);
    ti = j + 1;
  }
  return Math.max(1, s);
}
