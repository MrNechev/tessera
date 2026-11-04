//! Perceptual quality search ("smart" compression) and target-size search.

use image::{DynamicImage, GenericImageView};
use rayon::prelude::*;

use crate::decode::load_bytes;
use crate::encode::{self, Chroma};
use crate::error::Result;
use crate::format::ImageFormat;
use crate::quality::{luma_plane, ssim_planes};
use crate::resize::resize_exact;

/// Search parameters for one smart level.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Level {
    /// Minimum luma SSIM (8x8 windows, full-resolution tiles).
    pub threshold: f64,
    pub jpeg: (u8, u8),
    pub webp: (u8, u8),
    pub avif: (u8, u8),
    pub jpeg_chroma: Chroma,
    /// imagequant quality target for the PNG palette candidate.
    pub png_quant: u8,
}

/// Calibrated on photographic content; see the crate README for numbers.
pub(crate) const VISUALLY_LOSSLESS: Level = Level {
    threshold: 0.980,
    jpeg: (85, 98),
    webp: (85, 100),
    avif: (75, 98),
    jpeg_chroma: Chroma::S444,
    png_quant: 98,
};

pub(crate) const BALANCED: Level = Level {
    threshold: 0.965,
    jpeg: (60, 95),
    webp: (65, 95),
    avif: (50, 92),
    jpeg_chroma: Chroma::S420,
    png_quant: 90,
};

pub(crate) const SMALL: Level = Level {
    threshold: 0.945,
    jpeg: (40, 90),
    webp: (45, 90),
    avif: (35, 85),
    jpeg_chroma: Chroma::S420,
    png_quant: 75,
};

impl Level {
    pub fn range(&self, f: ImageFormat) -> (u8, u8) {
        match f {
            ImageFormat::Webp => self.webp,
            ImageFormat::Avif => self.avif,
            _ => self.jpeg,
        }
    }
}

/// AVIF speed used while searching (final encodes use the regular effort).
const AVIF_SEARCH_SPEED: u8 = 8;
const DEFAULT_EFFORT: u8 = 60;
const TILE: u32 = 512;
/// Images up to this many pixels are evaluated whole.
const WHOLE_IMAGE_PIXELS: u64 = 1_100_000;

/// Encode with a lossy codec at quality `q`.
pub(crate) fn encode_lossy(
    img: &DynamicImage,
    f: ImageFormat,
    q: u8,
    chroma: Chroma,
    searching: bool,
    bg: [u8; 3],
) -> Result<Vec<u8>> {
    match f {
        ImageFormat::Jpeg => {
            encode::encode_jpeg(img, q, chroma, !searching, DEFAULT_EFFORT, None, bg)
        }
        ImageFormat::Webp => encode::encode_webp(img, false, q, DEFAULT_EFFORT),
        ImageFormat::Avif => encode::encode_avif(
            img,
            q,
            if searching {
                AVIF_SEARCH_SPEED
            } else {
                encode::avif_speed(DEFAULT_EFFORT)
            },
        ),
        other => Err(crate::Error::Unsupported(format!(
            "{} has no lossy quality setting",
            other.label()
        ))),
    }
}

/// Number of tiles evaluated for large images.
const TILE_COUNT: usize = 6;

/// Pick up to six 512x512 tiles spread over the distribution of local edge
/// energy (from the busiest to fairly smooth areas): busy areas show ringing
/// first, smooth gradients show blocking/banding and loss of fine grain.
pub(crate) fn select_tiles(img: &DynamicImage) -> Vec<DynamicImage> {
    let (w, h) = img.dimensions();
    if (w as u64) * (h as u64) <= WHOLE_IMAGE_PIXELS {
        return vec![img.clone()];
    }
    let tw = TILE.min(w);
    let th = TILE.min(h);
    let cols = (w / tw).max(1);
    let rows = (h / th).max(1);
    let luma = img.to_luma8();
    let mut cells: Vec<(u64, u32, u32)> = Vec::with_capacity((cols * rows) as usize);
    for cy in 0..rows {
        for cx in 0..cols {
            // Spread the grid over the whole image, aligned to 16 px (JPEG MCU).
            let x = if cols > 1 {
                (cx * (w - tw) / (cols - 1)) & !15
            } else {
                0
            };
            let y = if rows > 1 {
                (cy * (h - th) / (rows - 1)) & !15
            } else {
                0
            };
            let mut e = 0u64;
            let mut yy = y;
            while yy + 1 < y + th {
                let mut xx = x;
                while xx + 1 < x + tw {
                    let p = luma.get_pixel(xx, yy).0[0] as i32;
                    let r = luma.get_pixel(xx + 1, yy).0[0] as i32;
                    let d = luma.get_pixel(xx, yy + 1).0[0] as i32;
                    e += ((p - r).abs() + (p - d).abs()) as u64;
                    xx += 2;
                }
                yy += 2;
            }
            cells.push((e, x, y));
        }
    }
    cells.sort_by_key(|c| std::cmp::Reverse(c.0));
    let n = cells.len();
    let mut picks: Vec<(u32, u32)> = Vec::new();
    if n <= TILE_COUNT {
        picks.extend(cells.iter().map(|c| (c.1, c.2)));
    } else {
        // Quantiles 0%, 15%, 30%, 45%, 60%, 75% of the energy ranking: skips
        // the flattest quarter (sky, walls), where every codec does well.
        for i in 0..TILE_COUNT {
            let idx = (i * (n - 1) * 15 / 100).min(n - 1);
            let c = cells[idx];
            if !picks.contains(&(c.1, c.2)) {
                picks.push((c.1, c.2));
            }
        }
    }
    picks
        .into_iter()
        .map(|(x, y)| img.crop_imm(x, y, tw, th))
        .collect()
}

/// Mean SSIM over tiles for quality `q`.
fn tiles_ssim(
    tiles: &[DynamicImage],
    refs: &[(Vec<u8>, usize, usize)],
    f: ImageFormat,
    q: u8,
    chroma: Chroma,
    bg: [u8; 3],
) -> Result<f64> {
    let scores: Vec<(f64, f64)> = tiles
        .par_iter()
        .zip(refs.par_iter())
        .map(|(t, (rl, w, h))| -> Result<(f64, f64)> {
            let enc = encode_lossy(t, f, q, chroma, true, bg)?;
            let dec = load_bytes(&enc, Some(f))?.image.to_rgba8();
            let weight = (*w * *h) as f64;
            if dec.dimensions() != (*w as u32, *h as u32) {
                return Ok((0.0, weight));
            }
            Ok((ssim_planes(rl, &luma_plane(&dec), *w, *h) * weight, weight))
        })
        .collect::<Result<Vec<_>>>()?;
    let total: f64 = scores.iter().map(|s| s.1).sum();
    Ok(scores.iter().map(|s| s.0).sum::<f64>() / total.max(1.0))
}

/// Tile-based SSIM of `reference` encoded as `f` at quality `q` (the metric
/// the smart search uses). Exposed for calibration tooling.
pub(crate) fn search_metric(
    reference: &DynamicImage,
    f: ImageFormat,
    q: u8,
    chroma: Chroma,
) -> Result<f64> {
    let tiles = select_tiles(reference);
    let refs: Vec<(Vec<u8>, usize, usize)> = tiles
        .iter()
        .map(|t| {
            let rgba = t.to_rgba8();
            (
                luma_plane(&rgba),
                rgba.width() as usize,
                rgba.height() as usize,
            )
        })
        .collect();
    tiles_ssim(&tiles, &refs, f, q, chroma, [255, 255, 255])
}

/// Outcome of a smart search.
pub(crate) struct SmartResult {
    pub bytes: Vec<u8>,
    pub quality: u8,
    /// SSIM of the full encoded image against the reference (as reported
    /// by [`crate::ssim`]).
    pub ssim: Option<f64>,
}

/// Find the lowest quality whose (tile) SSIM reaches `level.threshold`, then
/// encode the full image and verify it, nudging the quality up if the full
/// image falls short. `reference` must already be flattened when the target
/// has no alpha.
pub(crate) fn smart_lossy(
    reference: &DynamicImage,
    f: ImageFormat,
    level: &Level,
    bg: [u8; 3],
) -> Result<SmartResult> {
    let chroma = if f == ImageFormat::Jpeg {
        level.jpeg_chroma
    } else {
        Chroma::Auto
    };
    let tiles = select_tiles(reference);
    let refs: Vec<(Vec<u8>, usize, usize)> = tiles
        .iter()
        .map(|t| {
            let rgba = t.to_rgba8();
            (
                luma_plane(&rgba),
                rgba.width() as usize,
                rgba.height() as usize,
            )
        })
        .collect();
    let (lo, hi) = level.range(f);
    let mut cache: Vec<(u8, f64)> = Vec::new();
    let mut eval = |q: u8| -> Result<f64> {
        if let Some(&(_, s)) = cache.iter().find(|(cq, _)| *cq == q) {
            return Ok(s);
        }
        let s = tiles_ssim(&tiles, &refs, f, q, chroma, bg)?;
        cache.push((q, s));
        Ok(s)
    };
    let mut quality = if eval(hi)? < level.threshold {
        hi
    } else {
        let (mut a, mut b) = (lo, hi);
        while a < b {
            let m = a + (b - a) / 2;
            if eval(m)? >= level.threshold {
                b = m;
            } else {
                a = m + 1;
            }
        }
        b
    };
    let reference_rgba = reference.to_rgba8();
    let mut bytes = encode_lossy(reference, f, quality, chroma, false, bg)?;
    let mut score = measure(&reference_rgba, &bytes, f);
    // Verify on the full image (cheap compared to the search).
    for _ in 0..4 {
        match score {
            Some(s) if s < level.threshold && quality < hi => {
                quality = (quality + 2).min(hi);
                bytes = encode_lossy(reference, f, quality, chroma, false, bg)?;
                score = measure(&reference_rgba, &bytes, f);
            }
            _ => break,
        }
    }
    Ok(SmartResult {
        bytes,
        quality,
        ssim: score,
    })
}

fn measure(reference: &image::RgbaImage, encoded: &[u8], f: ImageFormat) -> Option<f64> {
    let dec = load_bytes(encoded, Some(f)).ok()?.image.to_rgba8();
    (dec.dimensions() == reference.dimensions()).then(|| crate::quality::ssim(reference, &dec))
}

/// Outcome of a target-size search.
pub(crate) struct SizedResult {
    pub bytes: Vec<u8>,
    pub quality: Option<u8>,
    /// The image that was encoded (possibly downscaled).
    pub image: DynamicImage,
    pub reached: bool,
}

const MIN_TARGET_QUALITY: u8 = 40;
const MAX_TARGET_QUALITY: u8 = 95;
const MAX_DOWNSCALE_STEPS: u32 = 40;

/// Encode `img` as `f` so that the result is at most `budget` bytes:
/// highest quality in 40..=95 that fits, downscaling by 10% steps when even
/// quality 40 is too large.
pub(crate) fn fit_size(
    img: &DynamicImage,
    f: ImageFormat,
    budget: u64,
    bg: [u8; 3],
) -> Result<SizedResult> {
    let fits = |b: &[u8]| b.len() as u64 <= budget;
    let encode_at = |im: &DynamicImage, q: Option<u8>, searching: bool| -> Result<Vec<u8>> {
        match f {
            ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Avif => encode_lossy(
                im,
                f,
                q.unwrap_or(MAX_TARGET_QUALITY),
                Chroma::Auto,
                searching,
                bg,
            ),
            ImageFormat::Png => match q {
                None => encode::png_lossless(im, DEFAULT_EFFORT, None),
                Some(q) => {
                    match encode::png_quantized(&im.to_rgba8(), 0, q, 1.0, DEFAULT_EFFORT, None)? {
                        Some(v) => Ok(v),
                        None => encode::png_lossless(im, DEFAULT_EFFORT, None),
                    }
                }
            },
            _ => {
                let opts = encode::EncodeOptions {
                    format: f,
                    quality: q.unwrap_or(100),
                    lossless: q.is_none(),
                    background: bg,
                    ..Default::default()
                };
                encode::encode(im, &opts)
            }
        }
    };
    let lossy = matches!(f, ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Avif);
    // Quality ladder for formats without a continuous quality scale.
    let ladder: Vec<Option<u8>> = match f {
        ImageFormat::Png => vec![None, Some(90), Some(75), Some(60), Some(40)],
        ImageFormat::Gif => vec![Some(100), Some(75), Some(50), Some(30)],
        _ => vec![None],
    };
    let (w0, h0) = img.dimensions();
    let mut scale = 1.0f64;
    let mut cur = img.clone();
    let mut last: Option<(Vec<u8>, Option<u8>)> = None;
    for step in 0..=MAX_DOWNSCALE_STEPS {
        if step > 0 {
            scale *= 0.9;
            let w = ((w0 as f64 * scale).round() as u32).max(1);
            let h = ((h0 as f64 * scale).round() as u32).max(1);
            if w < 16 || h < 16 {
                break;
            }
            cur = resize_exact(img.clone(), w, h);
        }
        if lossy {
            let floor = encode_at(&cur, Some(MIN_TARGET_QUALITY), true)?;
            if !fits(&floor) {
                last = Some((floor, Some(MIN_TARGET_QUALITY)));
                continue;
            }
            // Largest quality that fits.
            let (mut lo, mut hi) = (MIN_TARGET_QUALITY, MAX_TARGET_QUALITY);
            while lo < hi {
                let m = lo + (hi - lo).div_ceil(2);
                if fits(&encode_at(&cur, Some(m), true)?) {
                    lo = m;
                } else {
                    hi = m - 1;
                }
            }
            let mut bytes = encode_at(&cur, Some(lo), false)?;
            if !fits(&bytes) {
                // The final (slower) settings can differ slightly from the search.
                bytes = encode_at(&cur, Some(lo), true)?;
            }
            if !fits(&bytes) && lo > MIN_TARGET_QUALITY {
                bytes = floor;
            }
            let q = if fits(&bytes) {
                Some(lo)
            } else {
                Some(MIN_TARGET_QUALITY)
            };
            return Ok(SizedResult {
                bytes,
                quality: q,
                image: cur,
                reached: true,
            });
        }
        // Ladder formats: at full size try every rung, later only the last.
        let rungs: &[Option<u8>] = if step == 0 {
            &ladder
        } else {
            &ladder[ladder.len() - 1..]
        };
        for &q in rungs {
            let b = encode_at(&cur, q, false)?;
            if fits(&b) {
                return Ok(SizedResult {
                    bytes: b,
                    quality: q,
                    image: cur,
                    reached: true,
                });
            }
            last = Some((b, q));
        }
    }
    let (bytes, quality) = match last {
        Some(l) => l,
        None => (encode_at(&cur, None, false)?, None),
    };
    Ok(SizedResult {
        bytes,
        quality,
        image: cur,
        reached: false,
    })
}
