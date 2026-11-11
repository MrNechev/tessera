/** Month-calendar element: date arithmetic and drawing. */
import { cssFont, ensureFont } from '../assets/fonts';
import type { CalendarElement } from '../model/types';
import type { AnyCtx } from './patterns';

export const MONTH_NAMES = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];
const DAY_NAMES = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

export function daysInMonth(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** Weeks of a month as rows of day numbers (0 = empty cell). */
export function monthGrid(year: number, month: number, firstDay: 0 | 1): number[][] {
  const days = daysInMonth(year, month);
  const start = (new Date(year, month - 1, 1).getDay() - firstDay + 7) % 7;
  const rows: number[][] = [];
  let row: number[] = Array(start).fill(0);
  for (let d = 1; d <= days; d++) {
    row.push(d);
    if (row.length === 7) {
      rows.push(row);
      row = [];
    }
  }
  if (row.length) rows.push([...row, ...Array(7 - row.length).fill(0)]);
  return rows;
}

export function dayNames(firstDay: 0 | 1): string[] {
  return firstDay === 1 ? [...DAY_NAMES.slice(1), DAY_NAMES[0]] : DAY_NAMES;
}

export function calendarTitle(el: CalendarElement): string {
  const name = MONTH_NAMES[(el.month - 1 + 12) % 12];
  return el.showYear ? `${name} ${el.year}` : name;
}

/** Draw the calendar in its local box (0,0)–(w,h). Fonts load lazily; the canvas redraws when they arrive. */
export function drawCalendar(c: AnyCtx, el: CalendarElement): void {
  void ensureFont(el.titleFont, 700);
  void ensureFont(el.font, 400);
  void ensureFont(el.font, 600);
  const { w, h } = el;
  // The element's own opacity is already applied; tints multiply onto it.
  const alpha = c.globalAlpha;
  const grid = monthGrid(el.year, el.month, el.firstDay);
  const names = dayNames(el.firstDay);
  const rows = grid.length;

  if (el.background) {
    c.fillStyle = el.background;
    c.beginPath();
    c.roundRect(0, 0, w, h, Math.min(w, h) * 0.04);
    c.fill();
  }

  const padX = w * 0.04;
  const padY = h * 0.05;
  const titleH = el.showTitle ? h * 0.2 : 0;
  const headH = h * 0.09;
  const gridTop = padY + titleH + headH;
  const cellW = (w - 2 * padX) / 7;
  const cellH = (h - gridTop - padY) / rows;
  const weekendIdx = el.firstDay === 1 ? [5, 6] : [0, 6];

  c.textAlign = 'center';
  c.textBaseline = 'middle';

  if (el.showTitle) {
    let size = titleH * 0.55;
    c.font = cssFont(el.titleFont, 700, size);
    const title = calendarTitle(el);
    const tw = c.measureText(title).width;
    if (tw > w - 2 * padX) {
      size *= (w - 2 * padX) / tw;
      c.font = cssFont(el.titleFont, 700, size);
    }
    c.fillStyle = el.color;
    c.fillText(title, w / 2, padY + titleH / 2);
  }

  // Weekday header.
  c.font = cssFont(el.font, 600, Math.min(headH * 0.5, cellW * 0.32));
  names.forEach((n, i) => {
    c.fillStyle = weekendIdx.includes(i) ? el.weekendColor : el.color;
    c.globalAlpha = alpha * 0.85;
    c.fillText(n.toUpperCase(), padX + (i + 0.5) * cellW, padY + titleH + headH / 2);
    c.globalAlpha = alpha;
  });

  const lineW = Math.max(0.5, Math.min(w, h) * 0.003);
  if (el.style === 'lines') {
    c.strokeStyle = el.color;
    c.globalAlpha = alpha * 0.25;
    c.lineWidth = lineW;
    c.beginPath();
    for (let r = 0; r <= rows; r++) {
      c.moveTo(padX, gridTop + r * cellH);
      c.lineTo(w - padX, gridTop + r * cellH);
    }
    c.stroke();
    c.globalAlpha = alpha;
  } else if (el.style === 'boxes') {
    c.strokeStyle = el.color;
    c.globalAlpha = alpha * 0.22;
    c.lineWidth = lineW;
    for (let r = 0; r < rows; r++)
      for (let i = 0; i < 7; i++) c.strokeRect(padX + i * cellW, gridTop + r * cellH, cellW, cellH);
    c.globalAlpha = alpha;
  }

  const numSize = Math.min(cellH * 0.48, cellW * 0.42);
  c.font = cssFont(el.font, 400, numSize);
  grid.forEach((week, r) => {
    week.forEach((d, i) => {
      if (!d) return;
      const cx = padX + (i + 0.5) * cellW;
      const cy = gridTop + (r + 0.5) * cellH;
      const marked = el.marked.includes(d);
      if (marked) {
        c.fillStyle = el.accent;
        c.beginPath();
        c.arc(cx, cy, Math.min(cellW, cellH) * 0.36, 0, Math.PI * 2);
        c.fill();
        c.fillStyle = '#ffffff';
      } else if (el.style === 'dots') {
        c.fillStyle = el.color;
        c.globalAlpha = alpha * 0.07;
        c.beginPath();
        c.arc(cx, cy, Math.min(cellW, cellH) * 0.36, 0, Math.PI * 2);
        c.fill();
        c.globalAlpha = alpha;
        c.fillStyle = weekendIdx.includes(i) ? el.weekendColor : el.color;
      } else {
        c.fillStyle = weekendIdx.includes(i) ? el.weekendColor : el.color;
      }
      c.fillText(String(d), cx, cy);
    });
  });
}
