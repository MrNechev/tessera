// The part of the canvas Path2D API that shape outlines use, so one outline can be built
// as a Path2D (collage renderer) or as SVG path data plus a flattened polygon (canvas
// diagrams, SVG export, hit/attach geometry).

export interface PathSink {
  moveTo(x: number, y: number): void;
  lineTo(x: number, y: number): void;
  quadraticCurveTo(cx: number, cy: number, x: number, y: number): void;
  bezierCurveTo(c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number): void;
  arcTo(x1: number, y1: number, x2: number, y2: number, r: number): void;
  ellipse(cx: number, cy: number, rx: number, ry: number, rotation: number, a0: number, a1: number, ccw?: boolean): void;
  rect(x: number, y: number, w: number, h: number): void;
  closePath(): void;
}

const f = (v: number) => String(Math.round(v * 100) / 100);

/** builds SVG path data (`d`) and a flattened outline (curves sampled) */
export class SvgPath implements PathSink {
  private parts: string[] = [];
  /** flattened sub-paths */
  polys: [number, number][][] = [];
  private cur: [number, number] | null = null;
  private start: [number, number] | null = null;

  private pt(x: number, y: number) {
    this.cur = [x, y];
    this.polys[this.polys.length - 1]?.push([x, y]);
  }

  moveTo(x: number, y: number) {
    this.parts.push(`M${f(x)} ${f(y)}`);
    this.polys.push([]);
    this.start = [x, y];
    this.pt(x, y);
  }

  lineTo(x: number, y: number) {
    if (!this.cur) return this.moveTo(x, y);
    this.parts.push(`L${f(x)} ${f(y)}`);
    this.pt(x, y);
  }

  quadraticCurveTo(cx: number, cy: number, x: number, y: number) {
    if (!this.cur) this.moveTo(cx, cy);
    const [x0, y0] = this.cur!;
    this.parts.push(`Q${f(cx)} ${f(cy)} ${f(x)} ${f(y)}`);
    for (let i = 1; i <= 8; i++) {
      const t = i / 8,
        u = 1 - t;
      this.pt(u * u * x0 + 2 * u * t * cx + t * t * x, u * u * y0 + 2 * u * t * cy + t * t * y);
    }
  }

  bezierCurveTo(c1x: number, c1y: number, c2x: number, c2y: number, x: number, y: number) {
    if (!this.cur) this.moveTo(c1x, c1y);
    const [x0, y0] = this.cur!;
    this.parts.push(`C${f(c1x)} ${f(c1y)} ${f(c2x)} ${f(c2y)} ${f(x)} ${f(y)}`);
    for (let i = 1; i <= 12; i++) {
      const t = i / 12,
        u = 1 - t;
      const a = u * u * u,
        b = 3 * u * u * t,
        c = 3 * u * t * t,
        d = t * t * t;
      this.pt(a * x0 + b * c1x + c * c2x + d * x, a * y0 + b * c1y + c * c2y + d * y);
    }
  }

  arcTo(x1: number, y1: number, x2: number, y2: number, r: number) {
    if (!this.cur) return this.moveTo(x1, y1);
    const [x0, y0] = this.cur;
    const v1x = x0 - x1,
      v1y = y0 - y1,
      v2x = x2 - x1,
      v2y = y2 - y1;
    const l1 = Math.hypot(v1x, v1y),
      l2 = Math.hypot(v2x, v2y);
    const cross = v1x * v2y - v1y * v2x;
    if (!r || !l1 || !l2 || Math.abs(cross) < 1e-9) return this.lineTo(x1, y1);
    const ang = Math.acos(Math.max(-1, Math.min(1, (v1x * v2x + v1y * v2y) / (l1 * l2))));
    const t = r / Math.tan(ang / 2);
    const t1 = [x1 + (v1x / l1) * t, y1 + (v1y / l1) * t] as const;
    const t2 = [x1 + (v2x / l2) * t, y1 + (v2y / l2) * t] as const;
    this.lineTo(t1[0], t1[1]);
    const sweep = cross < 0 ? 1 : 0;
    this.parts.push(`A${f(r)} ${f(r)} 0 0 ${sweep} ${f(t2[0])} ${f(t2[1])}`);
    // sample the arc for the outline: centre is along the bisector
    const bx = v1x / l1 + v2x / l2,
      by = v1y / l1 + v2y / l2;
    const bl = Math.hypot(bx, by) || 1;
    const dc = r / Math.sin(ang / 2);
    const cx = x1 + (bx / bl) * dc,
      cy = y1 + (by / bl) * dc;
    let a0 = Math.atan2(t1[1] - cy, t1[0] - cx),
      a1 = Math.atan2(t2[1] - cy, t2[0] - cx);
    if (sweep && a1 < a0) a1 += Math.PI * 2;
    if (!sweep && a1 > a0) a1 -= Math.PI * 2;
    for (let i = 1; i <= 4; i++) {
      const a = a0 + ((a1 - a0) * i) / 4;
      this.polys[this.polys.length - 1]?.push([cx + Math.cos(a) * r, cy + Math.sin(a) * r]);
    }
    this.cur = [t2[0], t2[1]];
  }

  ellipse(cx: number, cy: number, rx: number, ry: number, rotation: number, a0: number, a1: number, ccw = false) {
    // rotation is not used by the outlines (always 0)
    void rotation;
    let span = ccw ? a0 - a1 : a1 - a0;
    if (span < 0 || (span === 0 && a0 !== a1)) span += Math.PI * 2;
    if (Math.abs(a1 - a0) >= Math.PI * 2) span = Math.PI * 2;
    const at = (a: number) => [cx + Math.cos(a) * rx, cy + Math.sin(a) * ry] as const;
    const s = at(a0);
    if (this.cur) this.lineTo(s[0], s[1]);
    else this.moveTo(s[0], s[1]);
    const dir = ccw ? -1 : 1;
    const steps = Math.max(1, Math.ceil(span / (Math.PI / 2)));
    for (let i = 1; i <= steps; i++) {
      const a = a0 + (dir * span * i) / steps;
      const p = at(a);
      this.parts.push(`A${f(rx)} ${f(ry)} 0 0 ${ccw ? 0 : 1} ${f(p[0])} ${f(p[1])}`);
      for (let j = 1; j <= 6; j++) {
        const q = at(a0 + (dir * span * (i - 1 + j / 6)) / steps);
        this.polys[this.polys.length - 1]?.push([q[0], q[1]]);
      }
      this.cur = [p[0], p[1]];
    }
  }

  rect(x: number, y: number, w: number, h: number) {
    this.moveTo(x, y);
    this.lineTo(x + w, y);
    this.lineTo(x + w, y + h);
    this.lineTo(x, y + h);
    this.closePath();
  }

  closePath() {
    this.parts.push('Z');
    if (this.start) this.cur = [...this.start];
  }

  toString() {
    return this.parts.join('');
  }

  /** the first (outer) sub-path, flattened */
  get outline(): [number, number][] {
    return this.polys[0] ?? [];
  }
}

/**
 * Replay SVG path data that only uses absolute M, L, C, Q and Z (the authored outlines)
 * into a sink, scaled by (sx, sy) and moved by (ox, oy).
 */
export function replaySvg(sink: PathSink, d: string, ox: number, oy: number, sx: number, sy: number) {
  const tokens = d.match(/[MLCQZ]|-?\d*\.?\d+(?:e-?\d+)?/gi) ?? [];
  let cmd = '';
  let i = 0;
  const num = () => parseFloat(tokens[i++]);
  const X = () => ox + num() * sx;
  const Y = () => oy + num() * sy;
  while (i < tokens.length) {
    if (/[MLCQZ]/i.test(tokens[i])) cmd = tokens[i++].toUpperCase();
    if (cmd === 'Z') {
      sink.closePath();
      cmd = '';
      continue;
    }
    if (cmd === 'M') {
      sink.moveTo(X(), Y());
      cmd = 'L';
    } else if (cmd === 'L') sink.lineTo(X(), Y());
    else if (cmd === 'C') {
      const a = X(),
        b = Y(),
        c = X(),
        e = Y(),
        g = X(),
        h = Y();
      sink.bezierCurveTo(a, b, c, e, g, h);
    } else if (cmd === 'Q') {
      const a = X(),
        b = Y(),
        c = X(),
        e = Y();
      sink.quadraticCurveTo(a, b, c, e);
    } else i++; // unknown: skip a number
  }
}
