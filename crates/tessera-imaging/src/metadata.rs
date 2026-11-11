//! Metadata policy, EXIF filtering, embedding and summaries.

use std::borrow::Cow;
use std::path::Path;

use exif::{In, Tag, Value};
use serde::{Deserialize, Serialize};

use crate::container::{jpeg, png, webp};
use crate::decode::scan;
use crate::error::{Error, Result};
use crate::exif_util::{self, Group, GroupMask};
use crate::format::ImageFormat;

/// How metadata is carried over to output files.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum MetadataMode {
    /// Copy EXIF (byte-for-byte, with Orientation reset), ICC and XMP.
    #[default]
    KeepAll,
    /// Write no metadata at all (ICC included).
    StripAll,
    /// Keep the groups selected in [`MetadataPolicy`].
    Custom,
}

/// Metadata policy. The `keep_*` flags only matter in `Custom` mode, except
/// `keep_file_times` which applies to `KeepAll` and `Custom`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataPolicy {
    pub mode: MetadataMode,
    /// DateTime, DateTimeOriginal/Digitized, offsets, sub-seconds.
    pub keep_date_time: bool,
    /// Make, model, lens, software, serial numbers.
    pub keep_camera: bool,
    /// Exposure, ISO, aperture, focal length, flash, white balance, ...
    pub keep_exposure: bool,
    /// GPS IFD.
    pub keep_location: bool,
    /// Artist, Copyright, ImageDescription, UserComment, XP* tags, rating.
    pub keep_author: bool,
    /// ICC colour profile.
    pub keep_icc: bool,
    /// Copy access/modification times from the source file.
    pub keep_file_times: bool,
}

impl Default for MetadataPolicy {
    fn default() -> Self {
        MetadataPolicy {
            mode: MetadataMode::KeepAll,
            keep_date_time: true,
            keep_camera: true,
            keep_exposure: true,
            keep_location: true,
            keep_author: true,
            keep_icc: true,
            keep_file_times: true,
        }
    }
}

impl MetadataPolicy {
    /// Keep everything.
    pub fn keep_all() -> Self {
        Self::default()
    }

    /// Strip everything (including ICC and file times).
    pub fn strip_all() -> Self {
        MetadataPolicy {
            mode: MetadataMode::StripAll,
            keep_date_time: false,
            keep_camera: false,
            keep_exposure: false,
            keep_location: false,
            keep_author: false,
            keep_icc: false,
            keep_file_times: false,
        }
    }

    pub fn keeps_icc(&self) -> bool {
        match self.mode {
            MetadataMode::KeepAll => true,
            MetadataMode::StripAll => false,
            MetadataMode::Custom => self.keep_icc,
        }
    }

    /// XMP can contain anything (including location), so it is only kept in
    /// `KeepAll` mode.
    pub fn keeps_xmp(&self) -> bool {
        self.mode == MetadataMode::KeepAll
    }

    pub fn keeps_file_times(&self) -> bool {
        self.keep_file_times && self.mode != MetadataMode::StripAll
    }

    pub(crate) fn mask(&self) -> GroupMask {
        GroupMask {
            date_time: self.keep_date_time,
            camera: self.keep_camera,
            exposure: self.keep_exposure,
            location: self.keep_location,
            author: self.keep_author,
        }
    }
}

/// Filter an EXIF block (bare TIFF structure) according to `policy`, for
/// pixels that have been rotated upright: Orientation is always 1.
///
/// * `KeepAll`: byte copy with Orientation patched in place (MakerNote and
///   thumbnail preserved).
/// * `Custom`: rebuilt with only the selected groups; the thumbnail (IFD1),
///   MakerNote, unknown tags and PixelX/YDimension are dropped.
/// * `StripAll`: `None`.
pub fn filter_exif(exif_tiff: &[u8], policy: &MetadataPolicy) -> Option<Vec<u8>> {
    filter_exif_inner(exif_tiff, policy, 1, None)
}

/// Like [`filter_exif`] but also records the new pixel size in
/// PixelXDimension/PixelYDimension.
pub fn filter_exif_sized(
    exif_tiff: &[u8],
    policy: &MetadataPolicy,
    width: u32,
    height: u32,
) -> Option<Vec<u8>> {
    filter_exif_inner(exif_tiff, policy, 1, Some((width, height)))
}

/// Core filter. `orientation` is the value to record (1 when pixels were
/// rotated; the original value when they were not). When the policy would
/// drop everything but `orientation != 1`, a minimal block holding only the
/// orientation is returned so the image still displays upright.
pub(crate) fn filter_exif_inner(
    exif_tiff: &[u8],
    policy: &MetadataPolicy,
    orientation: u16,
    dims: Option<(u32, u32)>,
) -> Option<Vec<u8>> {
    let valid = exif_tiff.starts_with(b"II*\0") || exif_tiff.starts_with(b"MM\0*");
    let result = if !valid {
        None
    } else {
        match policy.mode {
            MetadataMode::StripAll => None,
            MetadataMode::KeepAll => {
                let mut v = exif_tiff.to_vec();
                let current = exif_util::read_orientation(&v);
                exif_util::patch_in_place(&mut v, Some(orientation), dims);
                if current == 1
                    && orientation != 1
                    && exif_util::read_orientation(&v) != orientation
                {
                    // Tag absent: rebuild to add it.
                    exif_util::parse(exif_tiff)
                        .and_then(|e| exif_util::rebuild(&e, GroupMask::ALL, orientation, dims))
                        .or(Some(v))
                } else {
                    Some(v)
                }
            }
            MetadataMode::Custom => exif_util::parse(exif_tiff)
                .and_then(|e| exif_util::rebuild(&e, policy.mask(), orientation, dims)),
        }
    };
    if result.is_none() && orientation != 1 {
        return minimal_orientation_exif(orientation);
    }
    result
}

pub(crate) fn minimal_orientation_exif(orientation: u16) -> Option<Vec<u8>> {
    let f = exif::Field {
        tag: Tag::Orientation,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![orientation]),
    };
    exif_util::write_fields(&[f], false)
}

/// EXIF/ICC/XMP to write for an output, given a policy.
#[derive(Clone, Debug, Default)]
pub(crate) struct Planned {
    pub exif: Option<Vec<u8>>,
    pub icc: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

/// Plan metadata for re-encoded (upright) pixels of size `dims`.
pub(crate) fn plan(
    src: &crate::decode::RawMetadata,
    policy: &MetadataPolicy,
    dims: (u32, u32),
) -> Planned {
    Planned {
        exif: src
            .exif
            .as_deref()
            .and_then(|e| filter_exif_inner(e, policy, 1, Some(dims))),
        icc: src.icc.clone().filter(|_| policy.keeps_icc()),
        xmp: src
            .xmp
            .as_deref()
            .filter(|_| policy.keeps_xmp())
            .map(exif_util::xmp_reset_orientation),
    }
}

/// Insert metadata into already-encoded bytes, replacing any EXIF/ICC/XMP
/// already present (`None` removes). Returns the new bytes and user-facing
/// warnings for anything that could not be written.
///
/// JPEG: APP1 Exif / APP1 XMP / APP2 ICC. PNG: eXIf / iCCP / iTXt XMP.
/// WebP: VP8X + ICCP/EXIF/"XMP " chunks. TIFF: rewritten with Exif/GPS
/// sub-IFDs, ICC (tag 34675) and XMP (tag 700). Other formats: unchanged,
/// with a warning when metadata was requested.
pub fn embed(
    encoded: Vec<u8>,
    format: ImageFormat,
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Result<(Vec<u8>, Vec<String>)> {
    let mut warnings = Vec::new();
    let exif = exif.filter(|e| e.starts_with(b"II*\0") || e.starts_with(b"MM\0*"));
    let any = exif.is_some() || icc.is_some() || xmp.is_some();
    let out = match format {
        ImageFormat::Jpeg => {
            let parsed = jpeg::parse(&encoded)?;
            let exif: Option<Cow<[u8]>> = match exif {
                Some(e) if jpeg::exif_fits(e) => Some(Cow::Borrowed(e)),
                Some(e) => match shrink_exif(e) {
                    Some(s) if jpeg::exif_fits(&s) => {
                        warnings.push(
                            "EXIF data was too large for JPEG; the embedded thumbnail and maker notes were removed"
                                .to_string(),
                        );
                        Some(Cow::Owned(s))
                    }
                    _ => {
                        warnings.push(
                            "EXIF data is too large for JPEG and was not written".to_string(),
                        );
                        None
                    }
                },
                None => None,
            };
            let xmp = match xmp {
                Some(x) if !jpeg::xmp_fits(x) => {
                    warnings.push("XMP data is too large for JPEG and was not written".to_string());
                    None
                }
                other => other,
            };
            let icc = match icc {
                Some(i) if i.len() > 255 * 65519 => {
                    warnings
                        .push("ICC profile is too large for JPEG and was not written".to_string());
                    None
                }
                other => other,
            };
            jpeg::rewrite(
                &parsed,
                exif.as_deref(),
                icc,
                xmp,
                jpeg::Keep { other: true },
            )?
        }
        ImageFormat::Png => png::rewrite(
            &encoded,
            &png::Rewrite {
                exif,
                icc,
                xmp,
                keep_other: true,
                dpi: None,
            },
        )?,
        ImageFormat::Webp => webp::rewrite(&encoded, exif, icc, xmp)?,
        ImageFormat::Tiff => crate::tiff_out::rewrite(&encoded, exif, icc, xmp)?,
        ImageFormat::Avif => {
            if any {
                warnings.push(
                    "AVIF: metadata (EXIF/ICC/XMP) is not written by this version".to_string(),
                );
            }
            encoded
        }
        other => {
            if any {
                warnings.push(format!(
                    "{}: metadata is not supported and was not written",
                    other.label()
                ));
            }
            encoded
        }
    };
    Ok((out, warnings))
}

fn shrink_exif(exif: &[u8]) -> Option<Vec<u8>> {
    let o = exif_util::read_orientation(exif);
    let parsed = exif_util::parse(exif)?;
    exif_util::rebuild(&parsed, GroupMask::ALL, o, None)
}

// ---------------------------------------------------------------------------
// Summary

/// One EXIF field for display.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MetadataField {
    /// "date", "camera", "exposure", "location", "author", "technical" or "other".
    pub group: String,
    /// "primary" or "thumbnail".
    pub ifd: String,
    pub tag: String,
    pub value: String,
}

/// Human-oriented summary of a file's metadata.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetadataSummary {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub software: Option<String>,
    /// "YYYY-MM-DDTHH:MM:SS"
    pub date_taken: Option<String>,
    pub date_digitized: Option<String>,
    pub date_modified: Option<String>,
    /// e.g. "1/125 s"
    pub exposure_time: Option<String>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    /// Millimetres.
    pub focal_length: Option<f64>,
    pub focal_length_35mm: Option<u32>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
    pub artist: Option<String>,
    pub copyright: Option<String>,
    pub description: Option<String>,
    pub orientation: u16,
    pub has_exif: bool,
    pub has_icc: bool,
    pub has_xmp: bool,
    pub icc_description: Option<String>,
    /// The colours are sRGB: no ICC profile (untagged images are treated as
    /// sRGB) or a (near-)sRGB one (see [`crate::icc_is_srgb`]).
    #[serde(default)]
    pub icc_is_srgb: bool,
    /// The file is a camera RAW (`format` is [`ImageFormat::Raw`]).
    #[serde(default)]
    pub raw: bool,
    /// Upright pixel size.
    pub width: u32,
    pub height: u32,
    pub format: Option<ImageFormat>,
    pub file_size: u64,
    /// All readable EXIF fields (MakerNote excluded), for a details view.
    pub fields: Vec<MetadataField>,
}

/// Read a metadata summary without decoding pixels (where possible).
pub fn read_summary(path: &Path) -> Result<MetadataSummary> {
    let s = scan(path)?;
    let mut sum = MetadataSummary {
        orientation: s.orientation,
        has_exif: s.metadata.exif.is_some(),
        has_icc: s.metadata.icc.is_some(),
        has_xmp: s.metadata.xmp.is_some(),
        icc_description: s.metadata.icc.as_deref().and_then(icc_description),
        icc_is_srgb: s
            .metadata
            .icc
            .as_deref()
            .is_none_or(crate::color::icc_is_srgb),
        raw: s.format == ImageFormat::Raw,
        width: s.width,
        height: s.height,
        format: Some(s.format),
        file_size: s.file_size,
        ..Default::default()
    };
    if let Some(e) = s.metadata.exif.as_deref().and_then(exif_util::parse) {
        fill_summary(&mut sum, &e);
    }
    Ok(sum)
}

/// Summary from a raw EXIF block (bare TIFF).
pub fn summarize_exif(exif_tiff: &[u8]) -> Result<MetadataSummary> {
    let e = exif_util::parse(exif_tiff)
        .ok_or_else(|| Error::Metadata("could not parse EXIF".into()))?;
    let mut sum = MetadataSummary {
        has_exif: true,
        orientation: exif_util::read_orientation(exif_tiff),
        ..Default::default()
    };
    fill_summary(&mut sum, &e);
    Ok(sum)
}

fn fill_summary(sum: &mut MetadataSummary, e: &exif::Exif) {
    use exif_util::{ascii, rational, raw_ascii, uint};
    sum.camera_make = ascii(e, Tag::Make);
    sum.camera_model = ascii(e, Tag::Model);
    sum.lens = ascii(e, Tag::LensModel).or_else(|| ascii(e, Tag::LensMake));
    sum.software = ascii(e, Tag::Software);
    let date = |t: Tag| raw_ascii(e, t).and_then(|r| exif_util::exif_datetime_to_iso(&r));
    sum.date_taken = date(Tag::DateTimeOriginal);
    sum.date_digitized = date(Tag::DateTimeDigitized);
    sum.date_modified = date(Tag::DateTime);
    sum.exposure_time = e
        .get_field(Tag::ExposureTime, In::PRIMARY)
        .and_then(|f| match &f.value {
            Value::Rational(v) => v.first().copied(),
            _ => None,
        })
        .and_then(|r| format_exposure(r.num, r.denom));
    sum.f_number = rational(e, Tag::FNumber).filter(|v| *v > 0.0);
    sum.iso = uint(e, Tag::PhotographicSensitivity).filter(|v| *v > 0);
    sum.focal_length = rational(e, Tag::FocalLength).filter(|v| *v > 0.0);
    sum.focal_length_35mm = uint(e, Tag::FocalLengthIn35mmFilm).filter(|v| *v > 0);
    sum.gps_latitude = exif_util::gps_coord(e, Tag::GPSLatitude, Tag::GPSLatitudeRef);
    sum.gps_longitude = exif_util::gps_coord(e, Tag::GPSLongitude, Tag::GPSLongitudeRef);
    sum.gps_altitude = rational(e, Tag::GPSAltitude).map(|a| {
        let below = uint(e, Tag::GPSAltitudeRef) == Some(1);
        if below {
            -a
        } else {
            a
        }
    });
    sum.artist = ascii(e, Tag::Artist);
    sum.copyright = ascii(e, Tag::Copyright);
    sum.description = ascii(e, Tag::ImageDescription);
    for f in e.fields() {
        if f.tag == Tag::MakerNote {
            continue;
        }
        let group = match exif_util::classify(f.tag) {
            Group::DateTime => "date",
            Group::Camera => "camera",
            Group::Exposure => "exposure",
            Group::Location => "location",
            Group::Author => "author",
            Group::Structural => "technical",
            Group::Drop => "other",
        };
        let mut value = f.display_value().with_unit(e).to_string();
        if value.len() > 256 {
            let mut cut = 256;
            while !value.is_char_boundary(cut) {
                cut -= 1;
            }
            value.truncate(cut);
            value.push('…');
        }
        sum.fields.push(MetadataField {
            group: group.to_string(),
            ifd: if f.ifd_num == In::PRIMARY {
                "primary"
            } else {
                "thumbnail"
            }
            .to_string(),
            tag: f.tag.to_string(),
            value,
        });
    }
}

fn format_exposure(num: u32, denom: u32) -> Option<String> {
    if num == 0 || denom == 0 {
        return None;
    }
    let v = num as f64 / denom as f64;
    Some(if v >= 1.0 {
        let s = format!("{v:.1}");
        format!("{} s", s.trim_end_matches('0').trim_end_matches('.'))
    } else {
        format!("1/{} s", (denom as f64 / num as f64).round() as u64)
    })
}

/// Description of an ICC profile ('desc' tag, v2 text or v4 mluc).
pub fn icc_description(icc: &[u8]) -> Option<String> {
    if icc.len() < 132 || &icc[36..40] != b"acsp" {
        return None;
    }
    let be32 = |p: usize| -> Option<u32> {
        let b = icc.get(p..p + 4)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let count = be32(128)? as usize;
    for i in 0..count.min(256) {
        let base = 132 + i * 12;
        if icc.get(base..base + 4)? != b"desc" {
            continue;
        }
        let off = be32(base + 4)? as usize;
        let len = be32(base + 8)? as usize;
        let data = icc.get(off..off.checked_add(len)?)?;
        let text = match data.get(0..4)? {
            b"desc" => {
                let n = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
                let s = data.get(12..12 + n.min(data.len().saturating_sub(12)))?;
                String::from_utf8_lossy(s)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string()
            }
            b"mluc" => {
                let n = u32::from_be_bytes(data.get(8..12)?.try_into().ok()?) as usize;
                if n == 0 {
                    return None;
                }
                let rec = data.get(16..28)?;
                let slen = u32::from_be_bytes(rec[4..8].try_into().ok()?) as usize;
                let soff = u32::from_be_bytes(rec[8..12].try_into().ok()?) as usize;
                let s = data.get(soff..soff.checked_add(slen)?)?;
                let units: Vec<u16> = s
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&c| u16::from_be_bytes(c))
                    .collect();
                String::from_utf16_lossy(&units)
                    .trim_end_matches('\0')
                    .trim()
                    .to_string()
            }
            _ => return None,
        };
        return (!text.is_empty()).then_some(text);
    }
    None
}
