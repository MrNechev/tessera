//! TIFF writer (Deflate + horizontal predictor) built on kamadak-exif's
//! TIFF writer, which lets us emit proper Exif/GPS sub-IFDs alongside the
//! image strips.

use std::io::Write;

use exif::{Context, Field, In, Rational, Tag, Value};
use image::DynamicImage;
use rayon::prelude::*;

use crate::error::{Error, Result};
use crate::exif_util;

const STRIP_TARGET_BYTES: usize = 256 * 1024;

fn tiff_tag(n: u16) -> Tag {
    Tag(Context::Tiff, n)
}

fn field(n: u16, value: Value) -> Field {
    Field {
        tag: tiff_tag(n),
        ifd_num: In::PRIMARY,
        value,
    }
}

/// Pixel layout ready for writing.
struct Raster {
    width: u32,
    height: u32,
    channels: usize,
    bits: usize,
    /// Little-endian samples.
    data: Vec<u8>,
}

fn raster(img: &DynamicImage) -> Raster {
    let (w, h) = (img.width(), img.height());
    let (channels, bits, data): (usize, usize, Vec<u8>) = match img {
        DynamicImage::ImageLuma8(i) => (1, 8, i.as_raw().clone()),
        DynamicImage::ImageLumaA8(i) => (2, 8, i.as_raw().clone()),
        DynamicImage::ImageRgb8(i) => (3, 8, i.as_raw().clone()),
        DynamicImage::ImageRgba8(i) => (4, 8, i.as_raw().clone()),
        DynamicImage::ImageLuma16(i) => (1, 16, le16(i.as_raw())),
        DynamicImage::ImageLumaA16(i) => (2, 16, le16(i.as_raw())),
        DynamicImage::ImageRgb16(i) => (3, 16, le16(i.as_raw())),
        DynamicImage::ImageRgba16(i) => (4, 16, le16(i.as_raw())),
        other if other.color().has_alpha() => (4, 16, le16(other.to_rgba16().as_raw())),
        other => (3, 16, le16(other.to_rgb16().as_raw())),
    };
    Raster {
        width: w,
        height: h,
        channels,
        bits,
        data,
    }
}

fn le16(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|s| s.to_le_bytes()).collect()
}

/// Apply TIFF predictor 2 (horizontal differencing) to one row in place.
fn predict_row(row: &mut [u8], channels: usize, bits: usize) {
    if bits == 8 {
        for i in (channels..row.len()).rev() {
            row[i] = row[i].wrapping_sub(row[i - channels]);
        }
    } else {
        let n = row.len() / 2;
        let get = |r: &[u8], i: usize| u16::from_le_bytes([r[i * 2], r[i * 2 + 1]]);
        for i in (channels..n).rev() {
            let v = get(row, i).wrapping_sub(get(row, i - channels));
            row[i * 2..i * 2 + 2].copy_from_slice(&v.to_le_bytes());
        }
    }
}

fn level(effort: u8) -> flate2::Compression {
    match effort {
        0..=29 => flate2::Compression::new(1),
        30..=69 => flate2::Compression::new(6),
        _ => flate2::Compression::new(9),
    }
}

/// Encode as a single-page little-endian TIFF with Deflate compression.
pub(crate) fn encode(img: &DynamicImage, effort: u8, dpi: Option<u32>) -> Result<Vec<u8>> {
    let r = raster(img);
    if r.width == 0 || r.height == 0 {
        return Err(Error::encode("TIFF", "image is empty"));
    }
    let row_bytes = r.width as usize * r.channels * r.bits / 8;
    let rows_per_strip = (STRIP_TARGET_BYTES / row_bytes.max(1)).clamp(1, r.height as usize);
    let lvl = level(effort);
    let strips: Vec<Vec<u8>> = r
        .data
        .par_chunks(row_bytes * rows_per_strip)
        .map(|chunk| {
            let mut buf = chunk.to_vec();
            for row in buf.chunks_mut(row_bytes) {
                predict_row(row, r.channels, r.bits);
            }
            let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), lvl);
            // Writing into a Vec cannot fail.
            let _ = enc.write_all(&buf);
            enc.finish().unwrap_or_default()
        })
        .collect();
    let mut fields = vec![
        field(0x0100, Value::Long(vec![r.width])),
        field(0x0101, Value::Long(vec![r.height])),
        field(0x0102, Value::Short(vec![r.bits as u16; r.channels])),
        field(0x0103, Value::Short(vec![8])),
        field(
            0x0106,
            Value::Short(vec![if r.channels <= 2 { 1 } else { 2 }]),
        ),
        field(0x0115, Value::Short(vec![r.channels as u16])),
        field(0x0116, Value::Long(vec![rows_per_strip as u32])),
        field(0x011C, Value::Short(vec![1])),
        field(0x013D, Value::Short(vec![2])),
    ];
    if r.channels == 2 || r.channels == 4 {
        // Unassociated alpha.
        fields.push(field(0x0152, Value::Short(vec![2])));
    }
    let d = dpi.unwrap_or(72).max(1);
    fields.push(field(
        0x011A,
        Value::Rational(vec![Rational { num: d, denom: 1 }]),
    ));
    fields.push(field(
        0x011B,
        Value::Rational(vec![Rational { num: d, denom: 1 }]),
    ));
    fields.push(field(0x0128, Value::Short(vec![2])));
    write(&fields, &strips)
}

fn write(fields: &[Field], strips: &[Vec<u8>]) -> Result<Vec<u8>> {
    let strip_refs: Vec<&[u8]> = strips.iter().map(|s| s.as_slice()).collect();
    let mut w = exif::experimental::Writer::new();
    for f in fields {
        w.push_field(f);
    }
    w.set_strips(&strip_refs, In::PRIMARY);
    let mut cur = std::io::Cursor::new(Vec::new());
    w.write(&mut cur, true)
        .map_err(|e| Error::encode("TIFF", e))?;
    Ok(cur.into_inner())
}

/// Tags describing the raster layout that must be carried over when
/// rewriting a TIFF (strip/tile offsets are synthesised by the writer).
fn is_layout(n: u16) -> bool {
    matches!(
        n,
        0x00FE | 0x0100..=0x0103 | 0x0106 | 0x0115 | 0x0116 | 0x011C | 0x013D | 0x0142 | 0x0143
            | 0x0152..=0x0155
    )
}

/// Rewrite a (single page, strip-based) TIFF with new metadata.
pub(crate) fn rewrite(
    tiff: &[u8],
    exif_block: Option<&[u8]>,
    icc: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let parsed = exif_util::parse(tiff)
        .ok_or_else(|| Error::Metadata("could not parse TIFF structure".into()))?;
    let get_uints = |n: u16| -> Option<Vec<u32>> {
        let f = parsed.get_field(tiff_tag(n), In::PRIMARY)?;
        Some(f.value.iter_uint()?.collect())
    };
    let offsets = get_uints(0x0111)
        .ok_or_else(|| Error::Metadata("TIFF without strips is not supported".into()))?;
    let counts = get_uints(0x0117)
        .ok_or_else(|| Error::Metadata("TIFF without strip byte counts".into()))?;
    if offsets.len() != counts.len() {
        return Err(Error::Metadata("inconsistent TIFF strips".into()));
    }
    let mut strips = Vec::with_capacity(offsets.len());
    for (&o, &c) in offsets.iter().zip(&counts) {
        let s = tiff
            .get(o as usize..o as usize + c as usize)
            .ok_or_else(|| Error::Metadata("TIFF strip out of bounds".into()))?;
        strips.push(s.to_vec());
    }
    let mut fields: Vec<Field> = parsed
        .fields()
        .filter(|f| {
            f.ifd_num == In::PRIMARY
                && f.tag.context() == Context::Tiff
                && (is_layout(f.tag.number()) || matches!(f.tag.number(), 0x011A | 0x011B | 0x0128))
        })
        .cloned()
        .collect();
    let have = |fields: &[Field], tag: Tag| fields.iter().any(|f| f.tag == tag);
    if let Some(block) = exif_block {
        if let Some(e) = exif_util::parse(block) {
            for f in e.fields() {
                if f.ifd_num != In::PRIMARY || matches!(f.value, Value::Unknown(..)) {
                    continue;
                }
                if f.tag.context() == Context::Tiff
                    && (is_layout(f.tag.number())
                        || matches!(f.tag.number(), 0x0111 | 0x0117 | 0x02BC | 0x8773))
                {
                    continue;
                }
                if have(&fields, f.tag) {
                    continue;
                }
                fields.push(f.clone());
            }
        }
    }
    if let Some(icc) = icc {
        fields.push(field(0x8773, Value::Undefined(icc.to_vec(), 0)));
    }
    if let Some(xmp) = xmp {
        fields.push(field(0x02BC, Value::Byte(xmp.to_vec())));
    }
    write(&fields, &strips)
}
