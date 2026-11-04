//! WebP RIFF container parser/writer for metadata (VP8X, ICCP, EXIF, XMP).

use crate::error::{Error, Result};

#[derive(Clone, Debug)]
pub(crate) struct Chunk<'a> {
    pub fourcc: [u8; 4],
    pub data: &'a [u8],
}

pub(crate) fn parse(buf: &[u8]) -> Result<Vec<Chunk<'_>>> {
    if buf.len() < 12 || &buf[0..4] != b"RIFF" || &buf[8..12] != b"WEBP" {
        return Err(Error::decode("WebP", "missing RIFF/WEBP header"));
    }
    let riff_len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    let end = (riff_len + 8).min(buf.len());
    let mut pos = 12;
    let mut chunks = Vec::new();
    while pos + 8 <= end {
        let fourcc = [buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]];
        let len =
            u32::from_le_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]) as usize;
        let start = pos + 8;
        let stop = start
            .checked_add(len)
            .filter(|&s| s <= end)
            .ok_or_else(|| Error::decode("WebP", "file is truncated"))?;
        chunks.push(Chunk {
            fourcc,
            data: &buf[start..stop],
        });
        pos = stop + (len & 1);
    }
    if chunks.is_empty() {
        return Err(Error::decode("WebP", "no chunks"));
    }
    Ok(chunks)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct WebpInfo {
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub animated: bool,
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

fn vp8_dims(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 10 || d[3..6] != [0x9D, 0x01, 0x2A] {
        return None;
    }
    let w = u16::from_le_bytes([d[6], d[7]]) & 0x3FFF;
    let h = u16::from_le_bytes([d[8], d[9]]) & 0x3FFF;
    Some((w as u32, h as u32))
}

/// (width, height, alpha_is_used)
fn vp8l_header(d: &[u8]) -> Option<(u32, u32, bool)> {
    if d.len() < 5 || d[0] != 0x2F {
        return None;
    }
    let bits = u32::from_le_bytes([d[1], d[2], d[3], d[4]]);
    let w = (bits & 0x3FFF) + 1;
    let h = ((bits >> 14) & 0x3FFF) + 1;
    let alpha = (bits >> 28) & 1 == 1;
    Some((w, h, alpha))
}

pub(crate) fn info(chunks: &[Chunk<'_>]) -> WebpInfo {
    let mut info = WebpInfo::default();
    for c in chunks {
        match &c.fourcc {
            b"VP8X" if c.data.len() >= 10 => {
                let flags = c.data[0];
                info.has_alpha |= flags & 0x10 != 0;
                info.animated = flags & 0x02 != 0;
                let d = c.data;
                info.width = u32::from_le_bytes([d[4], d[5], d[6], 0]) + 1;
                info.height = u32::from_le_bytes([d[7], d[8], d[9], 0]) + 1;
            }
            b"VP8 " if info.width == 0 => {
                if let Some((w, h)) = vp8_dims(c.data) {
                    info.width = w;
                    info.height = h;
                }
            }
            b"VP8L" => {
                if let Some((w, h, a)) = vp8l_header(c.data) {
                    if info.width == 0 {
                        info.width = w;
                        info.height = h;
                    }
                    info.has_alpha |= a;
                }
            }
            b"ALPH" => info.has_alpha = true,
            b"EXIF" if info.exif.is_none() => {
                let mut d = c.data;
                if d.starts_with(b"Exif\0\0") {
                    d = &d[6..];
                }
                if d.len() >= 8 {
                    info.exif = Some(d.to_vec());
                }
            }
            b"ICCP" if info.icc.is_none() => info.icc = Some(c.data.to_vec()),
            b"XMP " if info.xmp.is_none() => info.xmp = Some(c.data.to_vec()),
            _ => {}
        }
    }
    info
}

fn push_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() & 1 == 1 {
        out.push(0);
    }
}

/// Rebuild a WebP file with the given metadata. Produces a valid extended
/// (VP8X) file when any metadata is present, and a simple file otherwise
/// (unless the source needs VP8X for animation/alpha).
pub(crate) fn rewrite(
    buf: &[u8],
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let chunks = parse(buf)?;
    let inf = info(&chunks);
    if inf.width == 0 || inf.height == 0 {
        return Err(Error::Metadata(
            "could not determine WebP canvas size".into(),
        ));
    }
    let body: Vec<&Chunk> = chunks
        .iter()
        .filter(|c| !matches!(&c.fourcc, b"VP8X" | b"ICCP" | b"EXIF" | b"XMP "))
        .collect();
    let has_alph_chunk = body.iter().any(|c| &c.fourcc == b"ALPH");
    let has_anim = body.iter().any(|c| matches!(&c.fourcc, b"ANIM" | b"ANMF"));
    let unknown = body.iter().any(|c| !matches!(&c.fourcc, b"VP8 " | b"VP8L"));
    let need_vp8x = exif.is_some() || icc.is_some() || xmp.is_some() || has_anim || unknown;

    let mut out = Vec::with_capacity(buf.len() + 1024);
    out.extend_from_slice(b"RIFF\0\0\0\0WEBP");
    if need_vp8x {
        let mut flags = 0u8;
        if icc.is_some() {
            flags |= 0x20;
        }
        if inf.has_alpha || has_alph_chunk {
            flags |= 0x10;
        }
        if exif.is_some() {
            flags |= 0x08;
        }
        if xmp.is_some() {
            flags |= 0x04;
        }
        if has_anim {
            flags |= 0x02;
        }
        let w = (inf.width - 1).to_le_bytes();
        let h = (inf.height - 1).to_le_bytes();
        let vp8x = [flags, 0, 0, 0, w[0], w[1], w[2], h[0], h[1], h[2]];
        push_chunk(&mut out, b"VP8X", &vp8x);
        if let Some(icc) = icc {
            push_chunk(&mut out, b"ICCP", icc);
        }
    }
    for c in &body {
        push_chunk(&mut out, &c.fourcc, c.data);
    }
    if let Some(exif) = exif {
        push_chunk(&mut out, b"EXIF", exif);
    }
    if let Some(xmp) = xmp {
        push_chunk(&mut out, b"XMP ", xmp);
    }
    let riff_len = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&riff_len.to_le_bytes());
    Ok(out)
}
