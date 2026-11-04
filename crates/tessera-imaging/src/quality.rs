//! Perceptual similarity (SSIM).
//!
//! Implementation notes:
//! * Luma only (BT.601 weights), computed on 8-bit values after compositing
//!   any transparency onto white.
//! * 8x8 box windows evaluated at every pixel position (stride 1) using
//!   running column/row sums, so the cost is O(N) with O(width) memory.
//! * Standard constants C1 = (0.01*255)^2, C2 = (0.03*255)^2; the result is
//!   the mean SSIM over all windows.
//! * Images above ~4 MP are box-downscaled by the same integer factor first.
//!   This keeps the cost bounded but hides very fine artefacts, so values on
//!   large images read slightly higher than at 100% zoom. The smart
//!   compressor therefore searches on full-resolution tiles instead.

use image::RgbaImage;

const WIN: usize = 8;
const MAX_PIXELS: u64 = 4_000_000;
const C1: f64 = (0.01 * 255.0) * (0.01 * 255.0);
const C2: f64 = (0.03 * 255.0) * (0.03 * 255.0);

/// Mean luma SSIM between two images of identical size (1.0 = identical).
/// Returns 0.0 when the sizes differ or an image is empty.
pub fn ssim(a: &RgbaImage, b: &RgbaImage) -> f64 {
    if a.dimensions() != b.dimensions() || a.width() == 0 || a.height() == 0 {
        return 0.0;
    }
    let (w, h) = (a.width() as usize, a.height() as usize);
    let la = luma_plane(a);
    let lb = luma_plane(b);
    let pixels = (w as u64) * (h as u64);
    if pixels > MAX_PIXELS {
        let k = ((pixels as f64 / MAX_PIXELS as f64).sqrt().ceil() as usize).max(2);
        let (da, dw, dh) = downscale(&la, w, h, k);
        let (db, _, _) = downscale(&lb, w, h, k);
        return ssim_planes(&da, &db, dw, dh);
    }
    ssim_planes(&la, &lb, w, h)
}

/// 8-bit luma of an RGBA image composited over white.
pub(crate) fn luma_plane(img: &RgbaImage) -> Vec<u8> {
    img.pixels()
        .map(|p| {
            let [r, g, b, a] = p.0;
            let (r, g, b) = if a == 255 {
                (r as u32, g as u32, b as u32)
            } else {
                let a = a as u32;
                let inv = 255 - a;
                (
                    (r as u32 * a + 255 * inv + 127) / 255,
                    (g as u32 * a + 255 * inv + 127) / 255,
                    (b as u32 * a + 255 * inv + 127) / 255,
                )
            };
            ((299 * r + 587 * g + 114 * b + 500) / 1000) as u8
        })
        .collect()
}

fn downscale(p: &[u8], w: usize, h: usize, k: usize) -> (Vec<u8>, usize, usize) {
    let dw = (w / k).max(1);
    let dh = (h / k).max(1);
    let mut out = vec![0u8; dw * dh];
    for y in 0..dh {
        for x in 0..dw {
            let mut s = 0u32;
            let mut n = 0u32;
            for yy in y * k..((y + 1) * k).min(h) {
                let row = &p[yy * w..yy * w + w];
                for &v in &row[x * k..((x + 1) * k).min(w)] {
                    s += v as u32;
                    n += 1;
                }
            }
            out[y * dw + x] = ((s + n / 2) / n.max(1)) as u8;
        }
    }
    (out, dw, dh)
}

#[inline]
fn window_ssim(n: f64, sx: f64, sy: f64, sxx: f64, syy: f64, sxy: f64) -> f64 {
    let mx = sx / n;
    let my = sy / n;
    let vx = (sxx / n - mx * mx).max(0.0);
    let vy = (syy / n - my * my).max(0.0);
    let cxy = sxy / n - mx * my;
    ((2.0 * mx * my + C1) * (2.0 * cxy + C2)) / ((mx * mx + my * my + C1) * (vx + vy + C2))
}

/// SSIM of two luma planes of size w x h.
pub(crate) fn ssim_planes(a: &[u8], b: &[u8], w: usize, h: usize) -> f64 {
    debug_assert_eq!(a.len(), w * h);
    debug_assert_eq!(b.len(), w * h);
    if w < WIN || h < WIN {
        // Single global window.
        let n = (w * h) as f64;
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (&x, &y) in a.iter().zip(b) {
            let (x, y) = (x as f64, y as f64);
            sx += x;
            sy += y;
            sxx += x * x;
            syy += y * y;
            sxy += x * y;
        }
        return window_ssim(n, sx, sy, sxx, syy, sxy);
    }
    // Column sums over the current 8-row band.
    let mut cx = vec![0u32; w];
    let mut cy = vec![0u32; w];
    let mut cxx = vec![0u32; w];
    let mut cyy = vec![0u32; w];
    let mut cxy = vec![0u32; w];
    let add_row = |row: usize,
                   sign: bool,
                   cx: &mut [u32],
                   cy: &mut [u32],
                   cxx: &mut [u32],
                   cyy: &mut [u32],
                   cxy: &mut [u32]| {
        let ra = &a[row * w..row * w + w];
        let rb = &b[row * w..row * w + w];
        for i in 0..w {
            let x = ra[i] as u32;
            let y = rb[i] as u32;
            if sign {
                cx[i] += x;
                cy[i] += y;
                cxx[i] += x * x;
                cyy[i] += y * y;
                cxy[i] += x * y;
            } else {
                cx[i] -= x;
                cy[i] -= y;
                cxx[i] -= x * x;
                cyy[i] -= y * y;
                cxy[i] -= x * y;
            }
        }
    };
    for row in 0..WIN {
        add_row(row, true, &mut cx, &mut cy, &mut cxx, &mut cyy, &mut cxy);
    }
    let n = (WIN * WIN) as f64;
    let mut total = 0.0f64;
    let mut count = 0u64;
    let mut top = 0;
    loop {
        // Horizontal sliding over the column sums.
        let (mut sx, mut sy, mut sxx, mut syy, mut sxy) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for i in 0..WIN {
            sx += cx[i];
            sy += cy[i];
            sxx += cxx[i];
            syy += cyy[i];
            sxy += cxy[i];
        }
        let mut row_total = 0.0f64;
        let mut x = 0;
        loop {
            row_total += window_ssim(n, sx as f64, sy as f64, sxx as f64, syy as f64, sxy as f64);
            if x + WIN >= w {
                break;
            }
            sx = sx + cx[x + WIN] - cx[x];
            sy = sy + cy[x + WIN] - cy[x];
            sxx = sxx + cxx[x + WIN] - cxx[x];
            syy = syy + cyy[x + WIN] - cyy[x];
            sxy = sxy + cxy[x + WIN] - cxy[x];
            x += 1;
        }
        total += row_total;
        count += (w - WIN + 1) as u64;
        if top + WIN >= h {
            break;
        }
        add_row(top, false, &mut cx, &mut cy, &mut cxx, &mut cyy, &mut cxy);
        add_row(
            top + WIN,
            true,
            &mut cx,
            &mut cy,
            &mut cxx,
            &mut cyy,
            &mut cxy,
        );
        top += 1;
    }
    (total / count as f64).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_one() {
        let img = RgbaImage::from_fn(64, 48, |x, y| {
            image::Rgba([(x * 4) as u8, (y * 5) as u8, 7, 255])
        });
        assert!((ssim(&img, &img) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn noise_lowers_ssim() {
        let a = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([(x * 4) as u8, (y * 4) as u8, 128, 255])
        });
        let b = RgbaImage::from_fn(64, 64, |x, y| {
            let n = ((x * 7 + y * 13) % 17) as u8;
            image::Rgba([(x * 4) as u8 ^ n, (y * 4) as u8, 128, 255])
        });
        let s = ssim(&a, &b);
        assert!(s < 0.99 && s > 0.0, "{s}");
    }
}
