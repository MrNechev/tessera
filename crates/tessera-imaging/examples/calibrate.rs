//! Calibration helper for the smart compression thresholds.
//!
//! Usage:
//!   cargo run --release -p tessera-imaging --example calibrate -- curves <dir>
//!   cargo run --release -p tessera-imaging --example calibrate -- levels <dir> [jpeg|webp|avif]
//!
//! `curves` prints the search metric (mean tile SSIM) at fixed qualities for
//! every photo, both at full resolution and downscaled to a 1280 px long edge
//! (a nearly artefact-free reference), plus the medians per codec.
//! `levels` runs the real compression pipeline on every photo and reports
//! the chosen quality, SSIM, size and time per level.

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::DynamicImage;
use tessera_imaging::__internal::search_metric;
use tessera_imaging::*;

fn photos(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("readable directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| ImageFormat::from_path(p).is_some())
        .collect();
    v.sort();
    v
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if v.is_empty() {
        return f64::NAN;
    }
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn curves(dir: &Path) {
    let codecs: [(&str, ImageFormat, Chroma, &[u8]); 4] = [
        (
            "jpeg444",
            ImageFormat::Jpeg,
            Chroma::S444,
            &[60, 70, 75, 80, 85, 88, 90, 92, 95],
        ),
        (
            "jpeg420",
            ImageFormat::Jpeg,
            Chroma::S420,
            &[50, 60, 65, 70, 75, 80, 85, 90],
        ),
        (
            "webp",
            ImageFormat::Webp,
            Chroma::Auto,
            &[50, 60, 65, 70, 75, 80, 85, 90, 95],
        ),
        (
            "avif",
            ImageFormat::Avif,
            Chroma::Auto,
            &[40, 50, 60, 70, 75, 80, 85, 90],
        ),
    ];
    for variant in ["clean", "full"] {
        let mut table: Vec<Vec<Vec<f64>>> =
            codecs.iter().map(|c| vec![Vec::new(); c.3.len()]).collect();
        for p in photos(dir) {
            let src = load(&p).expect("decodable photo").image;
            let img = if variant == "clean" {
                apply_resize(src, &ResizeMode::LongEdge { pixels: 1280 })
            } else {
                src
            };
            let img = DynamicImage::ImageRgb8(img.to_rgb8());
            for (ci, (name, f, chroma, qs)) in codecs.iter().enumerate() {
                if variant == "full" && *f == ImageFormat::Avif {
                    continue;
                }
                let mut row = format!(
                    "{variant} {:>10} {name:8}",
                    p.file_name().unwrap().to_string_lossy()
                );
                for (qi, &q) in qs.iter().enumerate() {
                    let s = search_metric(&img, *f, q, *chroma).expect("encode");
                    table[ci][qi].push(s);
                    row.push_str(&format!(" q{q}={s:.4}"));
                }
                println!("{row}");
            }
        }
        println!("--- medians ({variant}) ---");
        for (ci, (name, _, _, qs)) in codecs.iter().enumerate() {
            let mut row = format!("{name:8}");
            for (qi, &q) in qs.iter().enumerate() {
                row.push_str(&format!(" q{q}={:.4}", median(table[ci][qi].clone())));
            }
            println!("{row}");
        }
    }
}

fn levels(dir: &Path, target: Option<ImageFormat>) {
    let levels = [
        ("visually-lossless", CompressLevel::VisuallyLossless),
        ("balanced", CompressLevel::Balanced),
        ("small", CompressLevel::Small),
    ];
    for p in photos(dir) {
        let orig = std::fs::metadata(&p).unwrap().len();
        for (name, level) in levels {
            let t = Instant::now();
            let opts = CompressOptions {
                level,
                format: target
                    .map(|format| TargetFormat::To { format })
                    .unwrap_or_default(),
                keep_if_larger: false,
                metadata: MetadataPolicy::strip_all(),
                ..Default::default()
            };
            let r = compress_preview(&p, &opts).expect("compress");
            println!(
                "{:>10} {:18} {:5} q={:>3} ssim={:.4} {:>9} -> {:>9} bytes ({:5.1}%) {:5.2}s",
                p.file_name().unwrap().to_string_lossy(),
                name,
                r.format.label(),
                r.quality
                    .map(|q| q.to_string())
                    .unwrap_or_else(|| "-".into()),
                r.ssim.unwrap_or(f64::NAN),
                orig,
                r.bytes.len(),
                100.0 * r.bytes.len() as f64 / orig as f64,
                t.elapsed().as_secs_f64()
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let dir = PathBuf::from(args.get(2).expect("photo directory"));
    match mode {
        "curves" => curves(&dir),
        "levels" => levels(
            &dir,
            args.get(3).and_then(|f| ImageFormat::from_extension(f)),
        ),
        _ => eprintln!("usage: calibrate (curves|levels) <dir> [format]"),
    }
}
