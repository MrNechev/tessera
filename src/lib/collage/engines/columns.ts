/**
 * "Classic" column layout — a faithful port of PhotoCollage's algorithm
 * (https://github.com/adrienverge/PhotoCollage, GPL-2.0-or-later, © Adrien Vergé).
 *
 * Phase A: photos are added one by one to the shortest column; wide photos may
 *          be "extended" over two neighbouring columns of similar height.
 * Phase B: bottom holes left by extended cells are removed.
 * Phase C: every column is stretched/shrunk to a common height.
 */
import type { Rect } from '../../model/types';
import type { Rng } from '../rng';
import { applyGap, type Engine, type Placement } from './types';

interface Photo {
  index: number;
  /** height / width, like PhotoCollage. */
  ratio: number;
}

type AnyCell = Cell | Extent;

class Column {
  cells: AnyCell[] = [];
  constructor(
    public page: Page,
    public w: number,
  ) {}

  get h(): number {
    if (this.cells.length === 0) return 0;
    const last = this.cells[this.cells.length - 1];
    return last.y + last.h;
  }

  get x(): number {
    let x = 0;
    for (const c of this.page.cols) {
      if (c === this) break;
      x += c.w;
    }
    return x;
  }

  leftNeighbor(): Column | null {
    const i = this.page.cols.indexOf(this);
    return i > 0 ? this.page.cols[i - 1] : null;
  }

  rightNeighbor(): Column | null {
    const i = this.page.cols.indexOf(this);
    return i >= 0 && i < this.page.cols.length - 1 ? this.page.cols[i + 1] : null;
  }

  adjustHeight(targetH: number): void {
    interface Group {
      y: number;
      h: number;
      cells: Cell[];
    }
    const groups: Group[] = [{ y: 0, h: 0, cells: [] }];
    for (const c of this.cells) {
      if (c instanceof Cell) {
        groups[groups.length - 1].cells.push(c);
      } else {
        const g = groups[groups.length - 1];
        g.h = c.y - g.y;
        groups.push({ y: c.y + c.h, h: 0, cells: [] });
      }
    }
    const lastGroup = groups[groups.length - 1];
    lastGroup.h = targetH - lastGroup.y;
    for (const g of groups) {
      if (g.cells.length === 0) continue;
      const total = g.cells.reduce((s, c) => s + c.h, 0);
      if (total <= 0) continue;
      const alpha = g.h / total;
      for (const c of g.cells) c.h *= alpha;
    }
  }
}

class Cell {
  extent: Extent | null = null;
  h: number;
  constructor(
    public parents: Column[],
    public photo: Photo,
  ) {
    this.h = this.w * photo.ratio;
  }

  get x(): number {
    return this.parents[0].x;
  }
  get w(): number {
    return this.parents.reduce((s, c) => s + c.w, 0);
  }
  get y(): number {
    const cells = this.parents[0].cells;
    const i = cells.indexOf(this);
    if (i <= 0) return 0;
    const prev = cells[i - 1];
    return prev.y + prev.h;
  }
  isExtended(): boolean {
    return this.extent !== null;
  }
  topNeighbor(): AnyCell | null {
    const cells = this.parents[0].cells;
    const i = cells.indexOf(this);
    return i > 0 ? cells[i - 1] : null;
  }
  bottomNeighbor(): AnyCell | null {
    const cells = this.parents[0].cells;
    const i = cells.indexOf(this);
    return i >= 0 && i < cells.length - 1 ? cells[i + 1] : null;
  }
}

class Extent {
  constructor(public origin: Cell) {
    origin.extent = this;
  }
  get parents(): Column[] {
    return [this.origin.parents[1]];
  }
  get y(): number {
    return this.origin.y;
  }
  get h(): number {
    return this.origin.h;
  }
  isExtended(): boolean {
    return false;
  }
  topNeighbor(): AnyCell | null {
    const cells = this.parents[0].cells;
    const i = cells.indexOf(this);
    return i > 0 ? cells[i - 1] : null;
  }
  bottomNeighbor(): AnyCell | null {
    const cells = this.parents[0].cells;
    const i = cells.indexOf(this);
    return i >= 0 && i < cells.length - 1 ? cells[i + 1] : null;
  }
}

class Page {
  cols: Column[] = [];
  constructor(
    w: number,
    public targetRatio: number,
    noCols: number,
    private rng: Rng,
  ) {
    const colW = w / noCols;
    for (let i = 0; i < noCols; i++) this.cols.push(new Column(this, colW));
  }

  get w(): number {
    return this.cols.reduce((s, c) => s + c.w, 0);
  }

  nextFreeCol(): Column {
    const minimum = Math.min(...this.cols.map((c) => c.h));
    const candidates = this.cols.filter((c) => Math.abs(c.h - minimum) < 1e-12);
    return this.rng.pick(candidates);
  }

  addCell(photo: Photo): void {
    const col = this.nextFreeCol();
    const left = col.leftNeighbor();
    const right = col.rightNeighbor();
    if (2 * this.rng.next() > photo.ratio) {
      if (left && Math.abs(col.h - left.h) < 0.5 * col.w) {
        this.addMulti(left, col, photo);
        return;
      } else if (right && Math.abs(col.h - right.h) < 0.5 * col.w) {
        this.addMulti(col, right, photo);
        return;
      }
    }
    col.cells.push(new Cell([col], photo));
  }

  private addMulti(c1: Column, c2: Column, photo: Photo): void {
    const cell = new Cell([c1, c2], photo);
    const extent = new Extent(cell);
    c1.cells.push(cell);
    c2.cells.push(extent);
  }

  removeEmptyCols(): void {
    this.cols = this.cols.filter((c) => c.cells.length > 0);
  }

  removeBottomHoles(): void {
    for (const col of this.cols) {
      const cell = col.cells[col.cells.length - 1];
      if (cell === col.cells[0]) continue;
      if (cell instanceof Cell && !cell.isExtended()) {
        const top = cell.topNeighbor();
        const right = col.rightNeighbor();
        const left = col.leftNeighbor();
        // Case A1: extend the cell to the right.
        if (top instanceof Cell && top.isExtended() && top.extent!.bottomNeighbor() === null && right) {
          const extent = new Extent(cell);
          right.cells.push(extent);
          cell.parents = [col, right];
        }
        // Case A2: extend the cell to the left.
        else if (top instanceof Extent && top.origin.bottomNeighbor() === null && left) {
          col.cells.splice(col.cells.indexOf(cell), 1);
          left.cells.push(cell);
          const extent = new Extent(cell);
          col.cells.push(extent);
          cell.parents = [left, col];
        }
      } else if (cell instanceof Cell && cell.isExtended() && cell.extent!.bottomNeighbor() === null) {
        const right = col.rightNeighbor();
        const left = col.leftNeighbor();
        const extentTop = cell.extent!.topNeighbor();
        const top = cell.topNeighbor();
        // Case B1: move the cell to the right.
        if (
          extentTop instanceof Cell &&
          extentTop.isExtended() &&
          extentTop.extent!.bottomNeighbor() === null &&
          right &&
          right.rightNeighbor()
        ) {
          const rr = right.rightNeighbor()!;
          col.cells.splice(col.cells.indexOf(cell), 1);
          right.cells.splice(right.cells.indexOf(cell.extent!), 1);
          right.cells.push(cell);
          rr.cells.push(cell.extent!);
          cell.parents = [right, rr];
        }
        // Case B2: move the cell to the left.
        else if (top instanceof Extent && top.origin.bottomNeighbor() === null && left && right) {
          col.cells.splice(col.cells.indexOf(cell), 1);
          right.cells.splice(right.cells.indexOf(cell.extent!), 1);
          left.cells.push(cell);
          col.cells.push(cell.extent!);
          cell.parents = [left, col];
        }
      }
    }
  }

  adjustColsHeights(): void {
    const targetH = this.w * this.targetRatio;
    for (const c of this.cols) c.adjustHeight(targetH);
  }

  adjust(): void {
    this.removeEmptyCols();
    this.removeBottomHoles();
    this.adjustColsHeights();
  }

  cells(): Cell[] {
    const out: Cell[] = [];
    for (const col of this.cols) for (const c of col.cells) if (c instanceof Cell) out.push(c);
    return out;
  }
}

/** PhotoCollage's heuristic for a good number of columns. */
export function columnCount(aspects: number[], pageRatio: number): number {
  if (aspects.length === 0) return 1;
  const avgRatio = aspects.reduce((s, a) => s + 1 / a, 0) / aspects.length;
  const virtualImages = 2 * aspects.length;
  return Math.max(1, Math.round(Math.sqrt((avgRatio / pageRatio) * virtualImages)));
}

export const columnsEngine: Engine = ({ items, area, gap, rng, settings }) => {
  if (items.length === 0) return { placements: [] };
  const ratio = area.h / area.w;
  const auto = columnCount(
    items.map((i) => i.aspect),
    ratio,
  );
  const noCols = Math.max(1, Math.min(items.length, settings.columns && settings.columns > 0 ? settings.columns : auto));
  const page = new Page(1, ratio, noCols, rng);
  for (let i = 0; i < items.length; i++) page.addCell({ index: i, ratio: 1 / items[i].aspect });
  page.adjust();

  const pw = page.w;
  const ph = pw * ratio;
  const cells = page.cells();
  const rects: Rect[] = cells.map((c) => ({
    x: area.x + (c.x / pw) * area.w,
    y: area.y + (c.y / ph) * area.h,
    w: (c.w / pw) * area.w,
    h: (c.h / ph) * area.h,
  }));
  const gapped = applyGap(rects, area, gap);
  const placements: Placement[] = cells.map((c, i) => ({ item: c.photo.index, rect: gapped[i] }));
  return { placements };
};
