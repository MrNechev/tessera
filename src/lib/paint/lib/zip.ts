/**
 * The small slice of ZIP that OpenRaster needs, with no dependencies:
 * writing (entries stored uncompressed — layers are PNGs, already
 * compressed — with `mimetype` first, as the spec asks) and reading (stored
 * or deflated entries, as written by GIMP, Krita, MyPaint…).
 */

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

export function crc32(data: Uint8Array): number {
  let c = 0xffffffff;
  for (let i = 0; i < data.length; i++) c = CRC_TABLE[(c ^ data[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export interface ZipEntry {
  name: string;
  data: Uint8Array;
}

const utf8 = new TextEncoder();

/** A ZIP archive of these entries, all stored (no compression), in this order. */
export function writeZip(entries: ZipEntry[], date = new Date()): Uint8Array<ArrayBuffer> {
  const time = ((date.getHours() << 11) | (date.getMinutes() << 5) | (date.getSeconds() >> 1)) & 0xffff;
  const day = (((Math.max(1980, date.getFullYear()) - 1980) << 9) | ((date.getMonth() + 1) << 5) | date.getDate()) & 0xffff;
  const names = entries.map((e) => utf8.encode(e.name));
  const crcs = entries.map((e) => crc32(e.data));
  let size = 22;
  entries.forEach((e, i) => (size += 30 + names[i].length + e.data.length + 46 + names[i].length));
  const out = new Uint8Array(size);
  const v = new DataView(out.buffer);
  let at = 0;
  const offsets: number[] = [];
  entries.forEach((e, i) => {
    offsets.push(at);
    v.setUint32(at, 0x04034b50, true);
    v.setUint16(at + 4, 20, true); // version needed
    v.setUint16(at + 6, 0x0800, true); // UTF-8 names
    v.setUint16(at + 8, 0, true); // stored
    v.setUint16(at + 10, time, true);
    v.setUint16(at + 12, day, true);
    v.setUint32(at + 14, crcs[i], true);
    v.setUint32(at + 18, e.data.length, true);
    v.setUint32(at + 22, e.data.length, true);
    v.setUint16(at + 26, names[i].length, true);
    v.setUint16(at + 28, 0, true);
    out.set(names[i], at + 30);
    at += 30 + names[i].length;
    out.set(e.data, at);
    at += e.data.length;
  });
  const cdStart = at;
  entries.forEach((e, i) => {
    v.setUint32(at, 0x02014b50, true);
    v.setUint16(at + 4, 20, true); // made by
    v.setUint16(at + 6, 20, true);
    v.setUint16(at + 8, 0x0800, true);
    v.setUint16(at + 10, 0, true);
    v.setUint16(at + 12, time, true);
    v.setUint16(at + 14, day, true);
    v.setUint32(at + 16, crcs[i], true);
    v.setUint32(at + 20, e.data.length, true);
    v.setUint32(at + 24, e.data.length, true);
    v.setUint16(at + 28, names[i].length, true);
    // extra, comment, disk, internal attrs = 0
    v.setUint32(at + 38, 0, true); // external attrs
    v.setUint32(at + 42, offsets[i], true);
    out.set(names[i], at + 46);
    at += 46 + names[i].length;
  });
  v.setUint32(at, 0x06054b50, true);
  v.setUint16(at + 8, entries.length, true);
  v.setUint16(at + 10, entries.length, true);
  v.setUint32(at + 12, at - cdStart, true);
  v.setUint32(at + 16, cdStart, true);
  return out;
}

/* --------------------------------------------------------------- inflate */

const LBITS = new Uint8Array(30);
const LBASE = new Uint16Array(30);
const DBITS = new Uint8Array(30);
const DBASE = new Uint16Array(30);
function bitsBase(bits: Uint8Array, base: Uint16Array, delta: number, first: number) {
  for (let i = 0; i < 30 - delta; i++) bits[i + delta] = (i / delta) | 0;
  for (let i = 0, sum = first; i < 30; i++) {
    base[i] = sum;
    sum += 1 << bits[i];
  }
}
bitsBase(LBITS, LBASE, 4, 3);
LBITS[28] = 0;
LBASE[28] = 258;
bitsBase(DBITS, DBASE, 2, 1);
const CLC = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

interface Tree {
  counts: Uint16Array;
  symbols: Uint16Array;
}
const tree = (): Tree => ({ counts: new Uint16Array(16), symbols: new Uint16Array(288) });

function build(t: Tree, lengths: Uint8Array, off: number, num: number) {
  t.counts.fill(0);
  for (let i = 0; i < num; i++) t.counts[lengths[off + i]]++;
  t.counts[0] = 0;
  const offs = new Uint16Array(16);
  for (let i = 0, sum = 0; i < 16; i++) {
    offs[i] = sum;
    sum += t.counts[i];
  }
  for (let i = 0; i < num; i++) if (lengths[off + i]) t.symbols[offs[lengths[off + i]]++] = i;
}

const FIXED_L = tree();
const FIXED_D = tree();
{
  const l = new Uint8Array(288);
  l.fill(8, 0, 144);
  l.fill(9, 144, 256);
  l.fill(7, 256, 280);
  l.fill(8, 280, 288);
  build(FIXED_L, l, 0, 288);
  build(FIXED_D, new Uint8Array(30).fill(5), 0, 30);
}

/** Decompress raw DEFLATE data (RFC 1951). */
export function inflateRaw(src: Uint8Array, sizeHint = 0): Uint8Array {
  let pos = 0;
  let tag = 0;
  let bitcount = 0;
  let out = new Uint8Array(Math.max(1024, sizeHint));
  let n = 0;
  const need = (k: number) => {
    if (n + k <= out.length) return;
    let len = out.length * 2;
    while (len < n + k) len *= 2;
    const next = new Uint8Array(len);
    next.set(out.subarray(0, n));
    out = next;
  };
  const bit = () => {
    if (!bitcount--) {
      if (pos >= src.length) throw new Error('unexpected end of compressed data');
      tag = src[pos++];
      bitcount = 7;
    }
    const b = tag & 1;
    tag >>>= 1;
    return b;
  };
  const bits = (num: number, base: number) => {
    let val = 0;
    for (let i = 0; i < num; i++) val |= bit() << i;
    return val + base;
  };
  const decode = (t: Tree) => {
    let sum = 0;
    let cur = 0;
    let len = 0;
    do {
      cur = 2 * cur + bit();
      if (++len > 15) throw new Error('bad compressed data');
      sum += t.counts[len];
      cur -= t.counts[len];
    } while (cur >= 0);
    return t.symbols[sum + cur];
  };
  const lt = tree();
  const dt = tree();
  const lengths = new Uint8Array(288 + 32);
  let final = 0;
  do {
    final = bit();
    const type = bits(2, 0);
    if (type === 0) {
      bitcount = 0;
      if (pos + 4 > src.length) throw new Error('unexpected end of compressed data');
      const len = src[pos] | (src[pos + 1] << 8);
      pos += 4;
      if (pos + len > src.length) throw new Error('unexpected end of compressed data');
      need(len);
      out.set(src.subarray(pos, pos + len), n);
      n += len;
      pos += len;
      continue;
    }
    let L = FIXED_L;
    let D = FIXED_D;
    if (type === 2) {
      const hlit = bits(5, 257);
      const hdist = bits(5, 1);
      const hclen = bits(4, 4);
      lengths.fill(0, 0, 19);
      for (let i = 0; i < hclen; i++) lengths[CLC[i]] = bits(3, 0);
      const code = tree();
      build(code, lengths, 0, 19);
      lengths.fill(0);
      for (let num = 0; num < hlit + hdist; ) {
        const sym = decode(code);
        if (sym < 16) lengths[num++] = sym;
        else {
          let fill = 0;
          let len: number;
          if (sym === 16) {
            if (!num) throw new Error('bad compressed data');
            fill = lengths[num - 1];
            len = bits(2, 3);
          } else if (sym === 17) len = bits(3, 3);
          else len = bits(7, 11);
          if (num + len > hlit + hdist) throw new Error('bad compressed data');
          lengths.fill(fill, num, num + len);
          num += len;
        }
      }
      build(lt, lengths, 0, hlit);
      build(dt, lengths, hlit, hdist);
      L = lt;
      D = dt;
    } else if (type !== 1) throw new Error('bad compressed data');
    for (;;) {
      let sym = decode(L);
      if (sym < 256) {
        need(1);
        out[n++] = sym;
      } else if (sym === 256) break;
      else {
        sym -= 257;
        const len = bits(LBITS[sym], LBASE[sym]);
        const ds = decode(D);
        const dist = bits(DBITS[ds], DBASE[ds]);
        if (dist > n) throw new Error('bad compressed data');
        need(len);
        for (let i = 0; i < len; i++, n++) out[n] = out[n - dist];
      }
    }
  } while (!final);
  return out.slice(0, n);
}

/* ---------------------------------------------------------------- reading */

/** Entries of a ZIP archive, by name, decompressed on demand. */
export function readZip(bytes: Uint8Array): Map<string, () => Uint8Array> {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 22 - 65535); i--) {
    if (v.getUint32(i, true) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error('not a ZIP file');
  const count = v.getUint16(eocd + 10, true);
  let at = v.getUint32(eocd + 16, true);
  const dec = new TextDecoder();
  const out = new Map<string, () => Uint8Array>();
  for (let i = 0; i < count; i++) {
    if (at + 46 > bytes.length || v.getUint32(at, true) !== 0x02014b50) throw new Error('damaged ZIP file');
    const method = v.getUint16(at + 10, true);
    const csize = v.getUint32(at + 20, true);
    const usize = v.getUint32(at + 24, true);
    const nlen = v.getUint16(at + 28, true);
    const xlen = v.getUint16(at + 30, true);
    const clen = v.getUint16(at + 32, true);
    const local = v.getUint32(at + 42, true);
    const name = dec.decode(bytes.subarray(at + 46, at + 46 + nlen));
    at += 46 + nlen + xlen + clen;
    if (csize === 0xffffffff || local === 0xffffffff) throw new Error('ZIP64 archives are not supported');
    out.set(name, () => {
      if (v.getUint32(local, true) !== 0x04034b50) throw new Error('damaged ZIP file');
      const start = local + 30 + v.getUint16(local + 26, true) + v.getUint16(local + 28, true);
      const raw = bytes.subarray(start, start + csize);
      if (method === 0) return raw;
      if (method === 8) return inflateRaw(raw, usize);
      throw new Error(`unsupported compression in ${name}`);
    });
  }
  return out;
}
