//! Downscaling (never upscaling) with Lanczos3 via `fast_image_resize`.

use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

/// How to resize an image before encoding. Images are never upscaled.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ResizeMode {
    #[default]
    None,
    /// Scale both sides by `percent` (e.g. 50.0 = half size).
    Percent { percent: f32 },
    /// Fit inside a `width` x `height` box, keeping the aspect ratio. A zero
    /// side means "unconstrained".
    Fit { width: u32, height: u32 },
    /// Limit the longest edge to `pixels`.
    LongEdge { pixels: u32 },
}

/// Fit (w, h) inside (max_w, max_h) keeping aspect ratio; never upscales;
/// zero box sides are unconstrained. Result sides are at least 1.
pub fn fit_dimensions(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (w, h);
    }
    let sx = if max_w == 0 {
        f64::INFINITY
    } else {
        max_w as f64 / w as f64
    };
    let sy = if max_h == 0 {
        f64::INFINITY
    } else {
        max_h as f64 / h as f64
    };
    let s = sx.min(sy);
    if !s.is_finite() || s >= 1.0 {
        return (w, h);
    }
    scaled(w, h, s)
}

fn scaled(w: u32, h: u32, s: f64) -> (u32, u32) {
    let nw = ((w as f64 * s).round() as u32).clamp(1, w);
    let nh = ((h as f64 * s).round() as u32).clamp(1, h);
    (nw, nh)
}

/// Dimensions an image of size (w, h) will have after `mode`.
pub fn target_dimensions(w: u32, h: u32, mode: &ResizeMode) -> (u32, u32) {
    match *mode {
        ResizeMode::None => (w, h),
        ResizeMode::Percent { percent } => {
            if !percent.is_finite() || percent <= 0.0 || percent >= 100.0 {
                (w, h)
            } else {
                scaled(w, h, percent as f64 / 100.0)
            }
        }
        ResizeMode::Fit { width, height } => fit_dimensions(w, h, width, height),
        ResizeMode::LongEdge { pixels } => {
            if pixels == 0 {
                (w, h)
            } else {
                fit_dimensions(w, h, pixels, pixels)
            }
        }
    }
}

/// Apply `mode` (never upscales; Lanczos3).
pub fn apply_resize(img: DynamicImage, mode: &ResizeMode) -> DynamicImage {
    let (w, h) = target_dimensions(img.width(), img.height(), mode);
    resize_exact(img, w, h)
}

/// Resize to exactly (w, h) with Lanczos3. Returns the input unchanged when
/// the size already matches.
pub(crate) fn resize_exact(img: DynamicImage, w: u32, h: u32) -> DynamicImage {
    if (img.width(), img.height()) == (w, h) || w == 0 || h == 0 {
        return img;
    }
    // fast_image_resize handles all 8/16-bit and f32 layouts the image crate
    // produces; anything else is normalised first.
    let src = match img {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_)
        | DynamicImage::ImageRgb32F(_)
        | DynamicImage::ImageRgba32F(_) => img,
        other => DynamicImage::ImageRgba8(other.to_rgba8()),
    };
    let mut dst = DynamicImage::new(w, h, src.color());
    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    let mut resizer = Resizer::new();
    match resizer.resize(&src, &mut dst, &opts) {
        Ok(()) => dst,
        // Should not happen (same pixel types); fall back to the image crate.
        Err(_) => src.resize_exact(w, h, image::imageops::FilterType::Lanczos3),
    }
}
