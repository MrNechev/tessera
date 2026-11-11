//! Colour management: embedded ICC profiles → sRGB (moxcms).

use std::sync::Arc;

use image::{DynamicImage, ImageBuffer, Pixel};
use moxcms::{
    ColorProfile, DataColorSpace, Layout, RenderingIntent, ToneReprCurve, TransformExecutor,
    TransformOptions, Xyzd,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::guard;

/// What to do with the colours of images that carry an ICC profile.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// Convert pixels with a non-sRGB embedded profile to sRGB and drop the
    /// profile (the result is plain sRGB).
    #[default]
    Srgb,
    /// Leave pixels and the embedded profile untouched.
    Keep,
}

/// Options for [`crate::load_with`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LoadOptions {
    pub color: ColorMode,
}

/// sRGB primaries as stored in ICC profiles (D50-adapted rXYZ/gXYZ/bXYZ).
const SRGB_D50: [[f64; 3]; 3] = [
    [0.4361, 0.2225, 0.0139],
    [0.3851, 0.7169, 0.0971],
    [0.1431, 0.0606, 0.7141],
];
/// Tolerance on colorant XYZ values (profiles round to s15Fixed16 and use
/// slightly different Bradford matrices).
const COLORANT_TOLERANCE: f64 = 0.004;
/// Tolerance on the linearised TRC, as a fraction of full scale. Gamma 2.2
/// "sRGB-like" curves pass (max difference ~0.004 above the toe).
const TRC_TOLERANCE: f32 = 0.012;

fn srgb_eotf(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn trc_is_srgb(trc: &Option<ToneReprCurve>) -> bool {
    let Some(trc) = trc else { return false };
    let Ok(eval) = trc.make_linear_evaluator() else {
        return false;
    };
    (1..=20).all(|i| {
        let v = i as f32 / 20.0;
        (eval.evaluate_value(v) - srgb_eotf(v)).abs() <= TRC_TOLERANCE
    })
}

fn colorant_matches(c: &Xyzd, want: &[f64; 3]) -> bool {
    (c.x - want[0]).abs() <= COLORANT_TOLERANCE
        && (c.y - want[1]).abs() <= COLORANT_TOLERANCE
        && (c.z - want[2]).abs() <= COLORANT_TOLERANCE
}

fn profile_is_srgb(p: &ColorProfile, description: Option<&str>) -> bool {
    match p.color_space {
        DataColorSpace::Rgb => {
            if p.is_matrix_shaper() {
                colorant_matches(&p.red_colorant, &SRGB_D50[0])
                    && colorant_matches(&p.green_colorant, &SRGB_D50[1])
                    && colorant_matches(&p.blue_colorant, &SRGB_D50[2])
                    && trc_is_srgb(&p.red_trc)
                    && trc_is_srgb(&p.green_trc)
                    && trc_is_srgb(&p.blue_trc)
            } else {
                // LUT-based profile: trust a clear sRGB description (e.g. the
                // ICC "sRGB v4 ICC preference" profiles).
                description.is_some_and(description_says_srgb)
            }
        }
        DataColorSpace::Gray => trc_is_srgb(&p.gray_trc),
        _ => false,
    }
}

fn description_says_srgb(d: &str) -> bool {
    let d = d.to_ascii_lowercase();
    d.contains("srgb") && !d.contains("linear") && !d.contains("scrgb")
}

/// Whether an ICC profile is (near-)sRGB: RGB matrix/TRC profiles are
/// compared on their colorants (±0.004 XYZ) and tone curves (±1.2 % over 20
/// samples), so sRGB profiles from any vendor (and gamma-2.2 "sRGB-like"
/// ones) match regardless of their description; grey profiles match on an
/// sRGB-like TRC; LUT-based RGB profiles match on an "sRGB" description.
/// Profiles moxcms can't parse are reported as sRGB when their description
/// says so, otherwise not.
pub fn icc_is_srgb(icc: &[u8]) -> bool {
    let desc = crate::metadata::icc_description(icc);
    match ColorProfile::new_from_slice(icc) {
        Ok(p) => profile_is_srgb(&p, desc.as_deref()),
        Err(_) => desc.as_deref().is_some_and(description_says_srgb),
    }
}

/// Whether [`to_srgb`] would convert pixels tagged with `icc`: a parseable,
/// usable (a transform can be built) RGB or grey profile that is not sRGB.
pub(crate) fn needs_conversion(icc: &[u8]) -> bool {
    let desc = crate::metadata::icc_description(icc);
    let Ok(p) = ColorProfile::new_from_slice(icc) else {
        return false;
    };
    if profile_is_srgb(&p, desc.as_deref()) {
        return false;
    }
    guard("colour conversion", true, || {
        Ok(match p.color_space {
            DataColorSpace::Rgb => p
                .create_transform_8bit(
                    Layout::Rgb,
                    &ColorProfile::new_srgb(),
                    Layout::Rgb,
                    options(),
                )
                .is_ok(),
            DataColorSpace::Gray => p
                .create_transform_8bit(Layout::Gray, &srgb_gray(), Layout::Gray, options())
                .is_ok(),
            _ => false,
        })
    })
    .unwrap_or(false)
}

/// Result of [`to_srgb`].
pub(crate) struct Converted {
    pub image: DynamicImage,
    /// Profile description for user messages.
    pub from: String,
}

fn options() -> TransformOptions {
    TransformOptions {
        rendering_intent: RenderingIntent::Perceptual,
        ..Default::default()
    }
}

fn srgb_gray() -> ColorProfile {
    let mut g = ColorProfile::new_gray_with_gamma(2.2);
    g.gray_trc = ColorProfile::new_srgb().red_trc;
    g
}

/// Run `t` over an interleaved buffer, row-parallel.
fn run<T: Copy + Default + Send + Sync>(
    t: &Arc<dyn TransformExecutor<T> + Send + Sync>,
    data: &mut [T],
    row_len: usize,
) -> bool {
    if row_len == 0 {
        return true;
    }
    data.par_chunks_mut(row_len).all(|row| {
        let src = row.to_vec();
        t.transform(&src, row).is_ok()
    })
}

fn convert_buffer<P>(
    mut buf: ImageBuffer<P, Vec<P::Subpixel>>,
    t: &Arc<dyn TransformExecutor<P::Subpixel> + Send + Sync>,
) -> Option<ImageBuffer<P, Vec<P::Subpixel>>>
where
    P: Pixel,
    P::Subpixel: Copy + Default + Send + Sync,
{
    let row = buf.width() as usize * P::CHANNEL_COUNT as usize;
    run(t, &mut buf, row).then_some(buf)
}

/// Convert `img` from the embedded profile `icc` to sRGB. Returns `None`
/// when no conversion is needed or possible (profile already sRGB,
/// unparseable, or not matching the pixel layout, e.g. CMYK). 8- and 16-bit
/// RGB(A)/grey use moxcms' integer (fixed-point) paths; float images the f32
/// path. Alpha is preserved.
pub(crate) fn to_srgb(img: &DynamicImage, icc: &[u8]) -> Option<Converted> {
    let desc = crate::metadata::icc_description(icc);
    let src = ColorProfile::new_from_slice(icc).ok()?;
    if profile_is_srgb(&src, desc.as_deref()) {
        return None;
    }
    let from = desc.unwrap_or_else(|| "the embedded colour profile".to_string());
    let rgb_src = src.color_space == DataColorSpace::Rgb;
    let gray_src = src.color_space == DataColorSpace::Gray;
    let srgb = ColorProfile::new_srgb();
    let out = guard(
        "colour conversion",
        true,
        || -> crate::Result<Option<DynamicImage>> {
            let o = options();
            let res = match img {
                DynamicImage::ImageRgb8(b) if rgb_src => {
                    let t = src
                        .create_transform_8bit(Layout::Rgb, &srgb, Layout::Rgb, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgb8)
                }
                DynamicImage::ImageRgba8(b) if rgb_src => {
                    let t = src
                        .create_transform_8bit(Layout::Rgba, &srgb, Layout::Rgba, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgba8)
                }
                DynamicImage::ImageRgb16(b) if rgb_src => {
                    let t = src
                        .create_transform_16bit(Layout::Rgb, &srgb, Layout::Rgb, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgb16)
                }
                DynamicImage::ImageRgba16(b) if rgb_src => {
                    let t = src
                        .create_transform_16bit(Layout::Rgba, &srgb, Layout::Rgba, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgba16)
                }
                DynamicImage::ImageRgb32F(b) if rgb_src => {
                    let t = src
                        .create_transform_f32(Layout::Rgb, &srgb, Layout::Rgb, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgb32F)
                }
                DynamicImage::ImageRgba32F(b) if rgb_src => {
                    let t = src
                        .create_transform_f32(Layout::Rgba, &srgb, Layout::Rgba, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageRgba32F)
                }
                DynamicImage::ImageLuma8(b) if gray_src => {
                    let g = srgb_gray();
                    let t = src
                        .create_transform_8bit(Layout::Gray, &g, Layout::Gray, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageLuma8)
                }
                DynamicImage::ImageLumaA8(b) if gray_src => {
                    let g = srgb_gray();
                    let t = src
                        .create_transform_8bit(Layout::GrayAlpha, &g, Layout::GrayAlpha, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageLumaA8)
                }
                DynamicImage::ImageLuma16(b) if gray_src => {
                    let g = srgb_gray();
                    let t = src
                        .create_transform_16bit(Layout::Gray, &g, Layout::Gray, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageLuma16)
                }
                DynamicImage::ImageLumaA16(b) if gray_src => {
                    let g = srgb_gray();
                    let t = src
                        .create_transform_16bit(Layout::GrayAlpha, &g, Layout::GrayAlpha, o)
                        .ok();
                    t.and_then(|t| convert_buffer(b.clone(), &t))
                        .map(DynamicImage::ImageLumaA16)
                }
                _ => None,
            };
            Ok(res)
        },
    )
    .ok()
    .flatten()?;
    Some(Converted { image: out, from })
}
