// How many muted/looping videos may play at once while scrolling a document.
// The desktop window's engine (WebKitGTK) keeps about 4 1080p streams at full
// speed (see the README); browsers have no practical limit. When a new video
// starts beyond the limit, the one that started first pauses; it resumes when
// it scrolls back into view.

const MAX = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window ? 4 : Infinity;
const playing: HTMLVideoElement[] = [];

export function autoplay(v: HTMLVideoElement): Promise<void> {
  if (!playing.includes(v)) {
    playing.push(v);
    while (playing.length > MAX) playing.shift()?.pause();
  }
  return v.play();
}

export function stopped(v: HTMLVideoElement): void {
  const i = playing.indexOf(v);
  if (i >= 0) playing.splice(i, 1);
}
