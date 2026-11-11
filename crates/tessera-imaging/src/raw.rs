//! Camera RAW decoding through `rawler`: metadata, embedded previews and a
//! default "develop" to 8-bit sRGB.
//!
//! Develop pipeline (rawler's `RawDevelop` steps, then our output stage):
//! black/white level scaling → demosaic (PPG for Bayer, bilinear for X-Trans
//! / 4-colour) → Fuji SuperCCD rotation → active-area crop → camera white
//! balance → camera colour matrix (D65, Bradford-adapted if needed) to
//! linear sRGB with highlight clipping → default crop → auto-exposure
//! (dcraw-style: the 99th percentile is mapped to white, gain limited to
//! 1..=4) → sRGB transfer curve → default tone curve (midtone lift + gentle
//! S-curve) → 8 bit. Pixels are returned in sensor orientation; the caller
//! applies `orientation`.

use std::io::Cursor;

use image::{DynamicImage, GrayImage, RgbImage};
use rawler::decoders::{Decoder, RawDecodeParams, RawMetadata as RawlerMetadata};
use rawler::formats::tiff::{DirectoryWriter, TiffWriter};
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};
use rawler::rawsource::RawSource;
use rawler::tags::{ExifTag, TiffCommonTag};
use rayon::prelude::*;

use crate::decode::RawMetadata;
use crate::error::{Error, Result};

const LABEL: &str = "Camera RAW";

fn err(e: impl std::fmt::Display) -> Error {
    Error::decode(LABEL, e)
}

/// Parsed RAW container (no pixel decoding yet).
pub(crate) struct RawFile {
    source: RawSource,
    decoder: Box<dyn Decoder>,
    meta: RawlerMetadata,
}

pub(crate) fn open(bytes: &[u8]) -> Result<RawFile> {
    let source = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&source).map_err(err)?;
    let meta = decoder
        .raw_metadata(&source, &RawDecodeParams::default())
        .map_err(err)?;
    Ok(RawFile {
        source,
        decoder,
        meta,
    })
}

impl RawFile {
    fn params() -> RawDecodeParams {
        RawDecodeParams::default()
    }

    /// EXIF orientation (1..=8) from the RAW metadata, else 1.
    fn exif_orientation(&self) -> Option<u16> {
        self.meta.exif.orientation.filter(|o| (1..=8).contains(o))
    }

    /// Sensor-orientation output size of the develop and the orientation,
    /// without decompressing pixels.
    pub fn dimensions(&self) -> Result<(u32, u32, u16)> {
        let raw = self
            .decoder
            .raw_image(&self.source, &Self::params(), true)
            .map_err(err)?;
        let o = self
            .exif_orientation()
            .or(Some(raw.orientation.to_u16()).filter(|o| (1..=8).contains(o)))
            .unwrap_or(1);
        let d = raw
            .crop_area
            .or(raw.active_area)
            .map(|r| (r.d.w, r.d.h))
            .unwrap_or((raw.width, raw.height));
        Ok((d.0 as u32, d.1 as u32, o))
    }

    /// Embedded preview (usually a camera JPEG), in sensor orientation.
    pub fn preview(&self) -> Option<DynamicImage> {
        let p = Self::params();
        let full = self.decoder.full_image(&self.source, &p).ok().flatten();
        let prev = || self.decoder.preview_image(&self.source, &p).ok().flatten();
        full.or_else(prev)
    }

    /// Bare TIFF EXIF block synthesised from rawler's metadata, Orientation
    /// set to 1 (the decoded pixels are upright).
    pub fn exif(&self) -> Option<Vec<u8>> {
        build_exif(&self.meta).ok()
    }

    /// Full decode + develop to 8-bit sRGB, plus the orientation to apply.
    pub fn develop(&self) -> Result<(DynamicImage, u16)> {
        let raw = self
            .decoder
            .raw_image(&self.source, &Self::params(), false)
            .map_err(err)?;
        let o = self
            .exif_orientation()
            .or(Some(raw.orientation.to_u16()).filter(|o| (1..=8).contains(o)))
            .unwrap_or(1);
        let dev = RawDevelop::new_with(&[
            ProcessingStep::Rescale,
            ProcessingStep::Demosaic,
            ProcessingStep::FujiRotate,
            ProcessingStep::CropActiveArea,
            ProcessingStep::WhiteBalance,
            ProcessingStep::Calibrate,
            ProcessingStep::CropDefault,
        ]);
        let inter = dev.develop_intermediate(&raw).map_err(err)?;
        let img = match inter {
            Intermediate::Monochrome(px) => {
                let (w, h) = (px.width, px.height);
                let data = px.data;
                let lut = output_lut(exposure_gain(&data));
                let out: Vec<u8> = data.par_iter().map(|&v| lut_at(&lut, v)).collect();
                DynamicImage::ImageLuma8(
                    GrayImage::from_raw(w as u32, h as u32, out)
                        .ok_or_else(|| err("bad image size"))?,
                )
            }
            Intermediate::ThreeColor(px) => {
                let (w, h) = (px.width, px.height);
                let data = px.into_flatten();
                to_rgb8(&data, w, h, 3)?
            }
            Intermediate::FourColor(px) => {
                let (w, h) = (px.width, px.height);
                let data = px.into_flatten();
                to_rgb8(&data, w, h, 4)?
            }
        };
        Ok((img, o))
    }
}

fn to_rgb8(data: &[f32], w: usize, h: usize, cpp: usize) -> Result<DynamicImage> {
    let lut = output_lut(exposure_gain(data));
    let mut out = vec![0u8; w * h * 3];
    out.par_chunks_mut(3)
        .zip(data.par_chunks(cpp))
        .for_each(|(o, i)| {
            o[0] = lut_at(&lut, i[0]);
            o[1] = lut_at(&lut, i[1]);
            o[2] = lut_at(&lut, i[2]);
        });
    Ok(DynamicImage::ImageRgb8(
        RgbImage::from_raw(w as u32, h as u32, out).ok_or_else(|| err("bad image size"))?,
    ))
}

const LUT_SIZE: usize = 1 << 14;

/// Gain such that 1 % of the samples clip (dcraw's auto-brightness),
/// limited to 1..=4.
fn exposure_gain(data: &[f32]) -> f32 {
    const BINS: usize = 4096;
    let hist = data
        .par_chunks(1 << 16)
        .map(|c| {
            let mut h = vec![0u32; BINS];
            for &v in c {
                let i = (v.clamp(0.0, 1.0) * (BINS - 1) as f32) as usize;
                h[i] += 1;
            }
            h
        })
        .reduce(
            || vec![0u32; BINS],
            |mut a, b| {
                a.iter_mut().zip(b).for_each(|(x, y)| *x += y);
                a
            },
        );
    let target = (data.len() as f64 * 0.01) as u64;
    let mut acc = 0u64;
    let mut white = BINS - 1;
    for i in (0..BINS).rev() {
        acc += hist[i] as u64;
        if acc > target {
            white = i;
            break;
        }
    }
    let white = (white.max(1) as f32) / (BINS - 1) as f32;
    (1.0 / white).clamp(1.0, 4.0)
}

/// Linear [0,1] (times `gain`) → sRGB-encoded 8 bit.
fn output_lut(gain: f32) -> Vec<u8> {
    (0..LUT_SIZE)
        .map(|i| {
            let v = (i as f32 / (LUT_SIZE - 1) as f32 * gain).min(1.0);
            let s = if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (tone(s) * 255.0 + 0.5).clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// Default "base curve" on sRGB-encoded values, roughly matching the look
/// of in-camera JPEGs: a slight midtone lift followed by a gentle S-curve
/// (monotonic; 0 and 1 are fixed).
fn tone(s: f32) -> f32 {
    use std::f32::consts::TAU;
    const LIFT: f32 = 0.92;
    const CONTRAST: f32 = 0.45;
    let s = s.clamp(0.0, 1.0).powf(LIFT);
    (s - CONTRAST * (TAU * s).sin() / TAU).clamp(0.0, 1.0)
}

#[inline]
fn lut_at(lut: &[u8], v: f32) -> u8 {
    let i = (v.clamp(0.0, 1.0) * (LUT_SIZE - 1) as f32 + 0.5) as usize;
    lut[i.min(LUT_SIZE - 1)]
}

fn build_exif(meta: &RawlerMetadata) -> std::result::Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let mut tiff = TiffWriter::new(&mut buf).map_err(|e| e.to_string())?;
    let mut root = DirectoryWriter::new();
    let mut exif_ifd = DirectoryWriter::new();
    exif_ifd.add_tag_undefined(ExifTag::ExifVersion, b"0230".to_vec());
    meta.write_exif_tags(&mut tiff, &mut root, &mut exif_ifd)
        .map_err(|e| e.to_string())?;
    if !meta.make.is_empty() {
        root.add_tag(TiffCommonTag::Make, meta.make.as_str());
    }
    if !meta.model.is_empty() {
        root.add_tag(TiffCommonTag::Model, meta.model.as_str());
    }
    root.remove_tag(TiffCommonTag::Orientation);
    root.add_tag(TiffCommonTag::Orientation, 1_u16);
    if let Some(lens) = &meta.lens {
        if meta.exif.lens_model.is_none() {
            exif_ifd.add_tag(ExifTag::LensModel, lens.lens_model.as_str());
        }
    }
    let off = exif_ifd.build(&mut tiff).map_err(|e| e.to_string())?;
    root.add_tag(TiffCommonTag::ExifIFDPointer, off);
    tiff.build(root).map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Metadata for a RAW file: synthesised EXIF, no ICC/XMP.
pub(crate) fn metadata(f: &RawFile) -> RawMetadata {
    RawMetadata {
        exif: f.exif(),
        icc: None,
        xmp: None,
    }
}
