//! Image format identification and capability table.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Image formats known to Tessera.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Jpeg,
    Png,
    Webp,
    Avif,
    Tiff,
    Bmp,
    Gif,
    Ico,
    Qoi,
    Tga,
    Heic,
    Jxl,
    /// Camera RAW (decode only, developed to sRGB through `rawler`).
    Raw,
}

/// Camera RAW extensions (lowercase). A superset of the extensions `rawler`
/// lists in `rawler::decoders::supported_extensions()` (checked by a unit
/// test) plus Sony/Kodak/Leica aliases handled by the same decoders.
pub const RAW_EXTENSIONS: &[&str] = &[
    "dng", "cr2", "cr3", "crm", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "orf", "ori",
    "rw2", "rwl", "raw", "pef", "srw", "erf", "mef", "mos", "mrw", "kdc", "dcr", "dcs", "3fr",
    "fff", "iiq", "x3f", "ari", "qtk",
];

/// Every variant, in UI order.
pub const ALL_FORMATS: [ImageFormat; 13] = [
    ImageFormat::Jpeg,
    ImageFormat::Png,
    ImageFormat::Webp,
    ImageFormat::Avif,
    ImageFormat::Heic,
    ImageFormat::Jxl,
    ImageFormat::Raw,
    ImageFormat::Tiff,
    ImageFormat::Gif,
    ImageFormat::Bmp,
    ImageFormat::Ico,
    ImageFormat::Qoi,
    ImageFormat::Tga,
];

impl ImageFormat {
    /// Map a file extension (without dot, any case) to a format.
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        Some(match ext.as_str() {
            "jpg" | "jpeg" | "jpe" | "jfif" | "jif" => ImageFormat::Jpeg,
            "png" | "apng" => ImageFormat::Png,
            "webp" => ImageFormat::Webp,
            "avif" => ImageFormat::Avif,
            "tif" | "tiff" => ImageFormat::Tiff,
            "bmp" | "dib" => ImageFormat::Bmp,
            "gif" => ImageFormat::Gif,
            "ico" => ImageFormat::Ico,
            "qoi" => ImageFormat::Qoi,
            "tga" | "icb" | "vda" | "vst" => ImageFormat::Tga,
            "heic" | "heif" | "hif" => ImageFormat::Heic,
            "jxl" => ImageFormat::Jxl,
            e if RAW_EXTENSIONS.contains(&e) => ImageFormat::Raw,
            _ => return None,
        })
    }

    /// Format implied by a path's extension.
    pub fn from_path(p: &Path) -> Option<Self> {
        p.extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }

    /// Identify a format from the first bytes of a file (magic numbers).
    /// Prefer this over [`ImageFormat::from_path`]. TGA has no signature and
    /// is never detected here.
    ///
    /// Camera RAW is recognised for DNG (TIFF with a DNGVersion tag, when IFD0
    /// lies inside `bytes`), CR2, CR3 (ISO-BMFF `crx `), CRW, RAF, ORF, RW2,
    /// MRW and X3F. Other TIFF-based RAWs (NEF, ARW, PEF, ...) are reported
    /// as TIFF here; [`ImageFormat::detect_path`] resolves them by extension.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        let b = bytes;
        if is_raw_magic(b) {
            return Some(ImageFormat::Raw);
        }
        if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Some(ImageFormat::Jpeg);
        }
        if b.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Some(ImageFormat::Png);
        }
        if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
            return Some(ImageFormat::Webp);
        }
        if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
            return Some(ImageFormat::Gif);
        }
        if b.starts_with(b"II*\0") || b.starts_with(b"MM\0*") {
            return Some(ImageFormat::Tiff);
        }
        // BigTIFF
        if b.starts_with(b"II+\0") || b.starts_with(b"MM\0+") {
            return Some(ImageFormat::Tiff);
        }
        if b.starts_with(b"qoif") {
            return Some(ImageFormat::Qoi);
        }
        if b.starts_with(&[0xFF, 0x0A])
            || b.starts_with(&[
                0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A,
            ])
        {
            return Some(ImageFormat::Jxl);
        }
        if let Some(f) = detect_isobmff(b) {
            return Some(f);
        }
        if b.len() >= 14 && b.starts_with(b"BM") {
            return Some(ImageFormat::Bmp);
        }
        // ICO: reserved=0, type=1, count>0
        if b.len() >= 6 && b[0..4] == [0, 0, 1, 0] && (b[4] != 0 || b[5] != 0) {
            return Some(ImageFormat::Ico);
        }
        None
    }

    /// Content detection combined with the extension: like
    /// [`ImageFormat::detect`] on the first bytes of the file, except that a
    /// TIFF-structured file with a RAW extension (NEF, ARW, PEF, ...) is
    /// reported as [`ImageFormat::Raw`]; falls back to the extension when
    /// the content is not recognised. `None` if the file can't be read or
    /// nothing matches.
    pub fn detect_path(path: &Path) -> Option<Self> {
        use std::io::Read;
        let mut head = Vec::with_capacity(64 * 1024);
        let f = std::fs::File::open(path).ok()?;
        f.take(256 * 1024).read_to_end(&mut head).ok()?;
        resolve(&head, path)
    }

    /// Canonical file extension (no dot).
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::Webp => "webp",
            ImageFormat::Avif => "avif",
            ImageFormat::Tiff => "tif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Gif => "gif",
            ImageFormat::Ico => "ico",
            ImageFormat::Qoi => "qoi",
            ImageFormat::Tga => "tga",
            ImageFormat::Heic => "heic",
            ImageFormat::Jxl => "jxl",
            ImageFormat::Raw => "dng",
        }
    }

    /// MIME type.
    pub fn mime(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::Webp => "image/webp",
            ImageFormat::Avif => "image/avif",
            ImageFormat::Tiff => "image/tiff",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Ico => "image/x-icon",
            ImageFormat::Qoi => "image/qoi",
            ImageFormat::Tga => "image/x-tga",
            ImageFormat::Heic => "image/heic",
            ImageFormat::Jxl => "image/jxl",
            ImageFormat::Raw => "image/x-raw",
        }
    }

    /// Human readable name, e.g. "JPEG".
    pub fn label(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "JPEG",
            ImageFormat::Png => "PNG",
            ImageFormat::Webp => "WebP",
            ImageFormat::Avif => "AVIF",
            ImageFormat::Tiff => "TIFF",
            ImageFormat::Bmp => "BMP",
            ImageFormat::Gif => "GIF",
            ImageFormat::Ico => "ICO",
            ImageFormat::Qoi => "QOI",
            ImageFormat::Tga => "TGA",
            ImageFormat::Heic => "HEIC",
            ImageFormat::Jxl => "JPEG XL",
            ImageFormat::Raw => "Camera RAW",
        }
    }

    /// Whether this build can read the format.
    pub fn can_decode(self) -> bool {
        !matches!(self, ImageFormat::Heic) || cfg!(feature = "heif")
    }

    /// Whether this build can write the format.
    pub fn can_encode(self) -> bool {
        !matches!(
            self,
            ImageFormat::Heic | ImageFormat::Jxl | ImageFormat::Raw
        )
    }

    /// The quality setting has an effect when writing this format (for PNG
    /// and GIF it drives palette quantization).
    pub fn supports_lossy(self) -> bool {
        matches!(
            self,
            ImageFormat::Jpeg
                | ImageFormat::Webp
                | ImageFormat::Avif
                | ImageFormat::Png
                | ImageFormat::Gif
                | ImageFormat::Heic
                | ImageFormat::Jxl
        )
    }

    /// The format can store pixels exactly (as written by Tessera; AVIF is
    /// only near-lossless here, GIF is limited to 256 colours).
    pub fn supports_lossless(self) -> bool {
        matches!(
            self,
            ImageFormat::Png
                | ImageFormat::Webp
                | ImageFormat::Tiff
                | ImageFormat::Bmp
                | ImageFormat::Ico
                | ImageFormat::Qoi
                | ImageFormat::Tga
                | ImageFormat::Jxl
        )
    }

    /// The format can carry transparency.
    pub fn supports_alpha(self) -> bool {
        !matches!(self, ImageFormat::Jpeg)
    }

    /// EXIF is read from and (for writable formats) written to this format.
    pub fn supports_exif(self) -> bool {
        matches!(
            self,
            ImageFormat::Jpeg
                | ImageFormat::Png
                | ImageFormat::Webp
                | ImageFormat::Tiff
                | ImageFormat::Heic
                | ImageFormat::Jxl
                | ImageFormat::Raw
        )
    }

    /// ICC profiles are read from and (for writable formats) written to this
    /// format.
    pub fn supports_icc(self) -> bool {
        matches!(
            self,
            ImageFormat::Jpeg
                | ImageFormat::Png
                | ImageFormat::Webp
                | ImageFormat::Tiff
                | ImageFormat::Heic
                | ImageFormat::Jxl
        )
    }

    /// `image` crate equivalent, for formats decoded through it.
    pub(crate) fn to_image_format(self) -> Option<image::ImageFormat> {
        Some(match self {
            ImageFormat::Jpeg => image::ImageFormat::Jpeg,
            ImageFormat::Png => image::ImageFormat::Png,
            ImageFormat::Webp => image::ImageFormat::WebP,
            ImageFormat::Avif => image::ImageFormat::Avif,
            ImageFormat::Tiff => image::ImageFormat::Tiff,
            ImageFormat::Bmp => image::ImageFormat::Bmp,
            ImageFormat::Gif => image::ImageFormat::Gif,
            ImageFormat::Ico => image::ImageFormat::Ico,
            ImageFormat::Qoi => image::ImageFormat::Qoi,
            ImageFormat::Tga => image::ImageFormat::Tga,
            ImageFormat::Heic | ImageFormat::Jxl | ImageFormat::Raw => return None,
        })
    }
}

impl std::fmt::Display for ImageFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// ISO-BMFF (`ftyp`) brands for AVIF / HEIF.
fn detect_isobmff(b: &[u8]) -> Option<ImageFormat> {
    if b.len() < 16 || &b[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    let end = size.clamp(16, b.len().min(256));
    let mut brands: Vec<&[u8]> = vec![&b[8..12]];
    let mut i = 16;
    while i + 4 <= end {
        brands.push(&b[i..i + 4]);
        i += 4;
    }
    let has = |names: &[&[u8; 4]]| brands.iter().any(|br| names.iter().any(|n| br == n));
    let major = &b[8..12];
    if major == b"crx " || has(&[b"crx "]) {
        return Some(ImageFormat::Raw);
    }
    if major == b"avif" || major == b"avis" {
        return Some(ImageFormat::Avif);
    }
    if [
        b"heic", b"heix", b"hevc", b"hevx", b"heim", b"heis", b"hevm", b"hevs",
    ]
    .iter()
    .any(|n| major == *n)
    {
        return Some(ImageFormat::Heic);
    }
    // Generic HEIF major brand: decide by compatible brands.
    if has(&[b"avif", b"avis"]) {
        return Some(ImageFormat::Avif);
    }
    if has(&[
        b"heic", b"heix", b"hevc", b"hevx", b"mif1", b"msf1", b"miaf",
    ]) {
        return Some(ImageFormat::Heic);
    }
    None
}

/// Extensions (lowercase, no dot) of everything this build can import.
pub fn input_extensions() -> Vec<&'static str> {
    let mut v = vec![
        "jpg", "jpeg", "jpe", "jfif", "png", "apng", "webp", "avif", "tif", "tiff", "bmp", "dib",
        "gif", "ico", "qoi", "tga", "jxl",
    ];
    if cfg!(feature = "heif") {
        v.extend(["heic", "heif", "hif"]);
    }
    v.extend_from_slice(RAW_EXTENSIONS);
    v
}

/// RAW signatures that are unambiguous from the first bytes.
fn is_raw_magic(b: &[u8]) -> bool {
    // Fujifilm RAF
    if b.starts_with(b"FUJIFILMCCD-RAW") {
        return true;
    }
    // Olympus ORF ("IIRO", "IIRS", "MMOR"), Panasonic RW2/RAW/RWL ("IIU\0")
    if b.starts_with(b"IIRO") || b.starts_with(b"IIRS") || b.starts_with(b"MMOR") {
        return true;
    }
    if b.starts_with(b"IIU\0") {
        return true;
    }
    // Minolta MRW, Sigma X3F
    if b.starts_with(b"\0MRM") || b.starts_with(b"FOVb") {
        return true;
    }
    // Canon CRW (CIFF)
    if b.len() >= 14 && (b.starts_with(b"II") || b.starts_with(b"MM")) && &b[6..14] == b"HEAPCCDR" {
        return true;
    }
    // Canon CR2: TIFF header followed by "CR\x02"
    if b.len() >= 11 && b.starts_with(b"II*\0") && &b[8..11] == b"CR\x02" {
        return true;
    }
    // DNG: TIFF whose IFD0 carries DNGVersion (0xC612).
    tiff_ifd0_has_tag(b, 0xC612)
}

/// Whether the first IFD of a classic TIFF in `b` contains `tag`.
fn tiff_ifd0_has_tag(b: &[u8], tag: u16) -> bool {
    let le = if b.starts_with(b"II*\0") {
        true
    } else if b.starts_with(b"MM\0*") {
        false
    } else {
        return false;
    };
    let u16_at = |p: usize| -> Option<u16> {
        let s = b.get(p..p + 2)?;
        Some(if le {
            u16::from_le_bytes([s[0], s[1]])
        } else {
            u16::from_be_bytes([s[0], s[1]])
        })
    };
    let u32_at = |p: usize| -> Option<u32> {
        let s = b.get(p..p + 4)?;
        let a = [s[0], s[1], s[2], s[3]];
        Some(if le {
            u32::from_le_bytes(a)
        } else {
            u32::from_be_bytes(a)
        })
    };
    let Some(off) = u32_at(4).map(|o| o as usize) else {
        return false;
    };
    let Some(n) = u16_at(off) else {
        return false;
    };
    (0..n as usize).any(|i| u16_at(off + 2 + i * 12) == Some(tag))
}

/// Capability row for the UI.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FormatCapabilities {
    pub format: ImageFormat,
    pub label: String,
    pub extension: String,
    pub mime: String,
    pub can_decode: bool,
    pub can_encode: bool,
    pub lossy: bool,
    pub lossless: bool,
    pub alpha: bool,
    pub exif: bool,
    pub icc: bool,
}

/// Capability table for every known format.
pub fn capabilities() -> Vec<FormatCapabilities> {
    ALL_FORMATS
        .iter()
        .map(|&f| FormatCapabilities {
            format: f,
            label: f.label().to_string(),
            extension: f.extension().to_string(),
            mime: f.mime().to_string(),
            can_decode: f.can_decode(),
            can_encode: f.can_encode(),
            lossy: f.supports_lossy(),
            lossless: f.supports_lossless(),
            alpha: f.supports_alpha(),
            exif: f.supports_exif(),
            icc: f.supports_icc(),
        })
        .collect()
}

/// Detect from content first, falling back to the extension. TIFF-based
/// RAW files (NEF, ARW, ...) are TIFF by content and RAW by extension.
pub(crate) fn resolve(bytes: &[u8], path: &Path) -> Option<ImageFormat> {
    with_hint(ImageFormat::detect(bytes), ImageFormat::from_path(path))
}

/// Combine a content detection with an extension/caller hint.
pub(crate) fn with_hint(
    detected: Option<ImageFormat>,
    hint: Option<ImageFormat>,
) -> Option<ImageFormat> {
    match (detected, hint) {
        (Some(ImageFormat::Tiff), Some(ImageFormat::Raw)) => Some(ImageFormat::Raw),
        (Some(f), _) => Some(f),
        (None, h) => h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_extensions_cover_rawler() {
        for e in rawler::decoders::supported_extensions() {
            let e = e.to_ascii_lowercase();
            assert!(
                RAW_EXTENSIONS.contains(&e.as_str()),
                "missing RAW extension {e}"
            );
            assert_eq!(ImageFormat::from_extension(&e), Some(ImageFormat::Raw));
        }
    }
}
