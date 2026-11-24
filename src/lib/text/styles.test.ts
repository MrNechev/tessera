import { describe, expect, it } from 'vitest';
import { lookCss, presetElement, textPreset, TEXT_PRESETS, themedLook, titleCss, titleStyle, TITLE_STYLES } from './styles';
import { newSticker, rotatedBounds, snapAngle } from '../documents/lib/canvas';
import { FONTS } from '../assets/fonts';

describe('text styles', () => {
  it('only uses registered fonts', () => {
    const ids = new Set(FONTS.map((f) => f.id));
    for (const p of TEXT_PRESETS) expect(ids.has(p.look.font), p.id).toBe(true);
    for (const t of TITLE_STYLES) if (t.look) expect(ids.has(t.look.font), t.id).toBe(true);
  });

  it('turns a preset into the collage element it always was', () => {
    // Meme: outline of 8.5 % of the size in page units, white Anton in capitals
    const meme = presetElement(textPreset('meme')!, 200, 2000);
    expect(meme).toMatchObject({ font: 'anton', text: 'TOP TEXT', size: 200, uppercase: true, fill: '#ffffff', stroke: { width: 17, color: '#000000' } });
    const label = presetElement(textPreset('label')!, 100, 2000);
    expect(label.text).toBe('BEST FRIENDS');
    expect(label.background).toEqual({ color: '#ff5d8f', padding: 0.45, radius: 1 });
    expect(presetElement(textPreset('arched')!, 100, 2000).curve).toBe(0.35);
  });

  it('renders outlines as CSS like the collage renderer', () => {
    const css = lookCss(textPreset('meme')!.look);
    expect(css).toContain("font-family:'Anton'");
    expect(css).toContain('text-transform:uppercase');
    // the stroke is centred on the outline and drawn under the fill: twice the width
    expect(css).toContain('-webkit-text-stroke:0.17em #000000');
    expect(css).toContain('paint-order:stroke fill');
    expect(lookCss(undefined)).toBe('');
    expect(lookCss({ font: 'inter', shadow: { x: 0.1, y: 0.2, blur: 0.3, color: '#000' } })).toContain('text-shadow:0.1em 0.2em 0.3em #000');
    const pill = lookCss(textPreset('label')!.look, { pill: true });
    expect(pill).toContain('background:#ff5d8f');
    expect(pill).toContain('border-radius:1em');
  });

  it('keeps dark ink readable in themed views', () => {
    expect(themedLook(textPreset('poster')!).fill).toBeUndefined();
    expect(themedLook(textPreset('script')!).fill).toBe('#c2185b');
    expect(textPreset('poster')!.look.fill).toBe('#1d1b26');
  });

  it('styles section titles', () => {
    expect(titleCss(undefined)).toBe('');
    expect(titleCss('nope')).toBe('');
    expect(titleStyle('nope').id).toBe('classic');
    expect(titleCss('script')).toContain("'Great Vibes'");
    expect(titleCss('script')).toContain('--tscale:1.45');
    expect(titleCss('meme')).toContain('-webkit-text-stroke');
  });
});

describe('canvas stickers', () => {
  it('keeps the catalogue aspect ratio', () => {
    const s = newSticker({ id: 'cake', width: 400, height: 200 }, 100, 100, 160);
    expect(s).toMatchObject({ type: 'sticker', sticker: 'cake', w: 160, h: 80, x: 20, y: 60 });
    const tall = newSticker({ id: 'x', width: 100, height: 200 }, 0, 0, 100);
    expect([tall.w, tall.h]).toEqual([50, 100]);
  });

  it('measures turned stickers', () => {
    const s = newSticker({ id: 'a', width: 100, height: 100 }, 50, 50, 100);
    expect(rotatedBounds(s)).toBe(s);
    const b = rotatedBounds({ ...s, rotation: 45 });
    expect(b.w).toBeCloseTo(141.42, 1);
    expect(b.x + b.w / 2).toBeCloseTo(50);
  });

  it('snaps angles to quarter turns and normalises them', () => {
    expect(snapAngle(88)).toBe(90);
    expect(snapAngle(80)).toBe(80);
    expect(snapAngle(270)).toBe(-90);
    expect(snapAngle(-180)).toBe(180);
    expect(snapAngle(2)).toBe(0);
  });
});
