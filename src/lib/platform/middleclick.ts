/**
 * On Linux a middle click pastes the PRIMARY selection: whatever text was last
 * selected anywhere (file names in the Library grid, a path in a terminal…).
 * Middle-button panning on the canvas must not paste that into the story, so
 * paste handlers ignore a paste that comes right after a middle-button press.
 */
let last = -1e9;

if (typeof window !== 'undefined') {
  const mark = (e: MouseEvent) => {
    if (e.button === 1) last = performance.now();
  };
  for (const t of ['pointerdown', 'pointerup', 'mousedown', 'mouseup', 'auxclick'] as const) window.addEventListener(t, mark, true);
}

/** True when the paste being handled was most likely a middle click. */
export function middleClickPaste(): boolean {
  return performance.now() - last < 800;
}
