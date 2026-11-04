//! PNG chunk parser/writer for metadata (eXIf, iCCP, iTXt XMP, pHYs).

use std::io::{Read, Write};

use crate::error::{Error, Result};

pub(crate) const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";
/// Cap for decompressed metadata (ICC/XMP) to avoid zip bombs.
const MAX_INFLATE: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct Chunk<'a> {
    pub kind: [u8; 4],
    pub data: &'a [u8],
}

pub(crate) fn parse(buf: &[u8]) -> Result<Vec<Chunk<'_>>> {
    if !buf.starts_with(SIGNATURE) {
        return Err(Error::decode("PNG", "missing PNG signature"));
    }
    let mut pos = 8;
    let mut chunks = Vec::new();
    while pos + 8 <= buf.len() {
        let len = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]) as usize;
        let kind = [buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]];
        let start = pos + 8;
        let end = start
            .checked_add(len)
            .filter(|&e| e + 4 <= buf.len())
            .ok_or_else(|| Error::decode("PNG", "file is truncated"))?;
        chunks.push(Chunk {
            kind,
            data: &buf[start..end],
        });
        pos = end + 4;
        if &kind == b"IEND" {
            break;
        }
    }
    if chunks.first().map(|c| &c.kind) != Some(b"IHDR") {
        return Err(Error::decode("PNG", "missing IHDR chunk"));
    }
    Ok(chunks)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PngInfo {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

pub(crate) fn info(chunks: &[Chunk<'_>]) -> PngInfo {
    let mut info = PngInfo::default();
    for c in chunks {
        match &c.kind {
            b"IHDR" if c.data.len() >= 13 => {
                info.width = u32::from_be_bytes([c.data[0], c.data[1], c.data[2], c.data[3]]);
                info.height = u32::from_be_bytes([c.data[4], c.data[5], c.data[6], c.data[7]]);
                info.has_alpha = matches!(c.data[9], 4 | 6);
            }
            b"tRNS" => info.has_alpha = true,
            b"eXIf" if info.exif.is_none() => {
                let mut d = c.data;
                if d.starts_with(b"Exif\0\0") {
                    d = &d[6..];
                }
                if d.len() >= 8 {
                    info.exif = Some(d.to_vec());
                }
            }
            b"iCCP" if info.icc.is_none() => {
                if let Some(nul) = c.data.iter().position(|&b| b == 0) {
                    if let Some(z) = c.data.get(nul + 2..) {
                        info.icc = inflate(z).ok();
                    }
                }
            }
            b"iTXt" if info.xmp.is_none() => {
                if let Some(x) = parse_itxt(c.data, XMP_KEYWORD) {
                    info.xmp = Some(x);
                }
            }
            b"zTXt" | b"tEXt" | b"iTXt" if info.exif.is_none() => {
                if let Some(x) = parse_raw_profile_exif(&c.kind, c.data) {
                    info.exif = Some(x);
                }
            }
            _ => {}
        }
    }
    info
}

fn inflate(z: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(z)
        .take(MAX_INFLATE)
        .read_to_end(&mut out)
        .map_err(|e| Error::Metadata(format!("corrupt compressed PNG chunk: {e}")))?;
    Ok(out)
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing to a Vec cannot fail.
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn parse_itxt(data: &[u8], want_keyword: &[u8]) -> Option<Vec<u8>> {
    let nul = data.iter().position(|&b| b == 0)?;
    if &data[..nul] != want_keyword {
        return None;
    }
    let compressed = *data.get(nul + 1)? != 0;
    let rest = data.get(nul + 3..)?;
    let lang_end = rest.iter().position(|&b| b == 0)?;
    let rest = &rest[lang_end + 1..];
    let tk_end = rest.iter().position(|&b| b == 0)?;
    let text = &rest[tk_end + 1..];
    if compressed {
        inflate(text).ok()
    } else {
        Some(text.to_vec())
    }
}

/// ImageMagick-style "Raw profile type exif" text chunk (hex encoded).
fn parse_raw_profile_exif(kind: &[u8; 4], data: &[u8]) -> Option<Vec<u8>> {
    let nul = data.iter().position(|&b| b == 0)?;
    let keyword = &data[..nul];
    if keyword != b"Raw profile type exif" && keyword != b"Raw profile type APP1" {
        return None;
    }
    let text: Vec<u8> = match kind {
        b"tEXt" => data[nul + 1..].to_vec(),
        b"zTXt" => inflate(data.get(nul + 2..)?).ok()?,
        _ => return None,
    };
    // Format: "\nexif\n   <len>\n<hex...>"
    let s = std::str::from_utf8(&text).ok()?;
    let mut parts = s.split_whitespace();
    let _name = parts.next()?;
    let len: usize = parts.next()?.parse().ok()?;
    let hex: String = parts.collect();
    if hex.len() < len * 2 {
        return None;
    }
    let mut bytes = Vec::with_capacity(len);
    for i in 0..len {
        bytes.push(u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?);
    }
    let start = if bytes.starts_with(b"Exif\0\0") { 6 } else { 0 };
    let tiff = &bytes[start..];
    (tiff.starts_with(b"II") || tiff.starts_with(b"MM")).then(|| tiff.to_vec())
}

fn is_text_chunk(c: &Chunk<'_>) -> bool {
    matches!(&c.kind, b"tEXt" | b"zTXt" | b"iTXt")
}

fn is_xmp_chunk(c: &Chunk<'_>) -> bool {
    &c.kind == b"iTXt"
        && c.data.starts_with(XMP_KEYWORD)
        && c.data.get(XMP_KEYWORD.len()) == Some(&0)
}

fn is_raw_profile(c: &Chunk<'_>) -> bool {
    is_text_chunk(c) && c.data.starts_with(b"Raw profile type ")
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(data);
    out.extend_from_slice(&h.finalize().to_be_bytes());
}

/// Options for [`rewrite`].
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Rewrite<'a> {
    pub exif: Option<&'a [u8]>,
    pub icc: Option<&'a [u8]>,
    pub xmp: Option<&'a [u8]>,
    /// Keep text chunks (other than XMP / raw profiles) and tIME.
    pub keep_other: bool,
    /// Replace pHYs with this density (dots per inch).
    pub dpi: Option<u32>,
}

/// Rebuild a PNG with the given metadata. Image chunks are copied verbatim.
pub(crate) fn rewrite(buf: &[u8], opts: &Rewrite<'_>) -> Result<Vec<u8>> {
    let chunks = parse(buf)?;
    let mut out = Vec::with_capacity(buf.len() + 4096);
    out.extend_from_slice(SIGNATURE);
    for (i, c) in chunks.iter().enumerate() {
        let drop = match &c.kind {
            b"eXIf" | b"iCCP" => true,
            b"sRGB" => opts.icc.is_some(),
            b"pHYs" => opts.dpi.is_some(),
            b"tIME" => !opts.keep_other,
            _ if is_xmp_chunk(c) || is_raw_profile(c) => true,
            _ if is_text_chunk(c) => !opts.keep_other,
            _ => false,
        };
        if !drop {
            write_chunk(&mut out, &c.kind, c.data);
        }
        if i == 0 {
            // Right after IHDR: colour info and metadata must precede PLTE/IDAT.
            if let Some(icc) = opts.icc {
                let mut d = b"ICC Profile\0\0".to_vec();
                d.extend_from_slice(&deflate(icc));
                write_chunk(&mut out, b"iCCP", &d);
            }
            if let Some(dpi) = opts.dpi {
                let ppm = ((dpi as f64) / 0.0254).round() as u32;
                let mut d = Vec::with_capacity(9);
                d.extend_from_slice(&ppm.to_be_bytes());
                d.extend_from_slice(&ppm.to_be_bytes());
                d.push(1);
                write_chunk(&mut out, b"pHYs", &d);
            }
            if let Some(exif) = opts.exif {
                write_chunk(&mut out, b"eXIf", exif);
            }
            if let Some(xmp) = opts.xmp {
                let mut d = XMP_KEYWORD.to_vec();
                d.extend_from_slice(&[0, 0, 0, 0, 0]);
                d.extend_from_slice(xmp);
                write_chunk(&mut out, b"iTXt", &d);
            }
        }
    }
    if chunks.last().map(|c| &c.kind) != Some(b"IEND") {
        write_chunk(&mut out, b"IEND", &[]);
    }
    Ok(out)
}
