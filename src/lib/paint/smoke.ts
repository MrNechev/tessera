/**
 * A self-check of Paint for the desktop autotest (WebKitGTK): draws with the
 * real tools, reads pixels back, round-trips the .ora, saves and exports.
 * Loaded only by the autotest driver.
 */
import { ensureFont } from '../assets/fonts';
import { decodeOra, encodeOra } from './io';
import { exportDrawing, newPaint, saveNow } from './session.svelte';
import { paint } from './state.svelte';
import { newTextElement, toolFor, type PointerInfo } from './tools.svelte';

const P = (x: number, y: number, extra: Partial<PointerInfo> = {}): PointerInfo => ({
  x,
  y,
  sx: 0,
  sy: 0,
  pressure: 0.5,
  pen: false,
  button: 0,
  shift: false,
  alt: false,
  ctrl: false,
  ...extra,
});

export async function paintSmoke(log: (m: string) => unknown): Promise<void> {
  const doc = await newPaint({ width: 800, height: 600, background: 'white' });
  const px = (x: number, y: number) => [...doc.composite.getContext('2d')!.getImageData(x, y, 1, 1).data].join(',');
  const check = async (what: string, ok: boolean, detail = '') => log(`paint ${ok ? 'OK' : 'FAIL'} ${what} ${detail}`);

  paint.tool = 'brush';
  paint.primary = '#ff0000';
  const brush = toolFor('brush');
  brush.down(P(100, 300));
  for (let x = 110; x <= 700; x += 10) brush.move(P(x, 300), [P(x, 300)]);
  brush.up(P(700, 300));
  await check('brush', px(400, 300).startsWith('255,0,0'), px(400, 300));

  doc.undo();
  await check('undo', px(400, 300) === '255,255,255,255', px(400, 300));
  doc.redo();
  await check('redo', px(400, 300).startsWith('255,0,0'), px(400, 300));

  paint.primary = '#0000ff';
  toolFor('fill').down(P(400, 500));
  await check('fill', px(400, 500) === '0,0,255,255', px(400, 500));

  paint.tool = 'pencil';
  paint.opts.pencilSize = 3;
  paint.primary = '#00ff00';
  const pencil = toolFor('pencil');
  pencil.down(P(50, 50));
  pencil.move(P(150, 50), [P(150, 50)]);
  pencil.up(P(150, 50));
  await check('pencil', px(100, 50) === '0,255,0,255', px(100, 50));

  await ensureFont('anton', 400);
  paint.opts.textPreset = 'meme';
  const t = doc.addTextLayer(newTextElement(doc, { x: 100, y: 120 }, 'HELLO'), 'Text');
  await new Promise((r) => setTimeout(r, 300));
  const textPixels = doc.canvas(t.id).getContext('2d')!.getImageData(0, 0, 800, 600).data.some((v, i) => i % 4 === 3 && v > 0);
  await check('text', textPixels);

  const bytes = await encodeOra(doc);
  const back = await decodeOra(bytes);
  await check('ora round-trip', back.layers.length === doc.layers.length && back.layers.some((l) => l.text?.text === 'HELLO'), `${bytes.length} bytes, ${back.layers.length} layers`);
  const a = px(400, 300);
  const b = [...back.composite.getContext('2d')!.getImageData(400, 300, 1, 1).data].join(',');
  await check('ora pixels', a === b, `${a} vs ${b}`);

  await saveNow();
  await check('save', paint.saving === 'saved', paint.saving);
  const res = await exportDrawing('png', 90);
  await check('export', !!res, res?.path ?? '');
}
