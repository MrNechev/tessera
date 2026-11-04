mod common;

use common::*;
use image::{imageops, DynamicImage, RgbImage};
use tessera_imaging::*;

/// Stored (sensor) pixels for an upright image displayed with `orientation`.
fn stored_for(upright: &RgbImage, orientation: u16) -> RgbImage {
    match orientation {
        1 => upright.clone(),
        3 => imageops::rotate180(upright),
        // Displayed = stored rotated 90° clockwise => stored = upright rotated 90° CCW.
        6 => imageops::rotate270(upright),
        8 => imageops::rotate90(upright),
        _ => unimplemented!(),
    }
}

fn jpeg_with_orientation(dir: &std::path::Path, orientation: u16) -> std::path::PathBuf {
    let stored = DynamicImage::ImageRgb8(stored_for(&quadrants(), orientation));
    let exif = exif_block(orientation);
    let bytes = encode_with(&stored, ImageFormat::Jpeg, 95, Some(&exif), None, None);
    write(dir, &format!("o{orientation}.jpg"), &bytes)
}

#[test]
fn probe_and_load_apply_orientation() {
    let dir = tmpdir("orient");
    for o in [1u16, 3, 6, 8] {
        let p = jpeg_with_orientation(&dir, o);
        let info = probe(&p).unwrap();
        assert_eq!(info.format, ImageFormat::Jpeg);
        assert_eq!(info.orientation, o);
        assert_eq!((info.width, info.height), (64, 32), "probe o={o}");
        assert_eq!(info.taken_at.as_deref(), Some("2023-07-14T10:20:30"));

        let src = load(&p).unwrap();
        assert_eq!(src.orientation, o);
        assert_upright_quadrants(&src.image.to_rgba8());
        assert!(src.metadata.exif.is_some());

        let scaled = load_scaled_rgba(&p, 32, 32).unwrap();
        assert_eq!(scaled.dimensions(), (32, 16), "scaled o={o}");
        assert_upright_quadrants(&scaled);

        let prev = make_preview(&p, &dir.join(format!("prev{o}.png")), 40, 85).unwrap();
        assert!(prev.path.ends_with(".jpg"), "{}", prev.path);
        assert_eq!((prev.width, prev.height), (40, 20));
        let back = load(std::path::Path::new(&prev.path)).unwrap();
        assert_upright_quadrants(&back.image.to_rgba8());
    }
}

#[test]
fn large_jpeg_preview_uses_scaled_decode() {
    let dir = tmpdir("preview");
    let stored = DynamicImage::ImageRgb8(stored_for(
        &imageops::resize(&quadrants(), 2048, 1024, imageops::FilterType::Nearest),
        6,
    ));
    let bytes = encode_with(
        &stored,
        ImageFormat::Jpeg,
        90,
        Some(&exif_block(6)),
        None,
        None,
    );
    let p = write(&dir, "big.jpg", &bytes);
    let info = probe(&p).unwrap();
    assert_eq!((info.width, info.height), (2048, 1024));
    let prev = make_preview(&p, &dir.join("big-preview.jpg"), 300, 80).unwrap();
    assert_eq!((prev.width, prev.height), (300, 150));
    assert!(prev.path.ends_with("big-preview.jpg"));
    let img = load_scaled_rgba(&p, 512, 512).unwrap();
    assert_eq!(img.dimensions(), (512, 256));
    assert_upright_quadrants(&img);
}

#[test]
fn transparent_preview_is_png() {
    let dir = tmpdir("preview-alpha");
    let mut rgba = DynamicImage::ImageRgb8(quadrants()).to_rgba8();
    rgba.get_pixel_mut(0, 0).0[3] = 0;
    let bytes = encode(
        &DynamicImage::ImageRgba8(rgba),
        &EncodeOptions {
            format: ImageFormat::Png,
            lossless: true,
            ..Default::default()
        },
    )
    .unwrap();
    let p = write(&dir, "a.png", &bytes);
    let prev = make_preview(&p, &dir.join("a-prev.jpg"), 32, 80).unwrap();
    assert!(prev.path.ends_with("a-prev.png"), "{}", prev.path);
    assert_eq!((prev.width, prev.height), (32, 16));
}

#[test]
fn reencode_resets_orientation_and_rotates_pixels() {
    let dir = tmpdir("orient-reencode");
    for o in [3u16, 6] {
        let p = jpeg_with_orientation(&dir, o);
        for target in [
            ImageFormat::Jpeg,
            ImageFormat::Png,
            ImageFormat::Webp,
            ImageFormat::Tiff,
        ] {
            let opts = ConvertOptions {
                format: TargetFormat::To { format: target },
                quality: QualityMode::Reduce { percent: 5 },
                output: OutputSpec {
                    directory: Some(dir.join("out").to_string_lossy().into()),
                    suffix: format!("-{o}"),
                    conflict: ConflictPolicy::Overwrite,
                },
                ..Default::default()
            };
            let r = convert_file(&p, &opts);
            assert_eq!(r.status, JobStatus::Done, "{target:?}: {:?}", r.message);
            assert_eq!((r.width, r.height), (64, 32));
            let out = std::path::PathBuf::from(r.output.unwrap());
            let e = read_exif(&out).unwrap_or_else(|| panic!("{target:?}: no exif"));
            assert_eq!(orientation_of(&e), Some(1), "{target:?}");
            let back = load(&out).unwrap();
            assert_eq!(back.orientation, 1);
            assert_upright_quadrants(&back.image.to_rgba8());
        }
    }
}
