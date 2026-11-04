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
}

/// Every variant, in UI order.
pub const ALL_FORMATS: [ImageFormat; 12] = [
    ImageFormat::Jpeg,
    ImageFormat::Png,
    ImageFormat::Webp,
    ImageFormat::Avif,
    ImageFormat::Heic,
    ImageFormat::Jxl,
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
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        let b = bytes;
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
        }
    }

    /// Whether this build can read the format.
    pub fn can_decode(self) -> bool {
        !matches!(self, ImageFormat::Heic) || cfg!(feature = "heif")
    }

    /// Whether this build can write the format.
    pub fn can_encode(self) -> bool {
        !matches!(self, ImageFormat::Heic | ImageFormat::Jxl)
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
            ImageFormat::Heic | ImageFormat::Jxl => return None,
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
    v
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

/// Detect from content first, falling back to the extension.
pub(crate) fn resolve(bytes: &[u8], path: &Path) -> Option<ImageFormat> {
    ImageFormat::detect(bytes).or_else(|| ImageFormat::from_path(path))
}
