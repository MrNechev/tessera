/**
 * "IMG_0042.HEIC" → "IMG_0042 cut-out.png": no path separators or characters
 * other systems reject, and never a dot file (the library skips those).
 */
export function cutoutName(source: string): string {
  const base = source.split('/').pop() ?? 'photo';
  const dot = base.lastIndexOf('.');
  const stem = (dot > 0 ? base.slice(0, dot) : base).replace(/[\\/:*?"<>|\u0000-\u001f]/g, '_').replace(/^[.\s]+/, '').trim() || 'photo';
  return `${stem} cut-out.png`;
}
