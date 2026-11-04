mod common;

use std::path::{Path, PathBuf};

use common::*;
use exif::Tag;
use image::DynamicImage;
use tessera_imaging::*;

fn out_spec(dir: &Path, suffix: &str, conflict: ConflictPolicy) -> OutputSpec {
    OutputSpec {
        directory: Some(dir.to_string_lossy().into()),
        suffix: suffix.into(),
        conflict,
    }
}

fn photo_jpeg(dir: &Path, name: &str, w: u32, h: u32, q: u8, exif: Option<&[u8]>) -> PathBuf {
    let img = DynamicImage::ImageRgb8(photo_like(w, h, 7));
    let bytes = encode_with(
        &img,
        ImageFormat::Jpeg,
        q,
        exif,
        Some(&fake_icc("P")),
        Some(XMP.as_bytes()),
    );
    write(dir, name, &bytes)
}

#[test]
fn jpeg_to_jpeg_original_is_lossless_passthrough() {
    let dir = tmpdir("jobs-passthrough");
    let src = photo_jpeg(&dir, "a.jpg", 320, 200, 90, Some(&exif_block(6)));
    let orig = std::fs::read(&src).unwrap();
    for (policy, tag) in [
        (MetadataPolicy::keep_all(), "keep"),
        (MetadataPolicy::strip_all(), "strip"),
        (
            MetadataPolicy {
                mode: MetadataMode::Custom,
                keep_location: false,
                ..Default::default()
            },
            "custom",
        ),
    ] {
        let opts = ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Jpeg,
            },
            metadata: policy.clone(),
            output: out_spec(
                &dir.join("out"),
                &format!("-{tag}"),
                ConflictPolicy::Overwrite,
            ),
            ..Default::default()
        };
        let r = convert_file(&src, &opts);
        assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
        assert_eq!(r.ssim, Some(1.0));
        assert_eq!((r.width, r.height), (200, 320), "upright dims");
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let out = std::fs::read(r.output.as_ref().unwrap()).unwrap();
        assert_eq!(
            jpeg_image_data(&out).unwrap(),
            jpeg_image_data(&orig).unwrap(),
            "{tag}: image data must be identical"
        );
        let e = read_exif(Path::new(r.output.as_ref().unwrap()));
        match policy.mode {
            MetadataMode::KeepAll => assert_eq!(out, orig, "KeepAll copies the file"),
            MetadataMode::StripAll => {
                // Only the orientation survives so the photo still displays upright.
                let e = e.unwrap();
                assert_eq!(orientation_of(&e), Some(6));
                assert_eq!(e.fields().count(), 1);
                let s = read_summary(Path::new(r.output.as_ref().unwrap())).unwrap();
                assert!(!s.has_icc && !s.has_xmp);
                assert!(out.len() < orig.len());
            }
            MetadataMode::Custom => {
                let e = e.unwrap();
                assert_eq!(orientation_of(&e), Some(6));
                assert!(e.get_field(Tag::GPSLatitude, exif::In::PRIMARY).is_none());
                assert_eq!(ascii_field(&e, Tag::Make).as_deref(), Some("TestCam"));
            }
        }
        let back = load(Path::new(r.output.as_ref().unwrap())).unwrap();
        assert_eq!((back.image.width(), back.image.height()), (200, 320));
    }
}

#[test]
fn quality_modes() {
    let dir = tmpdir("jobs-quality");
    let img = DynamicImage::ImageRgb8(photo_like(256, 192, 4));
    let src = write(
        &dir,
        "p.png",
        &encode(
            &img,
            &EncodeOptions {
                format: ImageFormat::Png,
                lossless: true,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    // Original -> lossless WebP: pixels identical.
    let r = convert_file(
        &src,
        &ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Webp,
            },
            output: out_spec(&dir, "", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done);
    assert_eq!(r.ssim, Some(1.0));
    let back = load(Path::new(r.output.as_ref().unwrap())).unwrap();
    assert_eq!(back.image.to_rgba8(), img.to_rgba8());
    // Original -> JPEG warns.
    let r = convert_file(
        &src,
        &ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Jpeg,
            },
            output: out_spec(&dir, "", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r.quality, Some(100));
    assert!(
        r.warnings.iter().any(|w| w.contains("always lossy")),
        "{:?}",
        r.warnings
    );
    // Reduce by 40% -> quality 60, smaller than q100.
    let r60 = convert_file(
        &src,
        &ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Jpeg,
            },
            quality: QualityMode::Reduce { percent: 40 },
            output: out_spec(&dir, "-60", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r60.quality, Some(60));
    assert!(r60.output_bytes < r.output_bytes);
    assert!(r60.ssim.unwrap() < r.ssim.unwrap());
    // BMP ignores quality with a warning.
    let r = convert_file(
        &src,
        &ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Bmp,
            },
            quality: QualityMode::Reduce { percent: 40 },
            output: out_spec(&dir, "", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done);
    assert!(r.warnings.iter().any(|w| w.contains("ignored")));
    // Resize.
    let r = convert_file(
        &src,
        &ConvertOptions {
            format: TargetFormat::Keep,
            resize: ResizeMode::Percent { percent: 50.0 },
            output: out_spec(&dir, "-half", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!((r.width, r.height), (128, 96));
    assert_eq!(r.format, Some(ImageFormat::Png));
}

#[test]
fn output_naming_and_conflicts() {
    let dir = tmpdir("jobs-naming");
    let src = photo_jpeg(&dir, "photo.jpg", 64, 48, 90, None);
    let out_dir = dir.join("nested").join("out");
    let opts = ConvertOptions {
        format: TargetFormat::To {
            format: ImageFormat::Png,
        },
        output: out_spec(&out_dir, "-x", ConflictPolicy::Rename),
        ..Default::default()
    };
    let a = convert_file(&src, &opts);
    let b = convert_file(&src, &opts);
    assert_eq!(
        a.output.as_deref(),
        Some(out_dir.join("photo-x.png").to_str().unwrap())
    );
    assert_eq!(
        b.output.as_deref(),
        Some(out_dir.join("photo-x (1).png").to_str().unwrap())
    );
    let skip = convert_file(
        &src,
        &ConvertOptions {
            output: out_spec(&out_dir, "-x", ConflictPolicy::Skip),
            ..opts.clone()
        },
    );
    assert_eq!(skip.status, JobStatus::Skipped);

    // Next to the input, same name: Rename must not touch the input.
    let next = ConvertOptions {
        format: TargetFormat::Keep,
        quality: QualityMode::Reduce { percent: 50 },
        output: OutputSpec {
            directory: None,
            suffix: String::new(),
            conflict: ConflictPolicy::Rename,
        },
        ..Default::default()
    };
    let before = std::fs::read(&src).unwrap();
    let r = convert_file(&src, &next);
    assert_eq!(
        r.output.as_deref(),
        Some(dir.join("photo (1).jpg").to_str().unwrap())
    );
    assert_eq!(std::fs::read(&src).unwrap(), before);
    // Overwrite in place replaces the input atomically.
    let r = convert_file(
        &src,
        &ConvertOptions {
            output: OutputSpec {
                conflict: ConflictPolicy::Overwrite,
                ..next.output.clone()
            },
            ..next
        },
    );
    assert_eq!(r.status, JobStatus::Done);
    assert_eq!(r.output.as_deref(), Some(src.to_str().unwrap()));
    assert_ne!(std::fs::read(&src).unwrap(), before);
    assert!(load(&src).is_ok());
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("tessera-tmp"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn corrupt_inputs_fail_cleanly() {
    let dir = tmpdir("jobs-corrupt");
    let mut garbage = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
    garbage.extend((0..500u32).map(|i| (i * 31 % 251) as u8));
    let files = [
        write(&dir, "garbage.jpg", &garbage),
        write(&dir, "empty.png", &[]),
        write(
            &dir,
            "trunc.png",
            &encode(
                &DynamicImage::ImageRgb8(photo_like(64, 64, 1)),
                &EncodeOptions {
                    format: ImageFormat::Png,
                    lossless: true,
                    ..Default::default()
                },
            )
            .unwrap()[..60],
        ),
        write(
            &dir,
            "trunc.webp",
            b"RIFF\x40\0\0\0WEBPVP8 \x30\0\0\0\x50\x5a",
        ),
        write(&dir, "text.txt", b"hello world"),
        write(&dir, "fake.heic", b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
        write(&dir, "fake.jxl", &[0xFF, 0x0A, 1, 2, 3]),
        write(&dir, "fake.avif", b"\0\0\0\x18ftypavif\0\0\0\0mif1avif"),
    ];
    for f in &files {
        assert!(load(f).is_err(), "{f:?}");
        let _ = probe(f); // must not panic
        let _ = read_summary(f);
        let _ = make_preview(f, &dir.join("prev.jpg"), 64, 80);
        let r = convert_file(
            f,
            &ConvertOptions {
                output: out_spec(&dir.join("out"), "", ConflictPolicy::Rename),
                ..Default::default()
            },
        );
        assert_eq!(r.status, JobStatus::Failed, "{f:?}");
        assert!(r.message.is_some());
        let r = compress_file(
            f,
            &CompressOptions {
                output: out_spec(&dir.join("out"), "", ConflictPolicy::Rename),
                ..Default::default()
            },
        );
        assert_eq!(r.status, JobStatus::Failed, "{f:?}");
    }
    let missing = convert_file(&dir.join("nope.jpg"), &ConvertOptions::default());
    assert_eq!(missing.status, JobStatus::Failed);
}

#[test]
fn smart_compression_levels() {
    let dir = tmpdir("jobs-smart");
    // A high-quality source so that smart levels can save bytes.
    let src = photo_jpeg(&dir, "hq.jpg", 900, 600, 98, Some(&exif_block(1)));
    let src_len = std::fs::metadata(&src).unwrap().len();
    let (vl, bal, small) = smart_thresholds();
    let mut sizes = Vec::new();
    for (level, thr) in [
        (CompressLevel::VisuallyLossless, vl),
        (CompressLevel::Balanced, bal),
        (CompressLevel::Small, small),
    ] {
        let p = compress_preview(
            &src,
            &CompressOptions {
                level,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.format, ImageFormat::Jpeg);
        assert!(!p.kept_original, "{level:?}");
        assert!(
            (p.bytes.len() as u64) < src_len,
            "{level:?}: {} >= {src_len}",
            p.bytes.len()
        );
        let s = p.ssim.unwrap();
        assert!(s >= thr, "{level:?}: ssim {s} < {thr}");
        sizes.push(p.bytes.len());
    }
    assert!(sizes[0] >= sizes[1] && sizes[1] >= sizes[2], "{sizes:?}");

    for target in [ImageFormat::Webp, ImageFormat::Avif] {
        let p = compress_preview(
            &src,
            &CompressOptions {
                level: CompressLevel::Balanced,
                format: TargetFormat::To { format: target },
                resize: ResizeMode::LongEdge { pixels: 400 },
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(p.format, target);
        assert_eq!((p.width, p.height), (400, 267));
        assert!(p.ssim.unwrap() >= bal, "{target:?} {:?}", p.ssim);
        assert!(p.quality.is_some());
    }

    // Writes a file and reports.
    let r = compress_file(
        &src,
        &CompressOptions {
            output: out_spec(&dir.join("out"), "-c", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done);
    assert!(r.output_bytes < r.input_bytes);
    let e = read_exif(Path::new(r.output.as_ref().unwrap())).unwrap();
    assert_eq!(ascii_field(&e, Tag::Make).as_deref(), Some("TestCam"));
}

#[test]
fn smart_compression_large_image_uses_tiles() {
    let dir = tmpdir("jobs-tiles");
    let img = DynamicImage::ImageRgb8(photo_like(2200, 1500, 11));
    let src = write(
        &dir,
        "big.png",
        &encode(
            &img,
            &EncodeOptions {
                format: ImageFormat::Png,
                lossless: true,
                effort: 0,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let (vl, _, _) = smart_thresholds();
    let p = compress_preview(
        &src,
        &CompressOptions {
            level: CompressLevel::VisuallyLossless,
            format: TargetFormat::To {
                format: ImageFormat::Jpeg,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(p.ssim.unwrap() >= vl - 0.005, "{:?}", p.ssim);
    assert!(p.bytes.len() < p.original_bytes as usize);
}

#[test]
fn lossless_compression() {
    let dir = tmpdir("jobs-lossless");
    let img = DynamicImage::ImageRgb8(photo_like(400, 300, 5));
    // Baseline, non-optimised JPEG.
    let base = encode(
        &img,
        &EncodeOptions {
            progressive: false,
            effort: 0,
            quality: 88,
            ..Default::default()
        },
    )
    .unwrap();
    let (base, _) = embed(base, ImageFormat::Jpeg, Some(&exif_block(1)), None, None).unwrap();
    let src = write(&dir, "base.jpg", &base);
    let p = compress_preview(
        &src,
        &CompressOptions {
            level: CompressLevel::Lossless,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.format, ImageFormat::Jpeg);
    assert!(
        p.bytes.len() < base.len(),
        "{} vs {}",
        p.bytes.len(),
        base.len()
    );
    let a = load_bytes(&base, None).unwrap().image.to_rgba8();
    let b = load_bytes(&p.bytes, None).unwrap().image.to_rgba8();
    assert_eq!(a, b, "lossless JPEG optimisation must not change pixels");
    let e = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(&p.bytes))
        .unwrap();
    assert_eq!(ascii_field(&e, Tag::Make).as_deref(), Some("TestCam"));
    let opt = jpeg_optimize_lossless(&base, false).unwrap();
    assert_eq!(load_bytes(&opt, None).unwrap().image.to_rgba8(), a);

    // PNG: oxipng only.
    let png = encode(
        &img,
        &EncodeOptions {
            format: ImageFormat::Png,
            lossless: true,
            effort: 0,
            ..Default::default()
        },
    )
    .unwrap();
    let src = write(&dir, "p.png", &png);
    let p = compress_preview(
        &src,
        &CompressOptions {
            level: CompressLevel::Lossless,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        load_bytes(&p.bytes, None).unwrap().image.to_rgba8(),
        img.to_rgba8()
    );
    assert!(p.bytes.len() <= png.len());
}

#[test]
fn keep_if_larger_keeps_original() {
    let dir = tmpdir("jobs-keep");
    // Already heavily compressed JPEG: VL re-encode will be larger.
    let src = photo_jpeg(&dir, "low.jpg", 300, 200, 40, Some(&exif_block(1)));
    let orig_len = std::fs::metadata(&src).unwrap().len();
    let p = compress_preview(
        &src,
        &CompressOptions {
            level: CompressLevel::VisuallyLossless,
            metadata: MetadataPolicy::keep_all(),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(p.kept_original);
    assert_eq!(p.bytes.len() as u64, orig_len);
    let r = compress_file(
        &src,
        &CompressOptions {
            level: CompressLevel::VisuallyLossless,
            output: out_spec(&dir.join("out"), "", ConflictPolicy::Rename),
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done);
    assert!(r.message.unwrap().contains("Already optimized"));
    // With keep_if_larger off we get the re-encode.
    let p = compress_preview(
        &src,
        &CompressOptions {
            level: CompressLevel::VisuallyLossless,
            keep_if_larger: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!p.kept_original);
}

#[test]
fn target_size() {
    let dir = tmpdir("jobs-target");
    let img = DynamicImage::ImageRgb8(photo_like(1200, 900, 8));
    let src = write(
        &dir,
        "t.png",
        &encode(
            &img,
            &EncodeOptions {
                format: ImageFormat::Png,
                lossless: true,
                effort: 0,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    for (target, budget) in [
        (ImageFormat::Jpeg, 80_000u64),
        (ImageFormat::Webp, 40_000),
        (ImageFormat::Jpeg, 6_000),
        (ImageFormat::Png, 150_000),
    ] {
        let p = compress_preview(
            &src,
            &CompressOptions {
                level: CompressLevel::TargetSize { bytes: budget },
                format: TargetFormat::To { format: target },
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            p.bytes.len() as u64 <= budget,
            "{target:?} {budget}: {}",
            p.bytes.len()
        );
        if budget >= 40_000 && target != ImageFormat::Png {
            assert_eq!((p.width, p.height), (1200, 900), "{target:?}");
        }
    }
}
