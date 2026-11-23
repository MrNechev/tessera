//! Perceptual hash for duplicate detection.

use std::path::Path;

use anyhow::bail;

/// A 64-bit difference hash (dHash) of a thumbnail: the image is reduced to 9×8 grey cells
/// and each bit says whether a cell is brighter than its right neighbour. Re-encoded,
/// resized or slightly edited copies of a photo end up a few bits apart.
pub fn phash(thumb_path: &Path) -> anyhow::Result<String> {
    let img = image::open(thumb_path)?.to_rgba8();
    let (w, h) = (img.width() as i64, img.height() as i64);
    if w < 9 || h < 8 {
        bail!("too small");
    }
    let mut cells = [[0f64; 9]; 8];
    for (cy, row) in cells.iter_mut().enumerate() {
        let cy = cy as i64;
        let (y0, y1) = (cy * h / 8, (cy + 1) * h / 8);
        for (cx, cell) in row.iter_mut().enumerate() {
            let cx = cx as i64;
            let (x0, x1) = (cx * w / 9, (cx + 1) * w / 9);
            let mut sum = 0f64;
            let mut n = 0;
            // sample a sparse grid inside the cell (thumbnails are ≤320 px, this is cheap)
            let mut y = y0;
            while y < y1 {
                let mut x = x0;
                while x < x1 {
                    let p = img.get_pixel(x as u32, y as u32);
                    // Go's color.RGBA(): 16-bit, alpha-premultiplied
                    let a = p[3] as f64 / 255.0;
                    let c = |v: u8| v as f64 * 257.0 * a;
                    sum += 0.299 * c(p[0]) + 0.587 * c(p[1]) + 0.114 * c(p[2]);
                    n += 1;
                    x += ((x1 - x0) / 6).max(1);
                }
                y += ((y1 - y0) / 6).max(1);
            }
            if n > 0 {
                *cell = sum / n as f64;
            }
        }
    }
    let mut v: u64 = 0;
    for row in &cells {
        for cx in 0..8 {
            v <<= 1;
            if row[cx] > row[cx + 1] {
                v |= 1;
            }
        }
    }
    Ok(format!("{v:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_hash() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("g.png");
        // brightness decreasing left → right: every bit set
        let img = image::RgbImage::from_fn(90, 80, |x, _| {
            let v = 255 - (x * 255 / 90) as u8;
            image::Rgb([v, v, v])
        });
        img.save(&p).unwrap();
        assert_eq!(phash(&p).unwrap(), "ffffffffffffffff");
        let small = dir.path().join("s.png");
        image::RgbImage::new(5, 5).save(&small).unwrap();
        assert!(phash(&small).is_err());
    }
}
