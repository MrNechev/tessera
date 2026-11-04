//! EXIF helpers: a tiny in-place TIFF walker (for patching values without
//! re-laying-out the structure) and kamadak-exif based filtering/rebuilding.

use exif::{Context, Field, In, Tag, Value};

/// Minimal read-only view on a TIFF structure.
struct TiffView<'a> {
    buf: &'a [u8],
    le: bool,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    typ: u16,
    count: u32,
    /// Offset of the 4-byte value/offset field.
    value_pos: usize,
}

impl<'a> TiffView<'a> {
    fn new(buf: &'a [u8]) -> Option<Self> {
        let le = match buf.get(0..4)? {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        Some(TiffView { buf, le })
    }

    fn u16(&self, pos: usize) -> Option<u16> {
        let b = self.buf.get(pos..pos + 2)?;
        Some(if self.le {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    }

    fn u32(&self, pos: usize) -> Option<u32> {
        let b = self.buf.get(pos..pos + 4)?;
        Some(if self.le {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    }

    fn ifd0(&self) -> Option<usize> {
        Some(self.u32(4)? as usize)
    }

    fn find(&self, ifd: usize, tag: u16) -> Option<Entry> {
        let n = self.u16(ifd)? as usize;
        if n > 4096 {
            return None;
        }
        for i in 0..n {
            let p = ifd + 2 + i * 12;
            if self.u16(p)? == tag {
                return Some(Entry {
                    typ: self.u16(p + 2)?,
                    count: self.u32(p + 4)?,
                    value_pos: p + 8,
                });
            }
        }
        None
    }

    fn scalar(&self, e: Entry) -> Option<u32> {
        if e.count != 1 {
            return None;
        }
        match e.typ {
            3 => self.u16(e.value_pos).map(u32::from),
            4 => self.u32(e.value_pos),
            _ => None,
        }
    }
}

/// EXIF orientation (1..=8), 1 if absent or invalid.
pub(crate) fn read_orientation(exif: &[u8]) -> u16 {
    (|| {
        let t = TiffView::new(exif)?;
        let e = t.find(t.ifd0()?, 0x0112)?;
        let v = t.scalar(e)? as u16;
        (1..=8).contains(&v).then_some(v)
    })()
    .unwrap_or(1)
}

fn write_scalar(buf: &mut [u8], le: bool, e: Entry, value: u32) -> bool {
    match e.typ {
        3 if value <= u16::MAX as u32 => {
            let b = if le {
                (value as u16).to_le_bytes()
            } else {
                (value as u16).to_be_bytes()
            };
            buf[e.value_pos..e.value_pos + 2].copy_from_slice(&b);
            true
        }
        4 => {
            let b = if le {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            };
            buf[e.value_pos..e.value_pos + 4].copy_from_slice(&b);
            true
        }
        _ => false,
    }
}

/// Patch Orientation and (optionally) PixelXDimension/PixelYDimension in
/// place, preserving every other byte (MakerNote offsets stay valid).
pub(crate) fn patch_in_place(exif: &mut [u8], orientation: Option<u16>, dims: Option<(u32, u32)>) {
    let Some(t) = TiffView::new(exif) else { return };
    let le = t.le;
    let Some(ifd0) = t.ifd0() else { return };
    let orient_entry = t.find(ifd0, 0x0112).filter(|e| e.count == 1);
    let exif_ifd = t
        .find(ifd0, 0x8769)
        .and_then(|e| t.scalar(Entry { typ: 4, ..e }))
        .map(|v| v as usize);
    let (px, py) = match exif_ifd {
        Some(ifd) => (
            t.find(ifd, 0xA002).filter(|e| e.count == 1),
            t.find(ifd, 0xA003).filter(|e| e.count == 1),
        ),
        None => (None, None),
    };
    if let (Some(o), Some(e)) = (orientation, orient_entry) {
        write_scalar(exif, le, e, o as u32);
    }
    if let Some((w, h)) = dims {
        if let Some(e) = px {
            write_scalar(exif, le, e, w);
        }
        if let Some(e) = py {
            write_scalar(exif, le, e, h);
        }
    }
}

/// Parse with kamadak-exif, tolerating partially broken data.
pub(crate) fn parse(exif: &[u8]) -> Option<exif::Exif> {
    let mut r = exif::Reader::new();
    r.continue_on_error(true);
    match r.read_raw(exif.to_vec()) {
        Ok(e) => Some(e),
        Err(exif::Error::PartialResult(p)) => Some(p.into_inner().0),
        Err(_) => None,
    }
}

/// Metadata groups used by the custom metadata policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    /// Rendering-relevant or format bookkeeping; always kept.
    Structural,
    DateTime,
    Camera,
    Exposure,
    Location,
    Author,
    /// Never copied into a rebuilt EXIF block.
    Drop,
}

pub(crate) fn classify(tag: Tag) -> Group {
    let n = tag.number();
    match tag.context() {
        Context::Gps => Group::Location,
        Context::Interop => Group::Structural,
        Context::Tiff => match n {
            0x0112 | 0x011A | 0x011B | 0x0128 | 0x0213 | 0x013E | 0x013F => Group::Structural,
            0x0132 => Group::DateTime,
            0x010F | 0x0110 | 0x0131 | 0x013C => Group::Camera,
            0x010D | 0x010E | 0x013B | 0x8298 | 0x9C9B..=0x9C9F | 0x4746 | 0x4749 => Group::Author,
            _ => Group::Drop,
        },
        Context::Exif => match n {
            0x9000 | 0xA000 | 0xA001 | 0x9101 | 0xA500 => Group::Structural,
            // Pixel dimensions are handled separately.
            0xA002 | 0xA003 => Group::Drop,
            0x9003 | 0x9004 | 0x9010..=0x9012 | 0x9290..=0x9292 => Group::DateTime,
            0xA420 | 0xA431..=0xA435 | 0xA439..=0xA43C => Group::Camera,
            0x9286 | 0xA430 | 0xA436..=0xA438 => Group::Author,
            0x829A
            | 0x829D
            | 0x8822
            | 0x8824
            | 0x8827
            | 0x8828
            | 0x8830..=0x8835
            | 0x9201..=0x920A
            | 0x9214
            | 0x9400..=0x9405
            | 0xA20B
            | 0xA20C
            | 0xA20E
            | 0xA20F
            | 0xA210
            | 0xA214
            | 0xA215
            | 0xA217
            | 0xA300..=0xA302
            | 0xA401..=0xA40C
            | 0xA460..=0xA462 => Group::Exposure,
            // MakerNote (0x927C) is dropped: its internal offsets break when
            // the EXIF block is re-laid-out.
            _ => Group::Drop,
        },
        _ => Group::Drop,
    }
}

/// Which groups to keep when rebuilding.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GroupMask {
    pub date_time: bool,
    pub camera: bool,
    pub exposure: bool,
    pub location: bool,
    pub author: bool,
}

impl GroupMask {
    pub const ALL: GroupMask = GroupMask {
        date_time: true,
        camera: true,
        exposure: true,
        location: true,
        author: true,
    };

    fn keeps(&self, g: Group) -> bool {
        match g {
            Group::Structural => true,
            Group::DateTime => self.date_time,
            Group::Camera => self.camera,
            Group::Exposure => self.exposure,
            Group::Location => self.location,
            Group::Author => self.author,
            Group::Drop => false,
        }
    }
}

fn writable(v: &Value) -> bool {
    !matches!(v, Value::Unknown(..))
}

/// Rebuild an EXIF block from the primary-image fields of `exif`, keeping the
/// groups in `mask`. Thumbnail (IFD1) and MakerNote are always dropped.
/// Orientation is set to `orientation`; pixel dimensions are written when
/// `dims` is given. Returns `None` when nothing but structural fields would
/// remain.
pub(crate) fn rebuild(
    exif: &exif::Exif,
    mask: GroupMask,
    orientation: u16,
    dims: Option<(u32, u32)>,
) -> Option<Vec<u8>> {
    let mut fields: Vec<Field> = Vec::new();
    let mut has_content = false;
    let mut has_exif_ctx = false;
    for f in exif.fields() {
        if f.ifd_num != In::PRIMARY || !writable(&f.value) {
            continue;
        }
        let g = classify(f.tag);
        if !mask.keeps(g) || f.tag == Tag::Orientation {
            continue;
        }
        if g != Group::Structural {
            has_content = true;
        }
        if f.tag.context() == Context::Exif {
            has_exif_ctx = true;
        }
        fields.push(f.clone());
    }
    if !has_content {
        return None;
    }
    if !has_exif_ctx {
        // Interop IFD can only hang off the Exif IFD.
        fields.retain(|f| f.tag.context() != Context::Interop);
    }
    fields.push(Field {
        tag: Tag::Orientation,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![orientation.clamp(1, 8)]),
    });
    if let Some((w, h)) = dims {
        if has_exif_ctx {
            fields.push(Field {
                tag: Tag::PixelXDimension,
                ifd_num: In::PRIMARY,
                value: Value::Long(vec![w]),
            });
            fields.push(Field {
                tag: Tag::PixelYDimension,
                ifd_num: In::PRIMARY,
                value: Value::Long(vec![h]),
            });
        }
    }
    write_fields(&fields, exif.little_endian())
}

pub(crate) fn write_fields(fields: &[Field], little_endian: bool) -> Option<Vec<u8>> {
    let mut w = exif::experimental::Writer::new();
    for f in fields {
        w.push_field(f);
    }
    let mut cur = std::io::Cursor::new(Vec::new());
    w.write(&mut cur, little_endian).ok()?;
    Some(cur.into_inner())
}

/// Tags that describe the image structure of a TIFF file; never copied into
/// an EXIF block derived from a TIFF.
fn is_tiff_structure(tag: Tag) -> bool {
    tag.context() == Context::Tiff
        && matches!(
            tag.number(),
            0x00FE | 0x00FF | 0x0100..=0x0103 | 0x0106 | 0x0107 | 0x0111 | 0x0115..=0x0119
                | 0x011C | 0x0122 | 0x0123 | 0x0124 | 0x0125 | 0x0129 | 0x013D | 0x0140..=0x0147
                | 0x014A | 0x0152..=0x0159 | 0x0200..=0x0212 | 0x02BC | 0x83BB | 0x8649
                | 0x8773 | 0x935C
        )
}

/// (exif, icc, xmp)
pub(crate) type TiffMeta = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

/// Metadata of a TIFF file: an EXIF block built from its tags (the file *is*
/// a TIFF structure, but image data and layout tags must not be copied),
/// plus the ICC profile (tag 34675) and XMP packet (tag 700).
pub(crate) fn tiff_file_metadata(tiff: &[u8]) -> TiffMeta {
    let Some(parsed) = parse(tiff) else {
        return (None, None, None);
    };
    let bytes_of = |n: u16| -> Option<Vec<u8>> {
        match &parsed.get_field(Tag(Context::Tiff, n), In::PRIMARY)?.value {
            Value::Undefined(v, _) | Value::Byte(v) => Some(v.clone()),
            Value::Ascii(v) => Some(v.concat()),
            _ => None,
        }
        .filter(|v| !v.is_empty())
    };
    let icc = bytes_of(0x8773);
    let xmp = bytes_of(0x02BC);
    let fields: Vec<Field> = parsed
        .fields()
        .filter(|f| f.ifd_num == In::PRIMARY && writable(&f.value) && !is_tiff_structure(f.tag))
        .filter(|f| f.tag.number() != 0x927C) // MakerNote: offsets would break
        .cloned()
        .collect();
    let has_content = fields
        .iter()
        .any(|f| !matches!(classify(f.tag), Group::Structural | Group::Drop));
    let exif = if has_content {
        write_fields(&fields, parsed.little_endian())
    } else {
        None
    };
    (exif, icc, xmp)
}

/// "YYYY:MM:DD HH:MM:SS" -> "YYYY-MM-DDTHH:MM:SS".
pub(crate) fn exif_datetime_to_iso(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw)
        .ok()?
        .trim_matches(|c: char| c == '\0' || c.is_whitespace());
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !(digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19))
    {
        return None;
    }
    if &s[0..4] == "0000" {
        return None;
    }
    Some(format!(
        "{}-{}-{}T{}:{}:{}",
        &s[0..4],
        &s[5..7],
        &s[8..10],
        &s[11..13],
        &s[14..16],
        &s[17..19]
    ))
}

pub(crate) fn ascii(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let f = exif.get_field(tag, In::PRIMARY)?;
    match &f.value {
        Value::Ascii(v) => {
            let s: Vec<String> = v
                .iter()
                .map(|x| {
                    String::from_utf8_lossy(x)
                        .trim()
                        .trim_matches('\0')
                        .to_string()
                })
                .filter(|x| !x.is_empty())
                .collect();
            (!s.is_empty()).then(|| s.join(", "))
        }
        _ => {
            let s = f.display_value().to_string();
            (!s.is_empty()).then_some(s)
        }
    }
}

pub(crate) fn raw_ascii(exif: &exif::Exif, tag: Tag) -> Option<Vec<u8>> {
    match &exif.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(v) => v.first().cloned(),
        _ => None,
    }
}

pub(crate) fn rational(exif: &exif::Exif, tag: Tag) -> Option<f64> {
    let f = exif.get_field(tag, In::PRIMARY)?;
    let v = match &f.value {
        Value::Rational(v) => v.first().map(|r| r.to_f64()),
        Value::SRational(v) => v.first().map(|r| r.to_f64()),
        Value::Short(v) => v.first().map(|&x| x as f64),
        Value::Long(v) => v.first().map(|&x| x as f64),
        _ => None,
    }?;
    v.is_finite().then_some(v)
}

pub(crate) fn uint(exif: &exif::Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

/// GPS coordinate in signed decimal degrees.
pub(crate) fn gps_coord(exif: &exif::Exif, value: Tag, reference: Tag) -> Option<f64> {
    let f = exif.get_field(value, In::PRIMARY)?;
    let Value::Rational(v) = &f.value else {
        return None;
    };
    if v.is_empty() {
        return None;
    }
    let part = |i: usize| {
        v.get(i)
            .map(|r| r.to_f64())
            .filter(|x| x.is_finite())
            .unwrap_or(0.0)
    };
    let deg = part(0) + part(1) / 60.0 + part(2) / 3600.0;
    if !deg.is_finite() {
        return None;
    }
    let neg = raw_ascii(exif, reference)
        .map(|r| matches!(r.first(), Some(b'S') | Some(b'W') | Some(b's') | Some(b'w')))
        .unwrap_or(false);
    Some(if neg { -deg } else { deg })
}

/// `tiff:Orientation` in XMP -> 1 (the pixels have been rotated upright).
pub(crate) fn xmp_reset_orientation(xmp: &[u8]) -> Vec<u8> {
    let mut out = xmp.to_vec();
    for pat in [&b"tiff:Orientation=\""[..], &b"tiff:Orientation>"[..]] {
        let mut start = 0;
        while let Some(p) = find(&out[start..], pat) {
            let vpos = start + p + pat.len();
            let mut end = vpos;
            while end < out.len() && out[end].is_ascii_digit() {
                end += 1;
            }
            if end > vpos {
                out.splice(vpos..end, std::iter::once(b'1'));
            }
            start = vpos;
        }
    }
    out
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}
