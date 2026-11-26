import { describe, expect, it } from 'vitest';
import { deflateRawSync } from 'node:zlib';
import { fillMasked, matchColor } from './flood';
import { anchorOffset, arrowPolygon, capArea, fitView, linePixels, snapAngle, squareBox, toImage, zoomAt } from './geom';
import { History, TileRecorder, tileRect, tilesFor } from './history';
import { clipRect, combine, ellipseMask, invertMask, maskBounds, maskEdges, polygonMask, rectMask, translateMask } from './mask';
import { blendFromOra, blendToOra, buildOra, parseOra, parseStackXml, stackXml } from './ora';
import { crc32, inflateRaw, readZip, writeZip } from './zip';

function img(w: number, h: number, fill: [number, number, number, number]): Uint8ClampedArray {
  const d = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) d.set(fill, i * 4);
  return d;
}
function set(d: Uint8ClampedArray, w: number, x: number, y: number, c: [number, number, number, number]) {
  d.set(c, (y * w + x) * 4);
}
const count = (m: Uint8Array) => m.reduce((s, v) => s + (v ? 1 : 0), 0);

describe('flood fill', () => {
  // 6×4 white image with a black vertical wall at x = 2, and an isolated white pixel island right of it
  const w = 6;
  const h = 4;
  const d = img(w, h, [255, 255, 255, 255]);
  for (let y = 0; y < h; y++) set(d, w, 2, y, [0, 0, 0, 255]);

  it('contiguous fill stops at walls', () => {
    const r = matchColor(d, w, h, 0, 0, 0, true);
    expect(r.count).toBe(8);
    expect(r.bounds).toEqual({ x: 0, y: 0, w: 2, h: 4 });
    expect(r.mask[3]).toBe(0);
  });
  it('global match finds every matching pixel', () => {
    const r = matchColor(d, w, h, 0, 0, 0, false);
    expect(r.count).toBe(20);
    expect(r.bounds).toEqual({ x: 0, y: 0, w: 6, h: 4 });
  });
  it('tolerance includes similar colours', () => {
    const e = img(3, 1, [100, 100, 100, 255]);
    set(e, 3, 1, 0, [110, 100, 100, 255]);
    set(e, 3, 2, 0, [200, 100, 100, 255]);
    expect(matchColor(e, 3, 1, 0, 0, 0, true).count).toBe(1);
    expect(matchColor(e, 3, 1, 0, 0, 5, true).count).toBe(2);
    expect(matchColor(e, 3, 1, 0, 0, 50, true).count).toBe(3);
  });
  it('transparent pixels match regardless of colour; outside clicks match nothing', () => {
    const e = img(2, 1, [0, 0, 0, 0]);
    set(e, 2, 1, 0, [255, 0, 0, 0]);
    expect(matchColor(e, 2, 1, 0, 0, 0, true).count).toBe(2);
    expect(matchColor(e, 2, 1, 5, 5, 0, true).count).toBe(0);
  });
  it('fills a spiral-ish region through scanline seeds', () => {
    // U shape: wall everywhere except a corridor that turns back
    const W = 5;
    const H = 5;
    const e = img(W, H, [0, 0, 0, 255]);
    for (const [x, y] of [[0, 0], [0, 1], [0, 2], [0, 3], [0, 4], [1, 4], [2, 4], [3, 4], [4, 4], [4, 3], [4, 2], [4, 1], [4, 0], [3, 0], [2, 0], [2, 1], [2, 2]])
      set(e, W, x, y, [255, 255, 255, 255]);
    const r = matchColor(e, W, H, 2, 2, 0, true);
    expect(r.count).toBe(17);
  });
  it('fillMasked paints only masked pixels, limited by the selection', () => {
    const e = img(2, 2, [0, 0, 0, 0]);
    const mask = new Uint8Array([255, 255, 255, 0]);
    const limit = new Uint8Array([255, 0, 255, 255]);
    fillMasked(e, { x: 0, y: 0, w: 2, h: 2 }, 2, mask, [255, 0, 0, 255], limit);
    // a sub-rectangle: only the right column (pixels 1 and 3 of the image)
    const col = new Uint8ClampedArray(8);
    fillMasked(col, { x: 1, y: 0, w: 1, h: 2 }, 2, new Uint8Array([0, 255, 0, 255]), [0, 0, 255, 255]);
    expect([...col]).toEqual([0, 0, 255, 255, 0, 0, 255, 255]);
    expect([...e.slice(0, 4)]).toEqual([255, 0, 0, 255]);
    expect(e[7]).toBe(0);
    expect([...e.slice(8, 12)]).toEqual([255, 0, 0, 255]);
    expect(e[15]).toBe(0);
  });
});

describe('selection masks', () => {
  it('rect, clip and bounds', () => {
    const m = rectMask(10, 10, { x: 2, y: 3, w: 4, h: 2 });
    expect(count(m)).toBe(8);
    expect(maskBounds(m, 10, 10)).toEqual({ x: 2, y: 3, w: 4, h: 2 });
    expect(clipRect({ x: -5, y: 8, w: 8, h: 10 }, 10, 10)).toEqual({ x: 0, y: 8, w: 3, h: 2 });
    expect(clipRect({ x: 20, y: 0, w: 2, h: 2 }, 10, 10)).toBeNull();
    // dragged backwards
    expect(count(rectMask(10, 10, { x: 6, y: 5, w: -4, h: -2 }))).toBe(8);
    expect(maskBounds(new Uint8Array(100), 10, 10)).toBeNull();
  });
  it('ellipse area is about πab', () => {
    const m = ellipseMask(100, 100, { x: 10, y: 20, w: 80, h: 40 });
    expect(Math.abs(count(m) - Math.PI * 40 * 20) / (Math.PI * 800)).toBeLessThan(0.03);
    expect(maskBounds(m, 100, 100)).toEqual({ x: 10, y: 20, w: 80, h: 40 });
  });
  it('lasso polygon (triangle) and even-odd', () => {
    const m = polygonMask(20, 20, [
      { x: 0, y: 0 },
      { x: 20, y: 0 },
      { x: 0, y: 20 },
    ]);
    expect(Math.abs(count(m) - 200)).toBeLessThan(15);
    expect(m[0]).toBe(255);
    expect(m[19 * 20 + 19]).toBe(0);
  });
  it('combine modes and invert', () => {
    const a = rectMask(4, 1, { x: 0, y: 0, w: 2, h: 1 }); // 1100
    const b = rectMask(4, 1, { x: 1, y: 0, w: 2, h: 1 }); // 0110
    expect([...combine(a, b, 'add')]).toEqual([255, 255, 255, 0]);
    expect([...combine(a, b, 'subtract')]).toEqual([255, 0, 0, 0]);
    expect([...combine(a, b, 'intersect')]).toEqual([0, 255, 0, 0]);
    expect(combine(a, b, 'replace')).toBe(b);
    expect(combine(null, b, 'subtract')).toBe(b);
    expect([...invertMask(a)]).toEqual([0, 0, 255, 255]);
  });
  it('translate drops what leaves the image', () => {
    const m = rectMask(4, 4, { x: 0, y: 0, w: 2, h: 2 });
    const t = translateMask(m, 4, 4, 3, 1);
    expect(maskBounds(t, 4, 4)).toEqual({ x: 3, y: 1, w: 1, h: 2 });
    expect(count(translateMask(m, 4, 4, -1, -1))).toBe(1);
  });
  it('edges of a rectangle are four merged segments', () => {
    const m = rectMask(10, 10, { x: 2, y: 3, w: 4, h: 2 });
    const e = maskEdges(m, 10, 10);
    expect(e.length).toBe(16);
    const segs = [];
    for (let i = 0; i < e.length; i += 4) segs.push(e.slice(i, i + 4).join(','));
    expect(segs.sort()).toEqual(['2,3,2,5', '2,3,6,3', '2,5,6,5', '6,3,6,5'].sort());
    // two separate pixels: 8 segments
    const two = new Uint8Array(9);
    two[0] = 255;
    two[8] = 255;
    expect(maskEdges(two, 3, 3).length / 4).toBe(8);
  });
});

describe('undo history', () => {
  it('tiles of a rectangle', () => {
    expect(tilesFor({ x: 0, y: 0, w: 1, h: 1 }, 300, 300, 128)).toEqual([0]);
    expect(tilesFor({ x: 120, y: 120, w: 20, h: 20 }, 300, 300, 128)).toEqual([0, 1, 3, 4]);
    expect(tilesFor({ x: -50, y: 250, w: 1000, h: 1000 }, 300, 300, 128)).toEqual([3, 4, 5, 6, 7, 8]);
    expect(tilesFor({ x: 0, y: 0, w: 0, h: 5 }, 300, 300)).toEqual([]);
    expect(tileRect(8, 300, 300, 128)).toEqual({ x: 256, y: 256, w: 44, h: 44 });
  });
  it('the recorder keeps only tiles that changed, before and after', () => {
    const w = 8;
    const h = 8;
    const px = new Uint8ClampedArray(w * h * 4);
    const read = (r: { x: number; y: number; w: number; h: number }) => {
      const out = new Uint8ClampedArray(r.w * r.h * 4);
      for (let y = 0; y < r.h; y++) out.set(px.subarray(((r.y + y) * w + r.x) * 4, ((r.y + y) * w + r.x + r.w) * 4), y * r.w * 4);
      return out;
    };
    const rec = new TileRecorder(w, h, read, 4);
    rec.touch({ x: 0, y: 0, w: 8, h: 2 }); // tiles 0 and 1
    rec.touch({ x: 5, y: 5, w: 1, h: 1 }); // tile 3
    expect(rec.touched).toBe(3);
    px[(5 * w + 5) * 4] = 200; // only tile 3 changes
    const patches = rec.finish();
    expect(patches.length).toBe(1);
    expect(patches[0].rect).toEqual({ x: 4, y: 4, w: 4, h: 4 });
    expect(patches[0].before[(1 * 4 + 1) * 4]).toBe(0);
    expect(patches[0].after[(1 * 4 + 1) * 4]).toBe(200);
    expect(rec.touched).toBe(0);
  });
  it('undo/redo order, redo cleared by new steps, budget drops the oldest', () => {
    const log: string[] = [];
    const step = (n: string, bytes = 10) => ({ label: n, bytes, undo: () => log.push('u' + n), redo: () => log.push('r' + n) });
    const h = new History(25);
    h.push(step('a'));
    h.push(step('b'));
    expect(h.bytes).toBe(20);
    h.push(step('c'));
    // over budget: "a" forgotten
    expect(h.size).toBe(2);
    expect(h.bytes).toBe(20);
    h.undo();
    h.undo();
    expect(h.undo()).toBeNull();
    expect(log).toEqual(['uc', 'ub']);
    h.redo();
    expect(log.at(-1)).toBe('rb');
    expect(h.canRedo).toBe(true);
    h.push(step('d'));
    expect(h.canRedo).toBe(false);
    expect(h.redoLabel).toBeNull();
    expect(h.undoLabel).toBe('d');
    // a single huge step is still kept
    h.push(step('huge', 1000));
    expect(h.size).toBe(1);
    expect(h.canUndo).toBe(true);
  });
});

describe('geometry', () => {
  it('bresenham lines', () => {
    expect(linePixels(0, 0, 3, 0).length).toBe(4);
    expect(linePixels(0, 0, 3, 3)).toEqual([
      { x: 0, y: 0 },
      { x: 1, y: 1 },
      { x: 2, y: 2 },
      { x: 3, y: 3 },
    ]);
    expect(linePixels(2, 5, 2, 5)).toEqual([{ x: 2, y: 5 }]);
  });
  it('shift constraints', () => {
    const p = snapAngle({ x: 0, y: 0 }, { x: 10, y: 1 });
    expect(Math.round(p.y)).toBe(0);
    const q = snapAngle({ x: 0, y: 0 }, { x: 10, y: 9 });
    expect(Math.round(q.x)).toBe(Math.round(q.y));
    expect(squareBox({ x: 0, y: 0 }, { x: -3, y: 8 })).toEqual({ x: -8, y: 8 });
  });
  it('arrow polygon ends at the tip', () => {
    const a = arrowPolygon({ x: 0, y: 0 }, { x: 100, y: 0 }, 4);
    expect(a.length).toBe(7);
    expect(a[3]).toEqual({ x: 100, y: 0 });
    expect(arrowPolygon({ x: 1, y: 1 }, { x: 1, y: 1 }, 4)).toEqual([]);
  });
  it('zoom keeps the point under the cursor; fit centres', () => {
    const v = { zoom: 1, x: 10, y: 20 };
    const z = zoomAt(v, 4, 110, 120);
    const before = toImage(v, 110, 120);
    const after = toImage(z, 110, 120);
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
    const f = fitView(2000, 1000, 1048, 548, 24);
    expect(f.zoom).toBe(0.5);
    expect(f.x).toBe(24);
    expect(fitView(100, 100, 1000, 1000).zoom).toBe(1);
  });
  it('anchors and the 16 MP cap', () => {
    expect(anchorOffset(100, 100, 200, 300, 1, 1)).toEqual({ x: 50, y: 100 });
    expect(anchorOffset(100, 100, 200, 300, 2, 0)).toEqual({ x: 100, y: 0 });
    const c = capArea(8000, 6000, 16e6);
    expect(c.scaled).toBe(true);
    expect(c.w * c.h).toBeLessThanOrEqual(16e6);
    expect(Math.abs(c.w / c.h - 4 / 3)).toBeLessThan(0.01);
    expect(capArea(100, 100, 16e6)).toEqual({ w: 100, h: 100, scaled: false });
  });
});

describe('zip and OpenRaster', () => {
  it('crc32 of a known string', () => {
    expect(crc32(new TextEncoder().encode('123456789'))).toBe(0xcbf43926);
  });
  it('inflate matches zlib for stored, fixed and dynamic blocks', () => {
    const text = new TextEncoder().encode('Hello hello hello hello, OpenRaster! '.repeat(200));
    const rnd = new Uint8Array(70000);
    let s = 7;
    for (let i = 0; i < rnd.length; i++) rnd[i] = (s = (s * 1103515245 + 12345) >>> 0) >>> 24;
    for (const data of [text, rnd, new Uint8Array(0), new Uint8Array([1, 2, 3])]) {
      for (const level of [0, 1, 9]) {
        const z = deflateRawSync(data, { level });
        expect(Buffer.from(inflateRaw(new Uint8Array(z))).equals(Buffer.from(data))).toBe(true);
      }
    }
  });
  it('writes a zip that reads back, mimetype first and stored', () => {
    const z = writeZip([
      { name: 'mimetype', data: new TextEncoder().encode('image/openraster') },
      { name: 'dir/ü.bin', data: new Uint8Array([1, 2, 3]) },
    ]);
    // local header of the first entry: name at offset 30, data right after
    expect(new TextDecoder().decode(z.subarray(30, 38))).toBe('mimetype');
    expect(new TextDecoder().decode(z.subarray(38, 54))).toBe('image/openraster');
    const r = readZip(z);
    expect([...r.keys()]).toEqual(['mimetype', 'dir/ü.bin']);
    expect([...r.get('dir/ü.bin')!()]).toEqual([1, 2, 3]);
  });
  it('stack.xml round-trip with escaping, blend modes and Tessera text', () => {
    const layers = [
      { name: 'Top "quoted" <&>', x: 5, y: -3, opacity: 0.5, visible: false, locked: true, blend: 'multiply' as const, text: '{"t":"a\nb"}', selected: true },
      { name: 'Background', x: 0, y: 0, opacity: 1, visible: true, locked: false, blend: 'source-over' as const },
    ];
    const xml = stackXml({ width: 640, height: 480, layers });
    const p = parseStackXml(xml);
    expect(p.width).toBe(640);
    expect(p.height).toBe(480);
    expect(p.layers.map((l) => l.src)).toEqual(['data/layer1.png', 'data/layer0.png']);
    expect(p.layers[0]).toMatchObject({ name: 'Top "quoted" <&>', x: 5, y: -3, opacity: 0.5, visible: false, locked: true, blend: 'multiply', text: '{"t":"a\nb"}', selected: true });
    expect(p.layers[1]).toMatchObject({ name: 'Background', visible: true, locked: false, blend: 'source-over' });
    expect(blendFromOra(blendToOra('soft-light'))).toBe('soft-light');
    expect(blendFromOra('svg:unknown')).toBe('source-over');
  });
  it('reads nested stacks the way Krita writes them', () => {
    const xml = `<?xml version='1.0' encoding='UTF-8'?>
<image w='100' h='50' version='0.0.1'>
 <stack opacity="1" name="root">
  <stack name="group" visibility="hidden" opacity="0.5" x="10" y="0">
   <layer name="in group" src="data/a.png" opacity="0.5" x='1' y='2' composite-op="svg:screen"/>
  </stack>
  <layer name="bg" src="./data/b.png"/>
 </stack>
</image>`;
    const p = parseStackXml(xml);
    expect(p.layers.length).toBe(2);
    expect(p.layers[0]).toMatchObject({ name: 'in group', visible: false, opacity: 0.25, x: 11, y: 2, blend: 'screen' });
    expect(p.layers[1]).toMatchObject({ name: 'bg', visible: true, opacity: 1, x: 0, y: 0 });
  });
  it('a whole .ora round-trips', () => {
    const png = (n: number) => new Uint8Array([0x89, 0x50, 0x4e, 0x47, n]);
    const ora = buildOra(
      {
        width: 32,
        height: 16,
        layers: [
          { name: 'Ink', png: png(2), x: 0, y: 0, opacity: 0.8, visible: true, locked: false, blend: 'source-over' },
          { name: 'Paper', png: png(1), x: 0, y: 0, opacity: 1, visible: true, locked: true, blend: 'source-over' },
        ],
      },
      png(9),
      png(8),
    );
    const names = [...readZip(ora).keys()];
    expect(names[0]).toBe('mimetype');
    expect(names).toContain('mergedimage.png');
    expect(names).toContain('Thumbnails/thumbnail.png');
    const back = parseOra(ora);
    expect(back.width).toBe(32);
    expect(back.layers.map((l) => [l.name, l.png[4], l.opacity, l.locked])).toEqual([
      ['Ink', 2, 0.8, false],
      ['Paper', 1, 1, true],
    ]);
    expect(back.merged?.[4]).toBe(9);
    expect(() => parseOra(writeZip([{ name: 'mimetype', data: new TextEncoder().encode('text/plain') }]))).toThrow();
  });
});
