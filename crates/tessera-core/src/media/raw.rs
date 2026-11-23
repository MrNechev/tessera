//! Camera RAW files carry a full-size (or large) JPEG preview made by the camera. Tessera
//! shows that preview instead of developing the RAW: it is what the camera displayed and it
//! costs nothing to extract. Works for TIFF-based RAWs (CR2, NEF, ARW, DNG, ORF, RW2, PEF,
//! SRW) and for CR3/RAF, which store the JPEG unmodified inside their own containers.

use std::io::Read;
use std::path::Path;

use anyhow::bail;

use super::exif::{jpeg_exif, parse_tiff, Meta};

const MAX_RAW_READ: u64 = 512 << 20;

/// The largest embedded JPEG of a RAW file and the RAW's own metadata.
pub fn raw_preview(path: &Path) -> anyhow::Result<(Vec<u8>, Meta)> {
    let mut b = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_RAW_READ)
        .read_to_end(&mut b)?;
    let mut meta = parse_tiff(&b);
    if meta.camera.is_empty() && b.len() > 16 && &b[..16] == b"FUJIFILMCCD-RAW " {
        meta.camera = "Fujifilm".into();
    }
    let Some((start, end)) = largest_jpeg(&b) else {
        bail!("no embedded preview");
    };
    let jpg = b[start..end].to_vec();
    // the preview's own EXIF (if any) can fill in what the RAW header lacks
    let inner = jpeg_exif(&jpg[..]).map(|t| parse_tiff(&t)).unwrap_or_default();
    if !inner.camera.is_empty() || inner.taken.is_some() {
        if meta.camera.is_empty() {
            meta.camera = inner.camera.clone();
        }
        if meta.taken.is_none() {
            meta.taken = inner.taken;
        }
        if !meta.has_gps && inner.has_gps {
            meta.lat = inner.lat;
            meta.lon = inner.lon;
            meta.has_gps = true;
        }
        if meta.orientation == 0 {
            meta.orientation = inner.orientation;
        }
    }
    Ok((jpg, meta))
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Finds complete baseline/progressive JPEG streams (SOI … EOI that decode a header) and
/// returns the byte range of the one with the most pixels.
pub fn largest_jpeg(b: &[u8]) -> Option<(usize, usize)> {
    let mut best = None;
    let mut best_px = 0i64;
    let mut i = 0;
    while i + 4 < b.len() {
        let Some(j) = find(&b[i..], &[0xFF, 0xD8, 0xFF]) else {
            break;
        };
        let start = i + j;
        let cfg = jpeg_config(&b[start..]);
        let Ok((w, h)) = cfg else {
            i = start + 3;
            continue;
        };
        if w < 160 {
            i = start + 3;
            continue;
        }
        let end = jpeg_end(&b[start..]);
        if end <= 0 {
            i = start + 3;
            continue;
        }
        let px = w * h;
        if px > best_px {
            best_px = px;
            best = Some((start, start + end as usize));
        }
        i = start + end as usize;
    }
    best
}

/// Reads a JPEG's dimensions like Go's `jpeg.DecodeConfig`: only baseline, extended and
/// progressive Huffman JPEGs (SOF0–SOF2) are accepted.
pub fn jpeg_config(b: &[u8]) -> anyhow::Result<(i64, i64)> {
    let mut p = 0usize;
    let byte = |p: &mut usize| -> anyhow::Result<u8> {
        let v = *b.get(*p).ok_or_else(|| anyhow::anyhow!("unexpected EOF"))?;
        *p += 1;
        Ok(v)
    };
    if byte(&mut p)? != 0xFF || byte(&mut p)? != 0xD8 {
        bail!("missing SOI marker");
    }
    let mut dims: Option<(i64, i64)> = None;
    let mut jfif = false;
    loop {
        let mut t0 = byte(&mut p)?;
        let mut t1 = byte(&mut p)?;
        while t0 != 0xFF {
            t0 = t1;
            t1 = byte(&mut p)?;
        }
        let mut marker = t1;
        if marker == 0 {
            continue;
        }
        while marker == 0xFF {
            marker = byte(&mut p)?;
        }
        if marker == 0xD9 {
            break;
        }
        if (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        let hi = byte(&mut p)? as i64;
        let lo = byte(&mut p)? as i64;
        let n = (hi << 8) + lo - 2;
        if n < 0 {
            bail!("short segment length");
        }
        let n = n as usize;
        let seg = b
            .get(p..p + n)
            .ok_or_else(|| anyhow::anyhow!("unexpected EOF"))?;
        match marker {
            0xC0..=0xC2 => {
                if n < 6 {
                    bail!("bad SOF length");
                }
                let ncomp = seg[5] as usize;
                if !matches!(ncomp, 1 | 3 | 4) {
                    bail!("unsupported number of components");
                }
                if n != 6 + 3 * ncomp {
                    bail!("incorrect number of components");
                }
                let h = u16::from_be_bytes([seg[1], seg[2]]) as i64;
                let w = u16::from_be_bytes([seg[3], seg[4]]) as i64;
                dims = Some((w, h));
                if jfif {
                    return Ok((w, h));
                }
            }
            0xDA => {
                return dims.ok_or_else(|| anyhow::anyhow!("missing SOF marker"));
            }
            0xE0 => {
                if n >= 5 && &seg[..5] == b"JFIF\x00" {
                    jfif = true;
                }
            }
            0xC4 | 0xDB | 0xDD | 0xEE => {}
            m if (0xE0..=0xEF).contains(&m) || m == 0xFE => {}
            m if m < 0xC0 => bail!("unknown marker"),
            _ => bail!("unsupported JPEG feature: unknown marker"),
        }
        p += n;
    }
    dims.ok_or_else(|| anyhow::anyhow!("missing SOF marker"))
}

/// Walks the JPEG markers to the end of the image (EOI) and returns its length (or -1).
pub fn jpeg_end(b: &[u8]) -> i64 {
    let mut i = 2usize;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            return -1;
        }
        let marker = b[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xD9 {
            return (i + 2) as i64;
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let n = ((b[i + 2] as usize) << 8) | b[i + 3] as usize;
        i += 2 + n;
        if marker == 0xDA {
            // entropy-coded data: skip to the next marker that isn't a stuffed 0xFF00 or RSTn
            while i + 1 < b.len() {
                if b[i] == 0xFF && b[i + 1] != 0x00 && !(0xD0..=0xD7).contains(&b[i + 1]) {
                    break;
                }
                i += 1;
            }
        }
    }
    -1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::exif::tests::build_tiff;

    fn enc(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut out)
            .encode_image(&img)
            .unwrap();
        out
    }

    #[test]
    fn jpeg_exif_and_raw_preview() {
        let small = enc(200, 100);
        let big = enc(640, 480);
        // fake RAW: TIFF header with metadata, junk, a small and a big JPEG
        let mut raw = build_tiff();
        raw.extend(std::iter::repeat_n(0x11u8, 300));
        raw.extend_from_slice(&small);
        for _ in 0..50 {
            raw.extend_from_slice(&[0xFF, 0x00]);
        }
        raw.extend_from_slice(&big);
        raw.extend_from_slice(&[0, 1, 2]);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.nef");
        std::fs::write(&p, &raw).unwrap();
        let (jpg, meta) = raw_preview(&p).unwrap();
        assert!(jpg == big, "preview: got {} bytes, want the 640×480 one ({})", jpg.len(), big.len());
        assert_eq!(meta.camera, "Apple iPhone 15 Pro");
        let pr = crate::media::probe::probe_raw(&p);
        assert_eq!((pr.width, pr.height, pr.rotation), (480, 640, 90), "{pr:?}");
        assert!(pr.taken != 0 && pr.has_gps, "{pr:?}");
    }
}
