/** Decorative borders drawn around the whole page. */
import type { PageFrame, PageFrameStyle } from '../model/types';
import type { AnyCtx } from './patterns';

export const PAGE_FRAMES: { id: PageFrameStyle; name: string; color: string; width: number; inset: number }[] = [
  { id: 'none', name: 'None', color: '#000000', width: 0, inset: 0 },
  { id: 'line', name: 'Line', color: '#1d1b26', width: 0.006, inset: 0.025 },
  { id: 'double', name: 'Double', color: '#1d1b26', width: 0.005, inset: 0.022 },
  { id: 'stitched', name: 'Stitched', color: '#ffffff', width: 0.004, inset: 0.03 },
  { id: 'dots', name: 'Dots', color: '#ffffff', width: 0.005, inset: 0.025 },
  { id: 'scallop', name: 'Scallop', color: '#ffffff', width: 0.012, inset: 0.02 },
  { id: 'candy', name: 'Candy cane', color: '#d7263d', width: 0.012, inset: 0 },
  { id: 'hearts', name: 'Hearts', color: '#ff4f81', width: 0.012, inset: 0.028 },
  { id: 'stars', name: 'Stars', color: '#ffc145', width: 0.012, inset: 0.028 },
  { id: 'snow', name: 'Snowflakes', color: '#ffffff', width: 0.014, inset: 0.03 },
  { id: 'film', name: 'Film strip', color: '#141218', width: 0.012, inset: 0 },
  { id: 'corners', name: 'Corners', color: '#b8892d', width: 0.004, inset: 0.03 },
  { id: 'gold', name: 'Gold', color: '#c9a227', width: 0.005, inset: 0.022 },
];

function perimeterPoints(x0: number, y0: number, x1: number, y1: number, spacing: number): [number, number][] {
  const pts: [number, number][] = [];
  const sides: [number, number, number, number][] = [
    [x0, y0, x1, y0],
    [x1, y0, x1, y1],
    [x1, y1, x0, y1],
    [x0, y1, x0, y0],
  ];
  for (const [ax, ay, bx, by] of sides) {
    const len = Math.hypot(bx - ax, by - ay);
    const n = Math.max(1, Math.round(len / spacing));
    for (let i = 0; i < n; i++) pts.push([ax + ((bx - ax) * i) / n, ay + ((by - ay) * i) / n]);
  }
  return pts;
}

function heartAt(c: AnyCtx, cx: number, cy: number, s: number) {
  c.beginPath();
  c.moveTo(cx, cy + s * 0.42);
  c.bezierCurveTo(cx - s * 0.52, cy + s * 0.06, cx - s * 0.5, cy - s * 0.46, cx, cy - s * 0.22);
  c.bezierCurveTo(cx + s * 0.5, cy - s * 0.46, cx + s * 0.52, cy + s * 0.06, cx, cy + s * 0.42);
  c.fill();
}

function starAt(c: AnyCtx, cx: number, cy: number, r: number) {
  c.beginPath();
  for (let i = 0; i < 10; i++) {
    const rr = i % 2 === 0 ? r : r * 0.46;
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    if (i === 0) c.moveTo(cx + Math.cos(a) * rr, cy + Math.sin(a) * rr);
    else c.lineTo(cx + Math.cos(a) * rr, cy + Math.sin(a) * rr);
  }
  c.closePath();
  c.fill();
}

function flakeAt(c: AnyCtx, cx: number, cy: number, r: number) {
  c.save();
  c.translate(cx, cy);
  c.lineWidth = r * 0.14;
  c.lineCap = 'round';
  for (let i = 0; i < 6; i++) {
    c.rotate(Math.PI / 3);
    c.beginPath();
    c.moveTo(0, 0);
    c.lineTo(0, -r);
    c.moveTo(0, -r * 0.5);
    c.lineTo(-r * 0.28, -r * 0.78);
    c.moveTo(0, -r * 0.5);
    c.lineTo(r * 0.28, -r * 0.78);
    c.stroke();
  }
  c.restore();
}

function goldPaint(c: AnyCtx, W: number, H: number, base: string): CanvasGradient | string {
  if (base.toLowerCase() !== '#c9a227') return base;
  const g = c.createLinearGradient(0, 0, W, H);
  g.addColorStop(0, '#b8892d');
  g.addColorStop(0.25, '#f3d27a');
  g.addColorStop(0.5, '#b8892d');
  g.addColorStop(0.75, '#f6dc8d');
  g.addColorStop(1, '#a8781f');
  return g;
}

export function drawPageFrame(c: AnyCtx, frame: PageFrame, W: number, H: number): void {
  if (frame.style === 'none') return;
  const S = Math.min(W, H);
  const lw = Math.max(0.5, frame.width * S);
  const ins = frame.inset * S;
  c.save();
  c.fillStyle = frame.color;
  c.strokeStyle = frame.color;
  c.lineWidth = lw;
  const x0 = ins + lw / 2;
  const y0 = ins + lw / 2;
  const x1 = W - ins - lw / 2;
  const y1 = H - ins - lw / 2;
  switch (frame.style) {
    case 'line':
      c.strokeRect(x0, y0, x1 - x0, y1 - y0);
      break;
    case 'double':
    case 'gold': {
      const paint = frame.style === 'gold' ? goldPaint(c, W, H, frame.color) : frame.color;
      c.strokeStyle = paint;
      c.strokeRect(x0, y0, x1 - x0, y1 - y0);
      const d = lw * 2.6;
      c.lineWidth = lw * 0.45;
      c.strokeRect(x0 + d, y0 + d, x1 - x0 - 2 * d, y1 - y0 - 2 * d);
      if (frame.style === 'gold') {
        c.fillStyle = paint;
        for (const [x, y] of [
          [x0 + d / 2, y0 + d / 2],
          [x1 - d / 2, y0 + d / 2],
          [x1 - d / 2, y1 - d / 2],
          [x0 + d / 2, y1 - d / 2],
        ]) {
          c.beginPath();
          c.moveTo(x, y - d * 0.9);
          c.lineTo(x + d * 0.9, y);
          c.lineTo(x, y + d * 0.9);
          c.lineTo(x - d * 0.9, y);
          c.closePath();
          c.fill();
        }
      }
      break;
    }
    case 'stitched':
      c.setLineDash([lw * 3.2, lw * 2.4]);
      c.lineCap = 'round';
      c.strokeRect(x0, y0, x1 - x0, y1 - y0);
      break;
    case 'dots':
      for (const [x, y] of perimeterPoints(x0, y0, x1, y1, lw * 3.4)) {
        c.beginPath();
        c.arc(x, y, lw * 0.9, 0, Math.PI * 2);
        c.fill();
      }
      break;
    case 'scallop': {
      const r = lw * 1.6;
      const band = ins + r * 0.2;
      c.fillRect(0, 0, W, band);
      c.fillRect(0, H - band, W, band);
      c.fillRect(0, 0, band, H);
      c.fillRect(W - band, 0, band, H);
      for (const [x, y] of perimeterPoints(band, band, W - band, H - band, r * 1.75)) {
        c.beginPath();
        c.arc(x, y, r, 0, Math.PI * 2);
        c.fill();
      }
      break;
    }
    case 'candy': {
      const band = lw * 2.4;
      c.beginPath();
      c.rect(0, 0, W, H);
      c.rect(band, band, W - 2 * band, H - 2 * band);
      c.clip('evenodd');
      c.fillStyle = '#ffffff';
      c.fillRect(0, 0, W, H);
      c.fillStyle = frame.color;
      c.save();
      c.translate(W / 2, H / 2);
      c.rotate(-Math.PI / 4);
      const span = Math.hypot(W, H);
      for (let x = -span; x < span; x += band * 1.6) c.fillRect(x, -span, band * 0.8, span * 2);
      c.restore();
      break;
    }
    case 'hearts':
      for (const [x, y] of perimeterPoints(x0, y0, x1, y1, lw * 3.2)) heartAt(c, x, y, lw * 2.1);
      break;
    case 'stars':
      for (const [i, [x, y]] of perimeterPoints(x0, y0, x1, y1, lw * 3.2).entries()) starAt(c, x, y, lw * (i % 2 ? 0.7 : 1.1));
      break;
    case 'snow':
      for (const [i, [x, y]] of perimeterPoints(x0, y0, x1, y1, lw * 3.4).entries()) {
        if (i % 2) {
          c.beginPath();
          c.arc(x, y, lw * 0.28, 0, Math.PI * 2);
          c.fill();
        } else flakeAt(c, x, y, lw * 1.05);
      }
      break;
    case 'film': {
      const band = lw * 5;
      c.fillRect(0, 0, W, band);
      c.fillRect(0, H - band, W, band);
      c.fillStyle = 'rgba(255,255,255,0.92)';
      const hw = lw * 1.5;
      const hh = lw * 2;
      const step = lw * 4;
      for (let x = step / 2; x < W; x += step) {
        for (const y of [band / 2, H - band / 2]) {
          c.beginPath();
          c.roundRect(x - hw / 2, y - hh / 2, hw, hh, hw * 0.25);
          c.fill();
        }
      }
      break;
    }
    case 'corners': {
      const L = S * 0.11;
      c.lineCap = 'round';
      const corners: [number, number, number, number][] = [
        [x0, y0, 1, 1],
        [x1, y0, -1, 1],
        [x1, y1, -1, -1],
        [x0, y1, 1, -1],
      ];
      for (const [x, y, sx, sy] of corners) {
        c.beginPath();
        c.moveTo(x + sx * L, y);
        c.lineTo(x, y);
        c.lineTo(x, y + sy * L);
        c.stroke();
        const d = lw * 3;
        c.beginPath();
        c.moveTo(x + sx * (L * 0.6), y + sy * d);
        c.lineTo(x + sx * d, y + sy * d);
        c.lineTo(x + sx * d, y + sy * (L * 0.6));
        c.stroke();
        c.beginPath();
        c.arc(x + sx * d * 2.2, y + sy * d * 2.2, lw * 1.1, 0, Math.PI * 2);
        c.fill();
      }
      break;
    }
  }
  c.restore();
}
