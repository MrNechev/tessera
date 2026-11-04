//! Minimal JPEG marker-segment parser/writer.
//!
//! Only the header (everything before the first SOS) is parsed into
//! segments; the entropy-coded data and everything after it is treated as an
//! opaque tail and copied byte-for-byte, so metadata rewrites never touch the
//! image data.

use crate::error::{Error, Result};

pub(crate) const EXIF_PREFIX: &[u8] = b"Exif\0\0";
pub(crate) const XMP_PREFIX: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
pub(crate) const ICC_PREFIX: &[u8] = b"ICC_PROFILE\0";

/// Max payload of a marker segment (length field includes its own 2 bytes).
const MAX_SEGMENT_PAYLOAD: usize = 65533;

#[derive(Clone, Debug)]
pub(crate) struct Segment<'a> {
    pub marker: u8,
    /// Payload, without the marker and length bytes.
    pub data: &'a [u8],
}

impl Segment<'_> {
    pub fn is_exif(&self) -> bool {
        self.marker == 0xE1 && self.data.starts_with(b"Exif\0")
    }
    pub fn is_xmp(&self) -> bool {
        self.marker == 0xE1 && self.data.starts_with(XMP_PREFIX)
    }
    pub fn is_icc(&self) -> bool {
        self.marker == 0xE2 && self.data.starts_with(ICC_PREFIX)
    }
    pub fn is_jfif(&self) -> bool {
        self.marker == 0xE0 && self.data.starts_with(b"JFIF\0")
    }
    pub fn is_adobe(&self) -> bool {
        self.marker == 0xEE && self.data.starts_with(b"Adobe")
    }
    fn is_sof(&self) -> bool {
        matches!(self.marker, 0xC0..=0xCF) && !matches!(self.marker, 0xC4 | 0xC8 | 0xCC)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Jpeg<'a> {
    /// Segments between SOI and the first SOS, in file order.
    pub segments: Vec<Segment<'a>>,
    /// Everything from the first SOS marker (inclusive) to the end of file.
    pub tail: &'a [u8],
}

/// Result of a (possibly partial) parse.
pub(crate) enum Parse<'a> {
    Done(Jpeg<'a>),
    /// The buffer ended before the first SOS; caller may retry with more data.
    NeedMore,
}

pub(crate) fn parse(buf: &[u8]) -> Result<Jpeg<'_>> {
    match parse_partial(buf)? {
        Parse::Done(j) => Ok(j),
        Parse::NeedMore => Err(Error::decode("JPEG", "file is truncated")),
    }
}

/// Parse header segments. Returns `NeedMore` if `buf` ends inside the header.
pub(crate) fn parse_partial(buf: &[u8]) -> Result<Parse<'_>> {
    if !buf.starts_with(&[0xFF, 0xD8]) {
        return Err(Error::decode("JPEG", "missing SOI marker"));
    }
    let mut pos = 2;
    let mut segments = Vec::new();
    loop {
        // Find marker: one or more 0xFF fill bytes followed by a non-0xFF byte.
        if pos >= buf.len() {
            return Ok(Parse::NeedMore);
        }
        if buf[pos] != 0xFF {
            return Err(Error::decode("JPEG", "corrupt marker structure"));
        }
        let marker_start = pos;
        while pos < buf.len() && buf[pos] == 0xFF {
            pos += 1;
        }
        if pos >= buf.len() {
            return Ok(Parse::NeedMore);
        }
        let marker = buf[pos];
        pos += 1;
        match marker {
            0xDA => {
                return Ok(Parse::Done(Jpeg {
                    segments,
                    tail: &buf[marker_start..],
                }));
            }
            0xD9 => return Err(Error::decode("JPEG", "no image data")),
            0xD8 => return Err(Error::decode("JPEG", "unexpected SOI marker")),
            0x01 | 0xD0..=0xD7 => continue, // standalone markers
            0x00 => return Err(Error::decode("JPEG", "corrupt marker structure")),
            _ => {}
        }
        if pos + 2 > buf.len() {
            return Ok(Parse::NeedMore);
        }
        let len = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        if len < 2 {
            return Err(Error::decode("JPEG", "invalid segment length"));
        }
        let start = pos + 2;
        let end = pos + len;
        if end > buf.len() {
            return Ok(Parse::NeedMore);
        }
        segments.push(Segment {
            marker,
            data: &buf[start..end],
        });
        pos = end;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub components: u8,
    pub progressive: bool,
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
    /// First luminance quantization table in natural order (for quality
    /// estimation).
    pub luma_qtable: Option<[u16; 64]>,
}

impl Jpeg<'_> {
    pub fn info(&self) -> JpegInfo {
        let mut info = JpegInfo::default();
        let mut icc_chunks: Vec<(u8, u8, &[u8])> = Vec::new();
        for s in &self.segments {
            if s.is_sof() && s.data.len() >= 6 && info.width == 0 {
                info.height = u16::from_be_bytes([s.data[1], s.data[2]]) as u32;
                info.width = u16::from_be_bytes([s.data[3], s.data[4]]) as u32;
                info.components = s.data[5];
                info.progressive = matches!(s.marker, 0xC2 | 0xC6 | 0xCA | 0xCE);
            } else if s.is_exif() && info.exif.is_none() {
                let start = if s.data.starts_with(EXIF_PREFIX) {
                    6
                } else {
                    5
                };
                let mut payload = &s.data[start.min(s.data.len())..];
                // Some writers omit the second NUL.
                if !payload.starts_with(b"II") && !payload.starts_with(b"MM") {
                    if let Some(p) = find_tiff_start(payload) {
                        payload = &payload[p..];
                    }
                }
                if payload.len() >= 8 {
                    info.exif = Some(payload.to_vec());
                }
            } else if s.is_xmp() && info.xmp.is_none() {
                info.xmp = Some(s.data[XMP_PREFIX.len()..].to_vec());
            } else if s.is_icc() && s.data.len() >= 14 {
                icc_chunks.push((s.data[12], s.data[13], &s.data[14..]));
            } else if s.marker == 0xDB && info.luma_qtable.is_none() {
                info.luma_qtable = parse_first_qtable(s.data);
            }
        }
        if !icc_chunks.is_empty() {
            icc_chunks.sort_by_key(|c| c.0);
            let total = icc_chunks[0].1 as usize;
            let consistent = icc_chunks.len() == total
                && icc_chunks
                    .iter()
                    .enumerate()
                    .all(|(i, c)| c.0 as usize == i + 1);
            if consistent || total == 0 {
                let mut icc = Vec::new();
                for c in &icc_chunks {
                    icc.extend_from_slice(c.2);
                }
                if !icc.is_empty() {
                    info.icc = Some(icc);
                }
            }
        }
        info
    }
}

fn find_tiff_start(b: &[u8]) -> Option<usize> {
    (0..b.len().saturating_sub(4).min(16))
        .find(|&i| &b[i..i + 4] == b"II*\0" || &b[i..i + 4] == b"MM\0*")
}

fn parse_first_qtable(data: &[u8]) -> Option<[u16; 64]> {
    // Pq/Tq byte then 64 entries (8 or 16 bit) in zigzag order.
    let pq_tq = *data.first()?;
    let precision16 = pq_tq >> 4 != 0;
    let table_id = pq_tq & 0x0F;
    if table_id != 0 {
        return None;
    }
    let mut zz = [0u16; 64];
    for (i, z) in zz.iter_mut().enumerate() {
        *z = if precision16 {
            let o = 1 + i * 2;
            u16::from_be_bytes([*data.get(o)?, *data.get(o + 1)?])
        } else {
            *data.get(1 + i)? as u16
        };
    }
    let mut natural = [0u16; 64];
    for (i, &v) in zz.iter().enumerate() {
        natural[ZIGZAG[i]] = v;
    }
    Some(natural)
}

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Standard IJG luminance table (natural order).
const STD_LUMA: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];

/// Rough IJG-equivalent quality estimate from the luminance table.
pub(crate) fn estimate_quality(q: &[u16; 64]) -> u8 {
    let sum_q: f64 = q.iter().map(|&v| v as f64).sum();
    let sum_std: f64 = STD_LUMA.iter().map(|&v| v as f64).sum();
    let scale = sum_q / sum_std * 100.0; // IJG scale factor in percent
    let quality = if scale <= 100.0 {
        (200.0 - scale) / 2.0
    } else {
        5000.0 / scale
    };
    quality.round().clamp(1.0, 100.0) as u8
}

/// Which non-essential segments to keep when rewriting.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Keep {
    /// Keep other metadata segments (COM, APP3-13/15, MPF, JFXX, extended
    /// XMP) and trailing data after EOI.
    pub other: bool,
}

/// Rewrite `jpeg` replacing EXIF/XMP/ICC segments with the supplied ones.
/// `None` means "drop". The image data (tables, frame header, scans) is
/// copied byte-for-byte.
pub(crate) fn rewrite(
    jpeg: &Jpeg<'_>,
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    xmp: Option<&[u8]>,
    keep: Keep,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(jpeg.tail.len() + 70_000);
    out.extend_from_slice(&[0xFF, 0xD8]);
    let mut rest: &[Segment] = &jpeg.segments;
    if let Some(first) = rest.first() {
        if first.is_jfif() {
            write_segment(&mut out, first.marker, &[first.data])?;
            rest = &rest[1..];
        }
    }
    if let Some(exif) = exif {
        write_segment(&mut out, 0xE1, &[EXIF_PREFIX, exif])?;
    }
    if let Some(xmp) = xmp {
        write_segment(&mut out, 0xE1, &[XMP_PREFIX, xmp])?;
    }
    if let Some(icc) = icc {
        write_icc(&mut out, icc)?;
    }
    for s in rest {
        if s.is_exif() || s.is_xmp() || s.is_icc() {
            continue;
        }
        let essential = !matches!(s.marker, 0xE0..=0xEF | 0xFE) || s.is_adobe() || s.is_jfif();
        if !essential && !keep.other {
            continue;
        }
        write_segment(&mut out, s.marker, &[s.data])?;
    }
    if keep.other {
        out.extend_from_slice(jpeg.tail);
    } else {
        let end = find_eoi(jpeg.tail).unwrap_or(jpeg.tail.len());
        out.extend_from_slice(&jpeg.tail[..end]);
    }
    Ok(out)
}

/// Check that an EXIF/XMP payload fits in one APP1 segment.
pub(crate) fn exif_fits(exif: &[u8]) -> bool {
    exif.len() + EXIF_PREFIX.len() <= MAX_SEGMENT_PAYLOAD
}

pub(crate) fn xmp_fits(xmp: &[u8]) -> bool {
    xmp.len() + XMP_PREFIX.len() <= MAX_SEGMENT_PAYLOAD
}

fn write_segment(out: &mut Vec<u8>, marker: u8, parts: &[&[u8]]) -> Result<()> {
    let len: usize = parts.iter().map(|p| p.len()).sum::<usize>() + 2;
    if len > 65535 {
        return Err(Error::Metadata(format!(
            "JPEG segment too large ({} bytes)",
            len
        )));
    }
    out.extend_from_slice(&[0xFF, marker]);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    for p in parts {
        out.extend_from_slice(p);
    }
    Ok(())
}

fn write_icc(out: &mut Vec<u8>, icc: &[u8]) -> Result<()> {
    const CHUNK: usize = MAX_SEGMENT_PAYLOAD - 14;
    let count = icc.len().div_ceil(CHUNK);
    if count == 0 {
        return Ok(());
    }
    if count > 255 {
        return Err(Error::Metadata("ICC profile too large for JPEG".into()));
    }
    for (i, chunk) in icc.chunks(CHUNK).enumerate() {
        let hdr = [(i + 1) as u8, count as u8];
        write_segment(out, 0xE2, &[ICC_PREFIX, &hdr, chunk])?;
    }
    Ok(())
}

/// Find the end (exclusive) of the EOI marker in `tail` (which starts at the
/// first SOS). Walks scans and inter-scan segments properly.
pub(crate) fn find_eoi(tail: &[u8]) -> Option<usize> {
    let mut pos = 0;
    loop {
        // At a marker.
        if pos + 1 >= tail.len() || tail[pos] != 0xFF {
            return None;
        }
        while pos < tail.len() && tail[pos] == 0xFF {
            pos += 1;
        }
        let marker = *tail.get(pos)?;
        pos += 1;
        match marker {
            0xD9 => return Some(pos),
            0x01 | 0xD0..=0xD7 => continue,
            _ => {}
        }
        let len = u16::from_be_bytes([*tail.get(pos)?, *tail.get(pos + 1)?]) as usize;
        pos += len;
        if pos > tail.len() {
            return None;
        }
        if marker == 0xDA {
            // Skip entropy-coded data until a real marker.
            loop {
                let ff = tail[pos..].iter().position(|&b| b == 0xFF)?;
                pos += ff;
                let next = *tail.get(pos + 1)?;
                if next == 0x00 || (0xD0..=0xD7).contains(&next) || next == 0xFF {
                    pos += if next == 0xFF { 1 } else { 2 };
                    continue;
                }
                break;
            }
        }
    }
}

/// Range of `bytes` that holds the image data proper: from the first DQT/
/// DHT/SOF/SOS-related segment through EOI, excluding APPn/COM segments.
/// Used by tests to assert that a metadata rewrite was lossless.
pub fn image_data_fingerprint(bytes: &[u8]) -> Result<Vec<u8>> {
    let j = parse(bytes)?;
    let mut out = Vec::new();
    for s in &j.segments {
        if matches!(s.marker, 0xE0..=0xEF | 0xFE) {
            continue;
        }
        out.push(s.marker);
        out.extend_from_slice(s.data);
    }
    let end = find_eoi(j.tail).unwrap_or(j.tail.len());
    out.extend_from_slice(&j.tail[..end]);
    Ok(out)
}
