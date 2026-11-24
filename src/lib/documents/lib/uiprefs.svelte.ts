// Per-browser view preferences of the library and presentations (kept in localStorage;
// the keys are Letopis's, so existing choices survive). Settings → Documents & library
// edits the same values the panels use.
export type Transition = 'fade' | 'slide' | 'zoom' | 'none';

function read(k: string, d: string): string {
  try {
    return localStorage.getItem(k) ?? d;
  } catch {
    return d;
  }
}
function write(k: string, v: string) {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* storage unavailable (private window) */
  }
}

class UiPrefs {
  #transition = $state<Transition>((['fade', 'slide', 'zoom', 'none'].includes(read('letopis.transition', 'fade')) ? read('letopis.transition', 'fade') : 'fade') as Transition);
  #blur = $state(read('letopis.lib.blur', '0') === '1');
  #tile = $state(Number(read('letopis.lib.tile', '112')) || 112);

  get transition() {
    return this.#transition;
  }
  set transition(v: Transition) {
    this.#transition = v;
    write('letopis.transition', v);
  }
  /** Blur library thumbnails (for screen sharing); hover to peek. */
  get blur() {
    return this.#blur;
  }
  set blur(v: boolean) {
    this.#blur = v;
    write('letopis.lib.blur', v ? '1' : '0');
  }
  /** Smallest library tile, in CSS pixels. */
  get tile() {
    return this.#tile;
  }
  set tile(v: number) {
    this.#tile = v;
    write('letopis.lib.tile', String(v));
  }
}

export const uiPrefs = new UiPrefs();
