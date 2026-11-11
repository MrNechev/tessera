//! Decoding, probing and fast previews.

use std::fs::File;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use image::{DynamicImage, GrayImage, ImageDecoder, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::color::{self, ColorMode, LoadOptions};
use crate::container::{jpeg, normalize_exif, png, webp};
use crate::error::{guard, Error, Result};
use crate::exif_util;
use crate::format::{self, ImageFormat};
use crate::resize::{fit_dimensions, resize_exact};

/// Largest width/height we accept from a decoder.
const MAX_DIMENSION: u32 = 100_000;
/// Upper bound for a single decoder allocation.
const MAX_ALLOC: u64 = 4 * 1024 * 1024 * 1024;

/// Raw metadata blocks as found in the source file.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RawMetadata {
    /// Bare TIFF structure (starts with "II*\0" / "MM\0*"), no "Exif\0\0".
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

/// A decoded image with its metadata.
#[derive(Clone, Debug)]
pub struct SourceImage {
    /// Pixels, already rotated/flipped upright.
    pub image: DynamicImage,
    pub format: ImageFormat,
    /// Original orientation (EXIF 1..=8; 1 if none). Already applied to
    /// `image`. For HEIC/AVIF/JXL the container transform is applied by the
    /// decoder and this is informational only.
    pub orientation: u16,
    pub metadata: RawMetadata,
    pub file_size: u64,
    /// The pixels were converted from a non-sRGB embedded ICC profile to
    /// sRGB (`metadata.icc` is then `None`). Always false with
    /// [`ColorMode::Keep`].
    pub color_converted: bool,
}

/// Quick file information for import.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub path: String,
    pub format: ImageFormat,
    /// Upright width (after orientation).
    pub width: u32,
    /// Upright height (after orientation).
    pub height: u32,
    pub orientation: u16,
    pub file_size: u64,
    /// EXIF DateTimeOriginal (falls back to DateTimeDigitized) as
    /// "YYYY-MM-DDTHH:MM:SS".
    pub taken_at: Option<String>,
}

/// Result of [`make_preview`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub width: u32,
    pub height: u32,
    /// Path actually written (extension adjusted to .jpg or .png).
    pub path: String,
}

pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error::file(path, e))
}

fn unknown_format(path: &Path) -> Error {
    Error::UnknownFormat(format!(": {}", path.display()))
}

pub(crate) fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_image_width = Some(MAX_DIMENSION);
    l.max_image_height = Some(MAX_DIMENSION);
    l.max_alloc = Some(MAX_ALLOC);
    l
}

/// Decode a file fully. Pixels are returned upright and in sRGB (an
/// embedded non-sRGB ICC profile is applied, see [`load_with`]).
pub fn load(path: &Path) -> Result<SourceImage> {
    load_with(path, &LoadOptions::default())
}

/// Decode a file fully with explicit options. With [`ColorMode::Srgb`] the
/// pixels of images whose embedded ICC profile is not (near-)sRGB are
/// converted to sRGB and `metadata.icc` is cleared; with
/// [`ColorMode::Keep`] pixels and profile are returned untouched.
pub fn load_with(path: &Path, opts: &LoadOptions) -> Result<SourceImage> {
    let bytes = read_file(path)?;
    let format = format::resolve(&bytes, path).ok_or_else(|| unknown_format(path))?;
    Ok(decode_color(&bytes, format, opts.color)?.0)
}

/// Decode an in-memory file (colour-managed to sRGB like [`load`]). `hint`
/// is used when the content has no recognisable signature (TGA) and to
/// identify TIFF-based camera RAW files (`Some(ImageFormat::Raw)`).
pub fn load_bytes(bytes: &[u8], hint: Option<ImageFormat>) -> Result<SourceImage> {
    load_bytes_with(bytes, hint, &LoadOptions::default())
}

/// [`load_bytes`] with explicit options.
pub fn load_bytes_with(
    bytes: &[u8],
    hint: Option<ImageFormat>,
    opts: &LoadOptions,
) -> Result<SourceImage> {
    let format = format::with_hint(ImageFormat::detect(bytes), hint)
        .ok_or_else(|| Error::UnknownFormat(String::new()))?;
    Ok(decode_color(bytes, format, opts.color)?.0)
}

/// Decode without colour management (pixels and ICC as stored).
pub(crate) fn decode_bytes(bytes: &[u8], format: ImageFormat) -> Result<SourceImage> {
    let label = format.label();
    let (mut image, metadata, orientation, needs_orient) =
        guard(label, true, || decode_inner(bytes, format))?;
    if needs_orient && orientation != 1 {
        if let Some(o) = image::metadata::Orientation::from_exif(orientation as u8) {
            image.apply_orientation(o);
        }
    }
    Ok(SourceImage {
        image,
        format,
        orientation,
        metadata,
        file_size: bytes.len() as u64,
        color_converted: false,
    })
}

/// Decode and apply `mode`. Also returns the description of the profile the
/// pixels were converted from, when a conversion happened.
pub(crate) fn decode_color(
    bytes: &[u8],
    format: ImageFormat,
    mode: ColorMode,
) -> Result<(SourceImage, Option<String>)> {
    let mut src = decode_bytes(bytes, format)?;
    let from = apply_color(&mut src.image, &mut src.metadata.icc, mode);
    src.color_converted = from.is_some();
    Ok((src, from))
}

/// Convert `img` to sRGB in place when `mode` is `Srgb` and `icc` is a
/// usable non-sRGB profile; the profile is then cleared. Returns the source
/// profile description on conversion.
pub(crate) fn apply_color(
    img: &mut DynamicImage,
    icc: &mut Option<Vec<u8>>,
    mode: ColorMode,
) -> Option<String> {
    if mode != ColorMode::Srgb {
        return None;
    }
    let c = color::to_srgb(img, icc.as_deref()?)?;
    *img = c.image;
    *icc = None;
    Some(c.from)
}

fn image_err(label: &'static str, e: image::ImageError) -> Error {
    match e {
        image::ImageError::Limits(l) => Error::decode(label, format!("image is too large ({l})")),
        other => Error::decode(label, other),
    }
}

struct Generic {
    image: DynamicImage,
    icc: Option<Vec<u8>>,
    orientation: u16,
}

fn decode_generic(bytes: &[u8], format: ImageFormat) -> Result<Generic> {
    let label = format.label();
    let f = format
        .to_image_format()
        .ok_or_else(|| Error::Unsupported(format!("{label} decoding is not available")))?;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), f);
    reader.limits(limits());
    let mut dec = reader.into_decoder().map_err(|e| image_err(label, e))?;
    let icc = dec.icc_profile().ok().flatten();
    let orientation = dec.orientation().map(|o| o.to_exif() as u16).unwrap_or(1);
    let image = DynamicImage::from_decoder(dec).map_err(|e| image_err(label, e))?;
    Ok(Generic {
        image,
        icc,
        orientation,
    })
}

type Decoded = (DynamicImage, RawMetadata, u16, bool);

fn decode_inner(bytes: &[u8], format: ImageFormat) -> Result<Decoded> {
    let label = format.label();
    match format {
        ImageFormat::Jpeg => {
            let info = jpeg::parse(bytes)?.info();
            let g = decode_generic(bytes, format)?;
            let orientation = info
                .exif
                .as_deref()
                .map(exif_util::read_orientation)
                .unwrap_or(1);
            let md = RawMetadata {
                exif: info.exif,
                icc: info.icc,
                xmp: info.xmp,
            };
            Ok((g.image, md, orientation, true))
        }
        ImageFormat::Png => {
            let info = png::info(&png::parse(bytes)?);
            let g = decode_generic(bytes, format)?;
            let orientation = info
                .exif
                .as_deref()
                .map(exif_util::read_orientation)
                .unwrap_or(1);
            let md = RawMetadata {
                exif: info.exif,
                icc: info.icc,
                xmp: info.xmp,
            };
            Ok((g.image, md, orientation, true))
        }
        ImageFormat::Webp => {
            let info = webp::info(&webp::parse(bytes)?);
            let g = decode_generic(bytes, format)?;
            let orientation = info
                .exif
                .as_deref()
                .map(exif_util::read_orientation)
                .unwrap_or(1);
            let md = RawMetadata {
                exif: info.exif,
                icc: info.icc,
                xmp: info.xmp,
            };
            Ok((g.image, md, orientation, true))
        }
        ImageFormat::Tiff => {
            let g = decode_generic(bytes, format)?;
            let (exif, icc, xmp) = exif_util::tiff_file_metadata(bytes);
            let md = RawMetadata { exif, icc, xmp };
            Ok((g.image, md, g.orientation, true))
        }
        ImageFormat::Avif => {
            #[cfg(feature = "heif")]
            if let Ok((image, md)) = crate::heif::decode(bytes, label) {
                let o = md
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                return Ok((image, md, o, false));
            }
            let g = decode_generic(bytes, format)?;
            let md = RawMetadata {
                exif: None,
                icc: g.icc,
                xmp: None,
            };
            Ok((g.image, md, 1, false))
        }
        ImageFormat::Heic => {
            #[cfg(feature = "heif")]
            {
                let (image, md) = crate::heif::decode(bytes, label)?;
                let o = md
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                Ok((image, md, o, false))
            }
            #[cfg(not(feature = "heif"))]
            Err(Error::Unsupported(
                "HEIC support is not included in this build".into(),
            ))
        }
        ImageFormat::Jxl => {
            let mut dec = jxl_oxide::integration::JxlDecoder::new(Cursor::new(bytes))
                .map_err(|e| image_err(label, e))?;
            dec.set_limits(limits()).map_err(|e| image_err(label, e))?;
            let icc = dec.icc_profile().ok().flatten();
            let exif = dec
                .exif_metadata()
                .ok()
                .flatten()
                .and_then(|e| normalize_exif(&e));
            let image = DynamicImage::from_decoder(dec).map_err(|e| image_err(label, e))?;
            let o = exif
                .as_deref()
                .map(exif_util::read_orientation)
                .unwrap_or(1);
            let md = RawMetadata {
                exif,
                icc,
                xmp: None,
            };
            // jxl-oxide applies the codestream orientation itself.
            Ok((image, md, o, false))
        }
        ImageFormat::Raw => {
            let f = crate::raw::open(bytes)?;
            let (image, o) = f.develop()?;
            Ok((image, crate::raw::metadata(&f), o, true))
        }
        ImageFormat::Gif
        | ImageFormat::Bmp
        | ImageFormat::Ico
        | ImageFormat::Qoi
        | ImageFormat::Tga => {
            let g = decode_generic(bytes, format)?;
            let md = RawMetadata {
                exif: None,
                icc: g.icc,
                xmp: None,
            };
            Ok((g.image, md, 1, false))
        }
    }
}

// ---------------------------------------------------------------------------
// Probing (header + metadata only)

/// Header/metadata scan result shared by `probe` and `read_summary`.
#[derive(Clone, Debug)]
pub(crate) struct Scan {
    pub format: ImageFormat,
    /// Upright dimensions.
    pub width: u32,
    pub height: u32,
    pub orientation: u16,
    pub metadata: RawMetadata,
    pub file_size: u64,
}

const PROBE_HEAD: usize = 256 * 1024;

fn read_head(file: &mut File, max: usize) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(max.min(64 * 1024));
    file.by_ref().take(max as u64).read_to_end(&mut head)?;
    Ok(head)
}

fn upright(w: u32, h: u32, orientation: u16) -> (u32, u32) {
    if (5..=8).contains(&orientation) {
        (h, w)
    } else {
        (w, h)
    }
}

pub(crate) fn scan(path: &Path) -> Result<Scan> {
    let mut file = File::open(path).map_err(|e| Error::file(path, e))?;
    let file_size = file.metadata().map_err(|e| Error::file(path, e))?.len();
    let head = read_head(&mut file, PROBE_HEAD).map_err(|e| Error::file(path, e))?;
    let format = format::resolve(&head, path).ok_or_else(|| unknown_format(path))?;
    let label = format.label();
    let whole = |file: &mut File| -> Result<Vec<u8>> {
        file.seek(SeekFrom::Start(0))
            .map_err(|e| Error::file(path, e))?;
        let mut v = Vec::with_capacity(file_size as usize);
        file.read_to_end(&mut v).map_err(|e| Error::file(path, e))?;
        Ok(v)
    };
    let scan = guard(label, true, || -> Result<Scan> {
        let (w, h, orientation, md, needs_orient) = match format {
            ImageFormat::Jpeg => {
                let full;
                let parsed = match jpeg::parse_partial(&head)? {
                    jpeg::Parse::Done(j) => j,
                    jpeg::Parse::NeedMore => {
                        full = whole(&mut file)?;
                        jpeg::parse(&full)?
                    }
                };
                let info = parsed.info();
                if info.width == 0 || info.height == 0 {
                    return Err(Error::decode(label, "missing frame header"));
                }
                let o = info
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                let md = RawMetadata {
                    exif: info.exif,
                    icc: info.icc,
                    xmp: info.xmp,
                };
                (info.width, info.height, o, md, true)
            }
            ImageFormat::Png => {
                let owned = png_chunks_streaming(&mut file).map_err(|e| Error::decode(label, e))?;
                let chunks: Vec<png::Chunk> = owned
                    .iter()
                    .map(|(k, d)| png::Chunk { kind: *k, data: d })
                    .collect();
                if chunks.first().map(|c| &c.kind) != Some(b"IHDR") {
                    return Err(Error::decode(label, "missing IHDR chunk"));
                }
                let info = png::info(&chunks);
                let o = info
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                let md = RawMetadata {
                    exif: info.exif,
                    icc: info.icc,
                    xmp: info.xmp,
                };
                (info.width, info.height, o, md, true)
            }
            ImageFormat::Webp => {
                let owned =
                    webp_chunks_streaming(&mut file).map_err(|e| Error::decode(label, e))?;
                let chunks: Vec<webp::Chunk> = owned
                    .iter()
                    .map(|(k, d)| webp::Chunk {
                        fourcc: *k,
                        data: d,
                    })
                    .collect();
                let info = webp::info(&chunks);
                if info.width == 0 || info.height == 0 {
                    return Err(Error::decode(label, "missing image header"));
                }
                let o = info
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                let md = RawMetadata {
                    exif: info.exif,
                    icc: info.icc,
                    xmp: info.xmp,
                };
                (info.width, info.height, o, md, true)
            }
            ImageFormat::Heic | ImageFormat::Avif => {
                let bytes = whole(&mut file)?;
                scan_heif_like(&bytes, format)?
            }
            ImageFormat::Jxl => {
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| Error::file(path, e))?;
                let mut dec = jxl_oxide::integration::JxlDecoder::new(BufReader::new(&mut file))
                    .map_err(|e| image_err(label, e))?;
                let (w, h) = dec.dimensions();
                let icc = dec.icc_profile().ok().flatten();
                let exif = dec
                    .exif_metadata()
                    .ok()
                    .flatten()
                    .and_then(|e| normalize_exif(&e));
                let o = exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                // Dimensions are already oriented by jxl-oxide.
                (
                    w,
                    h,
                    o,
                    RawMetadata {
                        exif,
                        icc,
                        xmp: None,
                    },
                    false,
                )
            }
            ImageFormat::Raw => {
                let bytes = whole(&mut file)?;
                let f = crate::raw::open(&bytes)?;
                let (w, h, o) = f.dimensions()?;
                (w, h, o, crate::raw::metadata(&f), true)
            }
            ImageFormat::Tiff => {
                let bytes = whole(&mut file)?;
                let f =
                    image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Tiff);
                let mut dec = f.into_decoder().map_err(|e| image_err(label, e))?;
                let (w, h) = dec.dimensions();
                let o = dec.orientation().map(|o| o.to_exif() as u16).unwrap_or(1);
                let (exif, icc, xmp) = exif_util::tiff_file_metadata(&bytes);
                (w, h, o, RawMetadata { exif, icc, xmp }, true)
            }
            ImageFormat::Gif
            | ImageFormat::Bmp
            | ImageFormat::Ico
            | ImageFormat::Qoi
            | ImageFormat::Tga => {
                let f = format
                    .to_image_format()
                    .ok_or_else(|| unknown_format(path))?;
                file.seek(SeekFrom::Start(0))
                    .map_err(|e| Error::file(path, e))?;
                let r = image::ImageReader::with_format(BufReader::new(&mut file), f);
                let (w, h) = r.into_dimensions().map_err(|e| image_err(label, e))?;
                (w, h, 1, RawMetadata::default(), false)
            }
        };
        let (w, h) = if needs_orient {
            upright(w, h, orientation)
        } else {
            (w, h)
        };
        Ok(Scan {
            format,
            width: w,
            height: h,
            orientation,
            metadata: md,
            file_size,
        })
    })?;
    Ok(scan)
}

fn scan_heif_like(bytes: &[u8], format: ImageFormat) -> Result<(u32, u32, u16, RawMetadata, bool)> {
    let label = format.label();
    #[cfg(feature = "heif")]
    {
        match crate::heif::probe(bytes, label) {
            Ok((w, h, md)) => {
                let o = md
                    .exif
                    .as_deref()
                    .map(exif_util::read_orientation)
                    .unwrap_or(1);
                return Ok((w, h, o, md, false));
            }
            Err(e) if format == ImageFormat::Heic => return Err(e),
            Err(_) => {}
        }
    }
    if format == ImageFormat::Heic {
        return Err(Error::Unsupported(
            "HEIC support is not included in this build".into(),
        ));
    }
    let r = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Avif);
    let mut dec = r.into_decoder().map_err(|e| image_err(label, e))?;
    let (w, h) = dec.dimensions();
    let icc = dec.icc_profile().ok().flatten();
    Ok((
        w,
        h,
        1,
        RawMetadata {
            exif: None,
            icc,
            xmp: None,
        },
        false,
    ))
}

const MAX_META_CHUNK: u32 = 16 * 1024 * 1024;

fn png_chunks_streaming(file: &mut File) -> std::io::Result<Vec<([u8; 4], Vec<u8>)>> {
    file.seek(SeekFrom::Start(8))?;
    let mut r = BufReader::new(file);
    let mut out = Vec::new();
    let mut hdr = [0u8; 8];
    loop {
        if r.read_exact(&mut hdr).is_err() {
            break;
        }
        let len = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let kind = [hdr[4], hdr[5], hdr[6], hdr[7]];
        let wanted = matches!(
            &kind,
            b"IHDR" | b"tRNS" | b"eXIf" | b"iCCP" | b"iTXt" | b"zTXt" | b"tEXt"
        );
        if wanted && len <= MAX_META_CHUNK {
            let mut data = vec![0u8; len as usize];
            r.read_exact(&mut data)?;
            r.seek_relative(4)?;
            out.push((kind, data));
        } else {
            r.seek_relative(len as i64 + 4)?;
        }
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(out)
}

fn webp_chunks_streaming(file: &mut File) -> std::io::Result<Vec<([u8; 4], Vec<u8>)>> {
    file.seek(SeekFrom::Start(0))?;
    let mut r = BufReader::new(file);
    let mut hdr = [0u8; 12];
    r.read_exact(&mut hdr)?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WEBP" {
        return Err(std::io::Error::other("missing RIFF/WEBP header"));
    }
    let mut out = Vec::new();
    let mut ch = [0u8; 8];
    loop {
        if r.read_exact(&mut ch).is_err() {
            break;
        }
        let kind = [ch[0], ch[1], ch[2], ch[3]];
        let len = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]);
        let padded = len as i64 + (len & 1) as i64;
        match &kind {
            b"VP8X" | b"EXIF" | b"ICCP" | b"XMP " if len <= MAX_META_CHUNK => {
                let mut data = vec![0u8; len as usize];
                r.read_exact(&mut data)?;
                r.seek_relative(padded - len as i64)?;
                out.push((kind, data));
            }
            b"VP8 " | b"VP8L" => {
                let n = len.min(32) as usize;
                let mut data = vec![0u8; n];
                r.read_exact(&mut data)?;
                r.seek_relative(padded - n as i64)?;
                out.push((kind, data));
            }
            b"ALPH" => {
                r.seek_relative(padded)?;
                out.push((kind, Vec::new()));
            }
            _ => r.seek_relative(padded)?,
        }
    }
    Ok(out)
}

/// Fast probe: dimensions (upright), orientation and capture date, without
/// decoding pixels for JPEG/PNG/WebP/HEIC/JXL/TIFF.
pub fn probe(path: &Path) -> Result<ProbeInfo> {
    let s = scan(path)?;
    let taken_at = s.metadata.exif.as_deref().and_then(taken_at);
    Ok(ProbeInfo {
        path: path.to_string_lossy().into_owned(),
        format: s.format,
        width: s.width,
        height: s.height,
        orientation: s.orientation,
        file_size: s.file_size,
        taken_at,
    })
}

pub(crate) fn taken_at(exif: &[u8]) -> Option<String> {
    let e = exif_util::parse(exif)?;
    exif_util::raw_ascii(&e, exif::Tag::DateTimeOriginal)
        .and_then(|r| exif_util::exif_datetime_to_iso(&r))
        .or_else(|| {
            exif_util::raw_ascii(&e, exif::Tag::DateTimeDigitized)
                .and_then(|r| exif_util::exif_datetime_to_iso(&r))
        })
}

// ---------------------------------------------------------------------------
// Scaled loading and previews

/// Decode a JPEG at reduced size using libjpeg's DCT-domain scaling (1/2,
/// 1/4, 1/8), apply orientation, and resize to fit inside (max_w, max_h).
fn jpeg_scaled_upright(
    bytes: &[u8],
    max_w: u32,
    max_h: u32,
    mode: ColorMode,
) -> Result<DynamicImage> {
    let info = jpeg::parse(bytes)?.info();
    if info.components != 1 && info.components != 3 {
        return Err(Error::Unsupported("CMYK JPEG".into()));
    }
    let orientation = info
        .exif
        .as_deref()
        .map(exif_util::read_orientation)
        .unwrap_or(1);
    let (sw, sh) = (info.width, info.height);
    let (uw, uh) = upright(sw, sh, orientation);
    let (tw, th) = fit_dimensions(uw, uh, max_w, max_h);
    let (stw, sth) = upright(tw, th, orientation);
    let scaled_side = |side: u32, n: u32| (side as u64 * n as u64).div_ceil(8) as u32;
    let n = [1u32, 2, 4, 8]
        .into_iter()
        .find(|&n| scaled_side(sw, n) >= stw && scaled_side(sh, n) >= sth)
        .unwrap_or(8);
    let mut img = guard("JPEG", true, || -> Result<DynamicImage> {
        let io = |e: std::io::Error| Error::decode("JPEG", e);
        let mut d = mozjpeg::Decompress::new_mem(bytes).map_err(io)?;
        d.scale(n as u8);
        let img = match d.image().map_err(io)? {
            mozjpeg::Format::RGB(mut s) => {
                let (w, h) = (s.width() as u32, s.height() as u32);
                let px = s.read_scanlines::<u8>().map_err(io)?;
                let _ = s.finish();
                DynamicImage::ImageRgb8(
                    RgbImage::from_raw(w, h, px)
                        .ok_or_else(|| Error::decode("JPEG", "short read"))?,
                )
            }
            mozjpeg::Format::Gray(mut s) => {
                let (w, h) = (s.width() as u32, s.height() as u32);
                let px = s.read_scanlines::<u8>().map_err(io)?;
                let _ = s.finish();
                DynamicImage::ImageLuma8(
                    GrayImage::from_raw(w, h, px)
                        .ok_or_else(|| Error::decode("JPEG", "short read"))?,
                )
            }
            mozjpeg::Format::CMYK(_) => return Err(Error::Unsupported("CMYK JPEG".into())),
        };
        Ok(img)
    })?;
    if orientation != 1 {
        if let Some(o) = image::metadata::Orientation::from_exif(orientation as u8) {
            img.apply_orientation(o);
        }
    }
    let mut icc = info.icc;
    apply_color(&mut img, &mut icc, mode);
    Ok(resize_exact(img, tw, th))
}

/// Embedded preview of a RAW file, upright, when it covers `max_w` x
/// `max_h` (0 = unbounded never qualifies).
fn raw_preview_upright(bytes: &[u8], max_w: u32, max_h: u32) -> Option<DynamicImage> {
    if max_w == 0 || max_h == 0 {
        return None;
    }
    guard("Camera RAW", true, || -> Result<Option<DynamicImage>> {
        let f = crate::raw::open(bytes)?;
        let (fw, fh, o) = f.dimensions()?;
        let Some(mut img) = f.preview() else {
            return Ok(None);
        };
        // A preview stored for a different aspect ratio (e.g. 16:9 crop of a
        // 3:2 sensor) would change the framing; require a close match.
        let (pw, ph) = (img.width() as f64, img.height() as f64);
        let (rw, rh) = (fw as f64, fh as f64);
        if pw < 1.0 || ph < 1.0 || ((pw / ph) - (rw / rh)).abs() > 0.02 * (rw / rh) {
            return Ok(None);
        }
        if o != 1 {
            if let Some(o) = image::metadata::Orientation::from_exif(o as u8) {
                img.apply_orientation(o);
            }
        }
        let (uw, uh) = upright(fw, fh, o);
        let (tw, th) = fit_dimensions(uw, uh, max_w, max_h);
        if img.width() < tw || img.height() < th {
            return Ok(None);
        }
        let (tw, th) = fit_dimensions(img.width(), img.height(), max_w, max_h);
        Ok(Some(resize_exact(img, tw, th)))
    })
    .ok()
    .flatten()
}

fn scaled_upright(
    bytes: &[u8],
    format: ImageFormat,
    max_w: u32,
    max_h: u32,
    mode: ColorMode,
) -> Result<DynamicImage> {
    if format == ImageFormat::Jpeg {
        if let Ok(img) = jpeg_scaled_upright(bytes, max_w, max_h, mode) {
            return Ok(img);
        }
    }
    if format == ImageFormat::Raw {
        if let Some(img) = raw_preview_upright(bytes, max_w, max_h) {
            return Ok(img);
        }
    }
    let (src, _) = decode_color(bytes, format, mode)?;
    let (tw, th) = fit_dimensions(src.image.width(), src.image.height(), max_w, max_h);
    Ok(resize_exact(src.image, tw, th))
}

/// Decode `path` upright, fitting inside `max_w` x `max_h` (0 = unbounded),
/// never upscaling, as 8-bit RGBA in sRGB (embedded non-sRGB ICC profiles
/// are applied). JPEGs use DCT-domain downscaling; camera RAW files use the
/// embedded preview when it is large enough (see [`make_preview`]).
pub fn load_scaled_rgba(path: &Path, max_w: u32, max_h: u32) -> Result<RgbaImage> {
    let bytes = read_file(path)?;
    let format = format::resolve(&bytes, path).ok_or_else(|| unknown_format(path))?;
    Ok(scaled_upright(&bytes, format, max_w, max_h, ColorMode::Srgb)?.to_rgba8())
}

fn has_transparency(img: &DynamicImage) -> bool {
    match img {
        DynamicImage::ImageLumaA8(i) => i.pixels().any(|p| p.0[1] != u8::MAX),
        DynamicImage::ImageRgba8(i) => i.pixels().any(|p| p.0[3] != u8::MAX),
        DynamicImage::ImageLumaA16(i) => i.pixels().any(|p| p.0[1] != u16::MAX),
        DynamicImage::ImageRgba16(i) => i.pixels().any(|p| p.0[3] != u16::MAX),
        DynamicImage::ImageRgba32F(i) => i.pixels().any(|p| p.0[3] < 1.0),
        _ => false,
    }
}

pub(crate) fn image_has_transparency(img: &DynamicImage) -> bool {
    has_transparency(img)
}

fn with_ext(dst: &Path, allowed: &[&str], ext: &str) -> PathBuf {
    let cur = dst
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match cur {
        Some(c) if allowed.contains(&c.as_str()) => dst.to_path_buf(),
        _ => dst.with_extension(ext),
    }
}

/// Write an upright preview proxy of `src` no larger than `max_side` on its
/// long edge: JPEG (`jpeg_quality`) for opaque images, PNG when the image has
/// any transparency. The extension of `dst` is adjusted accordingly and the
/// path actually written is returned. Pixels are converted to sRGB.
///
/// Camera RAW: when the file embeds a preview (normally the camera JPEG)
/// with the full frame's aspect ratio whose upright size covers the
/// requested size, that preview is used (fast, and it carries the camera's
/// own rendering); otherwise the RAW is developed (seconds for 20+ MP).
pub fn make_preview(
    src: &Path,
    dst: &Path,
    max_side: u32,
    jpeg_quality: u8,
) -> Result<PreviewInfo> {
    let bytes = read_file(src)?;
    let format = format::resolve(&bytes, src).ok_or_else(|| unknown_format(src))?;
    let img = scaled_upright(&bytes, format, max_side, max_side, ColorMode::Srgb)?;
    let (width, height) = (img.width(), img.height());
    let (path, data) = if has_transparency(&img) {
        let path = with_ext(dst, &["png"], "png");
        (path, crate::encode::png_fast(&img.to_rgba8())?)
    } else {
        let path = with_ext(dst, &["jpg", "jpeg"], "jpg");
        let data = crate::encode::jpeg_fast(&img, jpeg_quality.clamp(1, 100))?;
        (path, data)
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::file(parent, e))?;
        }
    }
    // Temp file + rename: a crash mid-write must not leave a truncated
    // proxy that later runs would trust as a cache hit.
    crate::jobs::atomic_write(&path, &data)?;
    Ok(PreviewInfo {
        width,
        height,
        path: path.to_string_lossy().into_owned(),
    })
}
