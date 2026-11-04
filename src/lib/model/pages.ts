/** Page size presets. Print sizes are defined in millimetres and converted at a DPI. */

export interface PagePreset {
  id: string;
  name: string;
  group: 'Print' | 'Photo print' | 'Social' | 'Screen';
  /** Physical size in millimetres (print presets) … */
  mm?: [number, number];
  /** … or fixed pixel size (screen/social presets). */
  px?: [number, number];
  hint?: string;
}

export const PAGE_PRESETS: PagePreset[] = [
  { id: 'a4', name: 'A4', group: 'Print', mm: [210, 297] },
  { id: 'a3', name: 'A3', group: 'Print', mm: [297, 420] },
  { id: 'a5', name: 'A5', group: 'Print', mm: [148, 210] },
  { id: 'letter', name: 'US Letter', group: 'Print', mm: [215.9, 279.4] },
  { id: 'legal', name: 'US Legal', group: 'Print', mm: [215.9, 355.6] },
  { id: 'tabloid', name: 'Tabloid', group: 'Print', mm: [279.4, 431.8] },
  { id: 'poster-18x24', name: 'Poster 18×24″', group: 'Print', mm: [457.2, 609.6] },
  { id: 'scrapbook-12', name: 'Scrapbook 12×12″', group: 'Print', mm: [304.8, 304.8] },
  { id: 'photo-4x6', name: '4×6″ (10×15 cm)', group: 'Photo print', mm: [101.6, 152.4] },
  { id: 'photo-5x7', name: '5×7″ (13×18 cm)', group: 'Photo print', mm: [127, 177.8] },
  { id: 'photo-8x10', name: '8×10″ (20×25 cm)', group: 'Photo print', mm: [203.2, 254] },
  { id: 'photo-11x14', name: '11×14″', group: 'Photo print', mm: [279.4, 355.6] },
  { id: 'square', name: 'Square', group: 'Social', px: [3000, 3000], hint: 'Great for sharing' },
  { id: 'ig-post', name: 'Instagram post', group: 'Social', px: [1080, 1080] },
  { id: 'ig-portrait', name: 'Instagram portrait', group: 'Social', px: [1080, 1350] },
  { id: 'story', name: 'Story / Reel', group: 'Social', px: [1080, 1920] },
  { id: 'fb-cover', name: 'Facebook cover', group: 'Social', px: [1640, 924] },
  { id: 'hd', name: 'Full HD', group: 'Screen', px: [1920, 1080] },
  { id: '4k', name: '4K UHD', group: 'Screen', px: [3840, 2160] },
  { id: 'phone', name: 'Phone wallpaper', group: 'Screen', px: [1290, 2796] },
];

export const DPI_CHOICES = [72, 150, 200, 300, 600];

export function presetById(id: string | undefined): PagePreset | undefined {
  return PAGE_PRESETS.find((p) => p.id === id);
}

export function mmToPx(mm: number, dpi: number): number {
  return Math.round((mm / 25.4) * dpi);
}

/** Pixel size of a preset in the given orientation. */
export function presetSize(
  preset: PagePreset,
  dpi: number,
  orientation: 'portrait' | 'landscape' | 'auto' = 'auto',
): [number, number] {
  let w: number;
  let h: number;
  if (preset.px) {
    [w, h] = preset.px;
  } else {
    w = mmToPx(preset.mm![0], dpi);
    h = mmToPx(preset.mm![1], dpi);
  }
  if (orientation === 'landscape' && h > w) [w, h] = [h, w];
  if (orientation === 'portrait' && w > h) [w, h] = [h, w];
  return [w, h];
}

export function describePage(width: number, height: number, dpi: number): string {
  const inch = (v: number) => (v / dpi).toFixed(v / dpi >= 10 ? 1 : 2).replace(/\.?0+$/, '');
  const cm = (v: number) => ((v / dpi) * 2.54).toFixed(1).replace(/\.0$/, '');
  return `${width} × ${height} px · ${inch(width)}×${inch(height)} in · ${cm(width)}×${cm(height)} cm @ ${dpi} DPI`;
}

export const MAX_PAGE_SIDE = 20000;
export const MAX_PAGE_PIXELS = 200_000_000;
