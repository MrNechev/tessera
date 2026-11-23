//! Minimal EXIF/TIFF reader: orientation, camera, capture time and GPS — the capture
//! metadata Tessera uses. Works on JPEG APP1 payloads and TIFF-based RAW files.

use std::io::Read;

use chrono::{FixedOffset, Local, NaiveDateTime, TimeZone};

/// When, where, with what, and how it is rotated.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub orientation: i64,
    pub camera: String,
    /// Capture time, unix ms.
    pub taken: Option<i64>,
    pub lat: f64,
    pub lon: f64,
    pub has_gps: bool,
}

/// The TIFF part of the first EXIF APP1 segment of a JPEG, or `None`.
pub fn jpeg_exif(r: impl Read) -> Option<Vec<u8>> {
    let mut br = std::io::BufReader::with_capacity(8192, r);
    let mut soi = [0u8; 2];
    br.read_exact(&mut soi).ok()?;
    if soi != [0xFF, 0xD8] {
        return None;
    }
    for _ in 0..32 {
        let mut hdr = [0u8; 4];
        br.read_exact(&mut hdr[..2]).ok()?;
        if hdr[0] != 0xFF {
            return None;
        }
        let marker = hdr[1];
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0xFF {
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        br.read_exact(&mut hdr[2..4]).ok()?;
        let n = u16::from_be_bytes([hdr[2], hdr[3]]) as i64 - 2;
        if n < 0 {
            return None;
        }
        let n = n as usize;
        if marker != 0xE1 {
            let skipped =
                std::io::copy(&mut (&mut br).take(n as u64), &mut std::io::sink()).ok()?;
            if skipped as usize != n {
                return None;
            }
            continue;
        }
        let mut buf = vec![0u8; n];
        br.read_exact(&mut buf).ok()?;
        if buf.len() >= 14 && &buf[..6] == b"Exif\x00\x00" {
            return Some(buf[6..].to_vec());
        }
    }
    None
}

/// EXIF orientation of a JPEG (1..8) or 0.
pub fn jpeg_orientation(r: impl Read) -> i64 {
    jpeg_exif(r)
        .map(|t| parse_tiff(&t).orientation)
        .unwrap_or(0)
}

#[derive(Clone, Copy)]
enum Order {
    Le,
    Be,
}

struct Tiff<'a> {
    b: &'a [u8],
    bo: Order,
}

#[derive(Clone)]
struct IfdEntry<'a> {
    typ: u16,
    count: u32,
    val: &'a [u8],
}

fn type_size(t: u16) -> usize {
    match t {
        1 | 2 | 6 | 7 => 1,
        3 | 8 => 2,
        4 | 9 | 11 => 4,
        5 | 10 | 12 => 8,
        _ => 0,
    }
}

impl<'a> Tiff<'a> {
    fn u16(&self, b: &[u8]) -> u16 {
        match self.bo {
            Order::Le => u16::from_le_bytes([b[0], b[1]]),
            Order::Be => u16::from_be_bytes([b[0], b[1]]),
        }
    }
    fn u32(&self, b: &[u8]) -> u32 {
        match self.bo {
            Order::Le => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            Order::Be => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        }
    }

    fn ifd(&self, off: usize) -> Option<std::collections::HashMap<u16, IfdEntry<'a>>> {
        let b = self.b;
        if off < 8 || off + 2 > b.len() {
            return None;
        }
        let n = self.u16(&b[off..]) as usize;
        if n > 1000 {
            return None;
        }
        let mut out = std::collections::HashMap::with_capacity(n);
        for i in 0..n {
            let e = off + 2 + i * 12;
            if e + 12 > b.len() {
                break;
            }
            let tag = self.u16(&b[e..]);
            let typ = self.u16(&b[e + 2..]);
            let count = self.u32(&b[e + 4..]);
            let sz = type_size(typ).saturating_mul(count as usize);
            if sz == 0 || count > 1 << 24 {
                continue;
            }
            let val = if sz <= 4 {
                &b[e + 8..e + 8 + sz]
            } else {
                let vo = self.u32(&b[e + 8..]) as usize;
                if vo.checked_add(sz).map(|end| end > b.len()).unwrap_or(true) {
                    continue;
                }
                &b[vo..vo + sz]
            };
            out.insert(tag, IfdEntry { typ, count, val });
        }
        Some(out)
    }

    fn uint(&self, e: &IfdEntry<'_>) -> i64 {
        match e.typ {
            3 | 8 => self.u16(e.val) as i64,
            4 | 9 => self.u32(e.val) as i64,
            1 | 6 | 7 => e.val[0] as i64,
            _ => 0,
        }
    }

    fn str(&self, e: Option<&IfdEntry<'_>>) -> String {
        let Some(e) = e else { return String::new() };
        if e.typ != 2 && e.typ != 7 {
            return String::new();
        }
        let s = String::from_utf8_lossy(e.val);
        s.trim_end_matches(['\0', ' ']).trim().to_string()
    }

    fn rationals(&self, e: Option<&IfdEntry<'_>>) -> Vec<f64> {
        let Some(e) = e else { return Vec::new() };
        if e.typ != 5 && e.typ != 10 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(e.count as usize);
        let mut i = 0;
        while i + 8 <= e.val.len() {
            let (num, den) = if e.typ == 10 {
                (
                    self.u32(&e.val[i..]) as i32 as f64,
                    self.u32(&e.val[i + 4..]) as i32 as f64,
                )
            } else {
                (
                    self.u32(&e.val[i..]) as f64,
                    self.u32(&e.val[i + 4..]) as f64,
                )
            };
            out.push(if den == 0.0 { 0.0 } else { num / den });
            i += 8;
        }
        out
    }
}

/// Reads the EXIF fields Tessera cares about from a TIFF structure (an EXIF APP1 payload,
/// or a TIFF-based RAW file).
pub fn parse_tiff(b: &[u8]) -> Meta {
    let mut m = Meta::default();
    if b.len() < 8 {
        return m;
    }
    let bo = match &b[..2] {
        b"II" => Order::Le,
        b"MM" => Order::Be,
        _ => return m,
    };
    let t = Tiff { b, bo };
    let Some(ifd0) = t.ifd(t.u32(&b[4..8]) as usize) else {
        return m;
    };
    if let Some(e) = ifd0.get(&0x0112) {
        m.orientation = t.uint(e);
    }
    let mk = t.str(ifd0.get(&0x010F));
    let model = t.str(ifd0.get(&0x0110));
    m.camera = camera_name(&mk, &model);
    let mut taken = String::new();
    let mut offset = String::new();
    if let Some(e) = ifd0.get(&0x0132) {
        taken = t.str(Some(e));
    }
    if let Some(e) = ifd0.get(&0x8769) {
        if let Some(ex) = t.ifd(t.uint(e).max(0) as usize) {
            let s = t.str(ex.get(&0x9003));
            if !s.is_empty() {
                taken = s;
            }
            offset = t.str(ex.get(&0x9011));
        }
    }
    m.taken = parse_exif_time(&taken, &offset);
    if let Some(e) = ifd0.get(&0x8825) {
        if let Some(g) = t.ifd(t.uint(e).max(0) as usize) {
            let lat = t.rationals(g.get(&2));
            let lon = t.rationals(g.get(&4));
            if lat.len() == 3 && lon.len() == 3 {
                let mut la = lat[0] + lat[1] / 60.0 + lat[2] / 3600.0;
                let mut lo = lon[0] + lon[1] / 60.0 + lon[2] / 3600.0;
                if t.str(g.get(&1)).starts_with('S') {
                    la = -la;
                }
                if t.str(g.get(&3)).starts_with('W') {
                    lo = -lo;
                }
                if valid_gps(la, lo) {
                    m.lat = la;
                    m.lon = lo;
                    m.has_gps = true;
                }
            }
        }
    }
    m
}

pub fn valid_gps(la: f64, lo: f64) -> bool {
    !la.is_nan()
        && !lo.is_nan()
        && la.abs() <= 90.0
        && lo.abs() <= 180.0
        && (la != 0.0 || lo != 0.0)
}

/// Joins make and model without repeating the brand ("Apple iPhone 15 Pro", not "Apple
/// Apple iPhone 15 Pro").
pub fn camera_name(mk: &str, model: &str) -> String {
    let mk = mk.trim();
    let model = model.trim();
    if mk.is_empty() {
        return model.to_string();
    }
    if model.is_empty() {
        return mk.to_string();
    }
    let first = mk.split_whitespace().next().unwrap_or("").to_lowercase();
    if model.to_lowercase().starts_with(&first) {
        return model.to_string();
    }
    let mk = match mk.to_uppercase().as_str() {
        "NIKON CORPORATION" => "Nikon",
        "OLYMPUS IMAGING CORP." | "OLYMPUS CORPORATION" => "Olympus",
        _ => mk,
    };
    format!("{mk} {model}")
}

fn strict_digits(s: &str, positions: &[std::ops::Range<usize>]) -> bool {
    positions.iter().all(|r| {
        s.get(r.clone())
            .map(|p| p.bytes().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
    })
}

/// Parses "2024:07:14 18:22:05" (+ optional "+02:00"). Without an offset the time is taken
/// as local time of the server (that's how cameras store it). Returns unix ms.
pub fn parse_exif_time(s: &str, offset: &str) -> Option<i64> {
    let s = s.trim_end_matches('\0').trim();
    if s.len() < 19 || s.starts_with("0000") || !s.is_char_boundary(19) {
        return None;
    }
    let s = &s[..19];
    let b = s.as_bytes();
    if !strict_digits(s, &[0..4, 5..7, 8..10, 11..13, 14..16, 17..19])
        || b[4] != b':'
        || b[7] != b':'
        || b[10] != b' '
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let naive = NaiveDateTime::parse_from_str(s, "%Y:%m:%d %H:%M:%S").ok()?;
    if !offset.is_empty() {
        if let Some(off) = parse_offset(offset) {
            if let Some(t) = off.from_local_datetime(&naive).single() {
                return Some(t.timestamp_millis());
            }
        }
    }
    let t = Local.from_local_datetime(&naive);
    t.earliest()
        .or_else(|| t.latest())
        .map(|t| t.timestamp_millis())
        .or_else(|| Some(naive.and_utc().timestamp_millis()))
}

/// "+02:00" / "-05:30" (Go layout "-07:00").
fn parse_offset(s: &str) -> Option<FixedOffset> {
    let b = s.as_bytes();
    if b.len() != 6 || b[3] != b':' || !(b[0] == b'+' || b[0] == b'-') {
        return None;
    }
    let h: i32 = s[1..3].parse().ok()?;
    let m: i32 = s[4..6].parse().ok()?;
    if h > 24 || m > 59 {
        return None;
    }
    let secs = h * 3600 + m * 60;
    if b[0] == b'-' {
        FixedOffset::west_opt(secs)
    } else {
        FixedOffset::east_opt(secs)
    }
}

/// Parses QuickTime/Android locations like "+48.8577+002.2950+035.000/".
pub fn parse_iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    let s = s.strip_suffix('/').unwrap_or(s);
    if s.len() < 4 {
        return None;
    }
    let b = s.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    for i in 1..b.len() {
        if b[i] == b'+' || b[i] == b'-' {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    parts.push(&s[start..]);
    if parts.len() < 2 {
        return None;
    }
    let la: f64 = go_parse_float(parts[0])?;
    let lo: f64 = go_parse_float(parts[1])?;
    if !valid_gps(la, lo) {
        return None;
    }
    Some((la, lo))
}

/// `strconv.ParseFloat` accepts a leading '+'; Rust does too, but not some Go-only forms
/// we don't need. Kept as a single place for float parsing of metadata.
fn go_parse_float(s: &str) -> Option<f64> {
    s.parse().ok()
}

/// Parses a vipsheader EXIF rational field such as
/// "48/1 51/1 2436/100 (48, 51, 24.36, Rational, 3 components, 24 bytes)".
pub fn parse_vips_gps(v: &str) -> Option<f64> {
    let v = match v.find('(') {
        Some(i) => &v[..i],
        None => v,
    };
    let f: Vec<&str> = v.split_whitespace().collect();
    if f.len() < 3 {
        return None;
    }
    let mut parts = [0f64; 3];
    for i in 0..3 {
        let (n, d) = match f[i].split_once('/') {
            Some((n, d)) => (n, Some(d)),
            None => (f[i], None),
        };
        let num: f64 = n.parse().ok()?;
        let den: f64 = match d {
            Some(d) => d.parse().ok()?,
            None => 1.0,
        };
        if den == 0.0 {
            return None;
        }
        parts[i] = num / den;
    }
    Some(parts[0] + parts[1] / 60.0 + parts[2] / 3600.0)
}

/// Strips vipsheader's "(..., ASCII, n components, m bytes)" suffix.
pub fn vips_string(v: &str) -> String {
    let v = match v.rfind(" (") {
        Some(i) => &v[..i],
        None => v,
    };
    v.trim().to_string()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The smallest JPEG prefix with an EXIF APP1 orientation tag.
    fn jpeg_with_orientation(o: u16, le: bool) -> Vec<u8> {
        let w16 = |v: u16| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let w32 = |v: u32| if le { v.to_le_bytes() } else { v.to_be_bytes() };
        let mut tiff = Vec::new();
        tiff.extend_from_slice(if le { b"II" } else { b"MM" });
        tiff.extend_from_slice(&w16(42));
        tiff.extend_from_slice(&w32(8));
        tiff.extend_from_slice(&w16(1));
        tiff.extend_from_slice(&w16(0x0112));
        tiff.extend_from_slice(&w16(3));
        tiff.extend_from_slice(&w32(1));
        tiff.extend_from_slice(&w16(o));
        tiff.extend_from_slice(&w16(0));
        tiff.extend_from_slice(&w32(0));
        let mut app1 = b"Exif\x00\x00".to_vec();
        app1.extend_from_slice(&tiff);
        let mut b = vec![0xFF, 0xD8, 0xFF, 0xE1];
        b.extend_from_slice(&((app1.len() + 2) as u16).to_be_bytes());
        b.extend_from_slice(&app1);
        b.extend_from_slice(&[0xFF, 0xDA]);
        b
    }

    #[test]
    fn jpeg_orientation_both_orders() {
        for le in [true, false] {
            for o in [1u16, 3, 6, 8] {
                assert_eq!(
                    jpeg_orientation(&jpeg_with_orientation(o, le)[..]),
                    o as i64,
                    "le={le} o={o}"
                );
            }
        }
        assert_eq!(jpeg_orientation(&b"not a jpeg"[..]), 0);
    }

    /// A little-endian TIFF/EXIF block with make/model/orientation, an EXIF IFD with
    /// DateTimeOriginal+offset and a GPS IFD.
    pub(crate) fn build_tiff() -> Vec<u8> {
        struct Ent {
            tag: u16,
            typ: u16,
            count: u32,
            data: Vec<u8>,
        }
        let rat = |vals: &[(u32, u32)]| {
            let mut b = Vec::new();
            for (n, d) in vals {
                b.extend_from_slice(&n.to_le_bytes());
                b.extend_from_slice(&d.to_le_bytes());
            }
            b
        };
        let ascii = |s: &str| {
            let mut v = s.as_bytes().to_vec();
            v.push(0);
            v
        };
        let mut ifd0 = vec![
            Ent {
                tag: 0x010F,
                typ: 2,
                count: 0,
                data: ascii("Apple"),
            },
            Ent {
                tag: 0x0110,
                typ: 2,
                count: 0,
                data: ascii("iPhone 15 Pro"),
            },
            Ent {
                tag: 0x0112,
                typ: 3,
                count: 1,
                data: 6u16.to_le_bytes().to_vec(),
            },
            Ent {
                tag: 0x8769,
                typ: 4,
                count: 1,
                data: vec![],
            },
            Ent {
                tag: 0x8825,
                typ: 4,
                count: 1,
                data: vec![],
            },
        ];
        let exif = vec![
            Ent {
                tag: 0x9003,
                typ: 2,
                count: 0,
                data: ascii("2024:07:14 18:22:05"),
            },
            Ent {
                tag: 0x9011,
                typ: 2,
                count: 0,
                data: ascii("+02:00"),
            },
        ];
        let gps = vec![
            Ent {
                tag: 0x0001,
                typ: 2,
                count: 0,
                data: ascii("N"),
            },
            Ent {
                tag: 0x0002,
                typ: 5,
                count: 3,
                data: rat(&[(64, 1), (8, 1), (4776, 100)]),
            },
            Ent {
                tag: 0x0003,
                typ: 2,
                count: 0,
                data: ascii("W"),
            },
            Ent {
                tag: 0x0004,
                typ: 5,
                count: 3,
                data: rat(&[(21, 1), (56, 1), (3336, 100)]),
            },
        ];
        let size = |n: usize| 2 + 12 * n + 4;
        let off1 = 8 + size(ifd0.len());
        let off2 = off1 + size(exif.len());
        let data_off = off2 + size(gps.len());
        ifd0[3].data = (off1 as u32).to_le_bytes().to_vec();
        ifd0[4].data = (off2 as u32).to_le_bytes().to_vec();
        let mut buf = b"II*\x00".to_vec();
        buf.extend_from_slice(&8u32.to_le_bytes());
        let mut data = Vec::new();
        for es in [&ifd0, &exif, &gps] {
            buf.extend_from_slice(&(es.len() as u16).to_le_bytes());
            for e in es.iter() {
                let cnt = if e.typ == 2 {
                    e.data.len() as u32
                } else {
                    e.count
                };
                let mut rec = vec![0u8; 12];
                rec[0..2].copy_from_slice(&e.tag.to_le_bytes());
                rec[2..4].copy_from_slice(&e.typ.to_le_bytes());
                rec[4..8].copy_from_slice(&cnt.to_le_bytes());
                if e.data.len() <= 4 {
                    rec[8..8 + e.data.len()].copy_from_slice(&e.data);
                } else {
                    rec[8..12].copy_from_slice(&((data_off + data.len()) as u32).to_le_bytes());
                    data.extend_from_slice(&e.data);
                }
                buf.extend_from_slice(&rec);
            }
            buf.extend_from_slice(&0u32.to_le_bytes());
        }
        buf.extend_from_slice(&data);
        buf
    }

    #[test]
    fn parse_tiff_fields() {
        let m = parse_tiff(&build_tiff());
        assert_eq!(m.orientation, 6);
        assert_eq!(m.camera, "Apple iPhone 15 Pro");
        let want = chrono::Utc
            .with_ymd_and_hms(2024, 7, 14, 16, 22, 5)
            .unwrap()
            .timestamp_millis();
        assert_eq!(m.taken, Some(want));
        assert!(m.has_gps && m.lat > 64.14 && m.lat < 64.15 && m.lon < -21.94 && m.lon > -21.95);
    }

    #[test]
    fn iso6709_and_camera() {
        let (la, lo) = parse_iso6709("+48.8577+002.2950+035.000/").unwrap();
        assert_eq!((la, lo), (48.8577, 2.295));
        assert!(
            parse_iso6709("+00.0000+000.0000/").is_none(),
            "0,0 accepted"
        );
        assert_eq!(camera_name("NIKON CORPORATION", "NIKON Z 6"), "NIKON Z 6");
        assert_eq!(camera_name("Canon", "EOS R5"), "Canon EOS R5");
        let v =
            parse_vips_gps("48/1 51/1 2436/100 (48, 51, 24.36, Rational, 3 components, 24 bytes)")
                .unwrap();
        assert!(v > 48.856 && v < 48.857);
    }
}
