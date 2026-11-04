//! HEIC/HEIF (and AVIF, when libheif has an AV1 decoder) via libheif.

use image::{DynamicImage, RgbImage, RgbaImage};
use libheif_rs::{ColorSpace, HeifContext, ImageHandle, LibHeif, RgbChroma};

use crate::container::normalize_exif;
use crate::decode::RawMetadata;
use crate::error::{Error, Result};

fn err(label: &'static str, e: impl std::fmt::Display) -> Error {
    Error::decode(label, e)
}

fn metadata(handle: &ImageHandle) -> RawMetadata {
    let mut md = RawMetadata::default();
    for m in handle.all_metadata() {
        if &m.item_type.0 == b"Exif" && md.exif.is_none() {
            md.exif = normalize_exif(&m.raw_data);
        } else if m.content_type == "application/rdf+xml" && md.xmp.is_none() {
            md.xmp = Some(m.raw_data);
        }
    }
    if let Some(p) = handle.color_profile_raw() {
        let t = libheif_rs::ColorProfile::profile_type(&p).0;
        if (&t == b"prof" || &t == b"rICC") && !p.data.is_empty() {
            md.icc = Some(p.data);
        }
    }
    md
}

/// Upright dimensions and metadata without decoding pixels.
pub(crate) fn probe(bytes: &[u8], label: &'static str) -> Result<(u32, u32, RawMetadata)> {
    let ctx = HeifContext::read_from_bytes(bytes).map_err(|e| err(label, e))?;
    let handle = ctx.primary_image_handle().map_err(|e| err(label, e))?;
    // Handle dimensions already account for rotation/mirroring transforms.
    Ok((handle.width(), handle.height(), metadata(&handle)))
}

/// Decode the primary image (transforms such as `irot`/`imir` applied by
/// libheif) to 8-bit RGB(A).
pub(crate) fn decode(bytes: &[u8], label: &'static str) -> Result<(DynamicImage, RawMetadata)> {
    let lib = LibHeif::new_checked().map_err(|e| err(label, e))?;
    let ctx = HeifContext::read_from_bytes(bytes).map_err(|e| err(label, e))?;
    let handle = ctx.primary_image_handle().map_err(|e| err(label, e))?;
    let md = metadata(&handle);
    let alpha = handle.has_alpha_channel();
    let chroma = if alpha {
        RgbChroma::Rgba
    } else {
        RgbChroma::Rgb
    };
    let img = lib
        .decode(&handle, ColorSpace::Rgb(chroma), None)
        .map_err(|e| err(label, e))?;
    let planes = img.planes();
    let plane = planes
        .interleaved
        .ok_or_else(|| err(label, "decoder returned no interleaved plane"))?;
    let (w, h) = (plane.width, plane.height);
    let channels = if alpha { 4 } else { 3 };
    if plane.storage_bits_per_pixel as usize != channels * 8 {
        return Err(err(label, "unexpected pixel layout"));
    }
    let row = w as usize * channels;
    let mut buf = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        let start = y * plane.stride;
        let line = plane
            .data
            .get(start..start + row)
            .ok_or_else(|| err(label, "decoded plane is too small"))?;
        buf.extend_from_slice(line);
    }
    let image = if alpha {
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(w, h, buf).ok_or_else(|| err(label, "bad buffer size"))?,
        )
    } else {
        DynamicImage::ImageRgb8(
            RgbImage::from_raw(w, h, buf).ok_or_else(|| err(label, "bad buffer size"))?,
        )
    };
    Ok((image, md))
}
