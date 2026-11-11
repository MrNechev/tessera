import { describe, expect, it } from 'vitest';
import { placements, resizedSize, watermarkSize } from './watermark';

describe('watermark geometry (mirrors batch_tools.rs)', () => {
  it('places corners with margins like the backend', () => {
    expect(placements(1000, 500, 100, 50, 'br', 0.1)).toEqual([[850, 400]]);
    expect(placements(1000, 500, 100, 50, 'c', 0)).toEqual([[450, 225]]);
  });
  it('tiles cover the whole image', () => {
    const p = placements(1000, 1000, 100, 40, 'tile', 0);
    expect(p.length).toBeGreaterThan(20);
    expect(p.some(([x, y]) => x <= 0 && y <= 0)).toBe(true);
  });
  it('limits watermark height to 90% of the image', () => {
    expect(watermarkSize(200, 100, 10, 10, 0.5)).toEqual([90, 90]);
    expect(watermarkSize(1000, 1000, 400, 100, 0.25)).toEqual([250, 63]);
  });
  it('never enlarges when resizing', () => {
    expect(resizedSize(4000, 3000, { kind: 'longEdge', pixels: 2000 })).toEqual([2000, 1500]);
    expect(resizedSize(800, 600, { kind: 'longEdge', pixels: 2000 })).toEqual([800, 600]);
    expect(resizedSize(4000, 3000, { kind: 'fit', width: 1000, height: 0 })).toEqual([1000, 750]);
    expect(resizedSize(4000, 3000, { kind: 'percent', percent: 25 })).toEqual([1000, 750]);
  });
});
