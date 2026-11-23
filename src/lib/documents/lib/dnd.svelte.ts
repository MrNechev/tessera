// Drag & drop of library items into the document.
export const MEDIA_MIME = 'application/x-letopis-media';

class DnD {
  dragging = $state(false);
}
export const dnd = new DnD();

export function readDrop(e: DragEvent): { libraryId: string; path: string }[] | null {
  const raw = e.dataTransfer?.getData(MEDIA_MIME);
  if (!raw) return null;
  try {
    const v = JSON.parse(raw);
    return Array.isArray(v) ? v : null;
  } catch {
    return null;
  }
}

export function hasMedia(e: DragEvent) {
  return !!e.dataTransfer && [...e.dataTransfer.types].includes(MEDIA_MIME);
}
