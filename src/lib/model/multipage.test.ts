import { describe, expect, it } from 'vitest';
import { createDoc, createText } from './factory';
import { activePage, allPages, blankPage, duplicatePage, insertPages, movePage, pageCount, pageDoc, removePage, switchPage } from './multipage';

function doc3() {
  let d = createDoc({ width: 1000, height: 800 });
  d = { ...d, elements: [createText({ x: 0, y: 0, w: 100, text: 'one' })] };
  const two = blankPage();
  two.elements = [createText({ x: 0, y: 0, w: 100, text: 'two' })];
  const three = blankPage();
  three.elements = [createText({ x: 0, y: 0, w: 100, text: 'three' })];
  d = insertPages(d, [two, three], 0);
  return d;
}
const texts = (d: ReturnType<typeof doc3>) => allPages(d).map((p) => (p.elements[0] && p.elements[0].kind === 'text' ? p.elements[0].text : '-'));
const current = (d: ReturnType<typeof doc3>) => (d.elements[0] && d.elements[0].kind === 'text' ? d.elements[0].text : '-');

describe('multi-page documents', () => {
  it('keeps single-page documents unchanged', () => {
    const d = createDoc({ width: 10, height: 10 });
    expect(pageCount(d)).toBe(1);
    expect(d.pages).toBeUndefined();
    expect(switchPage(d, 3)).toBe(d);
  });

  it('inserts pages after the active one and activates the first new page', () => {
    const d = doc3();
    expect(pageCount(d)).toBe(3);
    expect(texts(d)).toEqual(['one', 'two', 'three']);
    expect(activePage(d)).toBe(1);
    expect(current(d)).toBe('two');
    expect(d.pages![1]).toBeNull();
  });

  it('switches pages without losing edits', () => {
    let d = doc3();
    d = { ...d, elements: [createText({ x: 0, y: 0, w: 100, text: 'two edited' })] };
    d = switchPage(d, 2);
    expect(current(d)).toBe('three');
    expect(texts(d)).toEqual(['one', 'two edited', 'three']);
    d = switchPage(d, 0);
    expect(current(d)).toBe('one');
  });

  it('duplicates, moves and removes pages', () => {
    let d = switchPage(doc3(), 0);
    d = duplicatePage(d);
    expect(texts(d)).toEqual(['one', 'one', 'two', 'three']);
    expect(allPages(d)[0].elements[0].id).not.toBe(allPages(d)[1].elements[0].id);
    d = movePage(d, 3, 0);
    expect(texts(d)).toEqual(['three', 'one', 'one', 'two']);
    expect(current(d)).toBe('one');
    d = removePage(d, 0);
    expect(texts(d)).toEqual(['one', 'one', 'two']);
    d = removePage(removePage(d), 0);
    expect(pageCount(d)).toBe(1);
    expect(d.pages).toBeUndefined();
  });

  it('renders any page as a standalone document', () => {
    const d = doc3();
    const p = pageDoc(d, 2);
    expect(p.pages).toBeUndefined();
    expect(p.elements[0].kind === 'text' && p.elements[0].text).toBe('three');
    expect(p.page).toEqual(d.page);
  });
});

import { chunkSizes } from '../editor/projects';

describe('photo book page rhythm', () => {
  it('distributes every photo, never leaving a single photo alone at the end', () => {
    for (const n of [1, 2, 5, 12, 37, 100]) {
      const sizes = chunkSizes(n, 0, 7);
      expect(sizes.reduce((a, b) => a + b, 0)).toBe(n);
      expect(sizes.every((s) => s >= 1 && s <= 6)).toBe(true);
      if (n > 2) expect(sizes[sizes.length - 1]).toBeGreaterThan(sizes.length > 1 && n > 6 ? 1 : 0);
    }
  });
  it('honours a fixed number per page', () => {
    expect(chunkSizes(10, 4)).toEqual([4, 4, 2]);
  });
});
