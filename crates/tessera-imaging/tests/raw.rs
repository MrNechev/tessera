//! Camera RAW support. A tiny linear (demosaiced) DNG is synthesised with
//! rawler's own DNG writer, so no fixture is needed. Real camera files are
//! exercised by `raw_samples` when `TESSERA_RAW_SAMPLES=<dir>` is set.

mod common;

use std::io::Cursor;
use std::path::Path;
use std::time::Instant;

use common::*;
use rawler::dng::writer::DngWriter;
use rawler::dng::{DngCompression, DNG_VERSION_V1_4};
use rawler::formats::tiff::{Rational, SRational};
use rawler::imgop::xyz::Illuminant;
use rawler::tags::{ExifTag, TiffCommonTag};
use tessera_imaging::*;

/// Quadrant colours in linear light (TL red, TR green, BL blue, BR white).
fn linear_quadrants(w: usize, h: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let px = match (x < w / 2, y < h / 2) {
                (true, true) => [180, 4, 4],
                (false, true) => [4, 160, 4],
                (true, false) => [4, 4, 200],
                (false, false) => [255, 255, 255],
            };
            v.extend_from_slice(&px);
        }
    }
    v
}

/// Minimal LinearRaw DNG (8-bit RGB, uncompressed) with an XYZ→sRGB
/// colour matrix (camera RGB = linear sRGB), neutral white balance, the
/// given orientation and a few EXIF fields.
fn synthetic_dng(w: usize, h: usize, orientation: u16) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut dng = DngWriter::new(&mut buf, DNG_VERSION_V1_4).unwrap();
        let mut raw = dng.subframe_on_root(0);
        raw.rgb_image_u8(
            &linear_quadrants(w, h),
            w,
            h,
            DngCompression::Uncompressed,
            1,
        )
        .unwrap();
        raw.finalize().unwrap();
        // XYZ (D65) → linear sRGB.
        let m = [
            3.2404542, -1.5371385, -0.4985314, -0.9692660, 1.8760108, 0.0415560, 0.0556434,
            -0.2040259, 1.0572252,
        ];
        let m: Vec<SRational> = m
            .iter()
            .map(|v: &f64| SRational::new((v * 10000.0).round() as i32, 10000))
            .collect();
        dng.color_matrix(1, Illuminant::D65, &m);
        dng.as_shot_neutral([Rational::new(1, 1); 3]);
        let root = dng.root_ifd_mut();
        root.add_tag(TiffCommonTag::Make, "Tessera");
        root.add_tag(TiffCommonTag::Model, "Synthetic DNG");
        root.add_tag(
            rawler::tags::DngTag::UniqueCameraModel,
            "Tessera Synthetic DNG",
        );
        root.add_tag(TiffCommonTag::Orientation, orientation);
        let exif = dng.exif_ifd_mut();
        exif.add_tag(ExifTag::DateTimeOriginal, "2024:05:06 07:08:09");
        exif.add_tag(ExifTag::FNumber, Rational::new(28, 10));
        exif.add_tag(ExifTag::ExposureTime, Rational::new(1, 250));
        exif.add_tag(ExifTag::ISOSpeedRatings, 400_u16);
        exif.add_tag(ExifTag::FocalLength, Rational::new(50, 1));
        dng.close().unwrap();
    }
    buf.into_inner()
}

#[test]
fn raw_format_table() {
    for e in [
        "dng", "CR2", "cr3", "nef", "arw", "raf", "orf", "rw2", "pef", "x3f", "3fr",
    ] {
        assert_eq!(
            ImageFormat::from_extension(e),
            Some(ImageFormat::Raw),
            "{e}"
        );
    }
    let r = ImageFormat::Raw;
    assert_eq!(serde_json::to_string(&r).unwrap(), "\"raw\"");
    assert_eq!(r.label(), "Camera RAW");
    assert!(r.can_decode() && !r.can_encode() && r.supports_exif());
    assert!(input_extensions().contains(&"nef") && input_extensions().contains(&"cr3"));
    assert!(capabilities().iter().any(|c| c.format == ImageFormat::Raw));
    // Magic numbers.
    assert_eq!(ImageFormat::detect(b"FUJIFILMCCD-RAW 0201"), Some(r));
    assert_eq!(ImageFormat::detect(b"IIRO\x08\0\0\0"), Some(r));
    assert_eq!(ImageFormat::detect(b"IIU\0\x08\0\0\0"), Some(r));
    assert_eq!(ImageFormat::detect(b"II*\0\x10\0\0\0CR\x02\0"), Some(r));
    let mut cr3 = vec![0, 0, 0, 0x18];
    cr3.extend_from_slice(b"ftypcrx \0\0\0\x01crx isom");
    assert_eq!(ImageFormat::detect(&cr3), Some(r));
    let dng = synthetic_dng(8, 8, 1);
    assert_eq!(ImageFormat::detect(&dng), Some(r));
    // A plain TIFF stays TIFF, but is RAW when named like a TIFF-based RAW.
    let tiff = encode_with(
        &image::DynamicImage::ImageRgb8(quadrants()),
        ImageFormat::Tiff,
        100,
        None,
        None,
        None,
    );
    assert_eq!(ImageFormat::detect(&tiff), Some(ImageFormat::Tiff));
    let dir = tmpdir("raw-detect");
    let p = write(&dir, "x.nef", &tiff);
    assert_eq!(ImageFormat::detect_path(&p), Some(r));
    let p = write(&dir, "x.tif", &tiff);
    assert_eq!(ImageFormat::detect_path(&p), Some(ImageFormat::Tiff));
}

#[test]
fn synthetic_dng_decode_probe_convert() {
    let dir = tmpdir("raw-dng");
    // Upright file (Orientation 1).
    let upright_src = write(&dir, "upright.dng", &synthetic_dng(64, 32, 1));
    let src = load(&upright_src).unwrap();
    assert_eq!(src.format, ImageFormat::Raw);
    assert!(src.metadata.icc.is_none() && src.metadata.xmp.is_none());
    assert_upright_quadrants(&src.image.to_rgba8());
    // Linear 255 white stays white; linear red (180) is sRGB-encoded (~ 224).
    let px = src.image.to_rgb8();
    assert_eq!(px.get_pixel(60, 30).0, [255, 255, 255]);
    let red = px.get_pixel(4, 4).0;
    assert!(red[0] > 210 && red[1] < 60, "{red:?}");

    // Rotated file: stored landscape, Orientation 6 → upright portrait.
    let rotated = write(&dir, "rot.dng", &synthetic_dng(64, 32, 6));
    let info = probe(&rotated).unwrap();
    assert_eq!((info.width, info.height, info.orientation), (32, 64, 6));
    assert_eq!(info.taken_at.as_deref(), Some("2024-05-06T07:08:09"));
    let src = load(&rotated).unwrap();
    assert_eq!((src.image.width(), src.image.height()), (32, 64));
    assert_eq!(src.orientation, 6);
    // Sensor TL (red) ends up top-right after a 90° CW rotation.
    assert_eq!(colour_name(src.image.to_rgba8().get_pixel(28, 4)), "red");

    // Synthesised EXIF is a bare TIFF readable by the summary code.
    let exif = src.metadata.exif.as_deref().expect("exif");
    assert!(exif.starts_with(b"II*\0") || exif.starts_with(b"MM\0*"));
    let s = summarize_exif(exif).unwrap();
    assert_eq!(s.camera_make.as_deref(), Some("Tessera"));
    assert_eq!(s.camera_model.as_deref(), Some("Synthetic DNG"));
    assert_eq!(s.date_taken.as_deref(), Some("2024-05-06T07:08:09"));
    assert_eq!(s.f_number, Some(2.8));
    assert_eq!(s.iso, Some(400));
    assert_eq!(s.focal_length, Some(50.0));
    assert_eq!(s.exposure_time.as_deref(), Some("1/250 s"));
    assert_eq!(s.orientation, 1);

    let sum = read_summary(&rotated).unwrap();
    assert_eq!(sum.format, Some(ImageFormat::Raw));
    assert!(sum.raw && sum.has_exif && sum.icc_is_srgb && !sum.has_icc);
    assert_eq!((sum.width, sum.height), (32, 64));

    // Previews/scaled loads are upright (no embedded preview → develop).
    let rgba = load_scaled_rgba(&rotated, 16, 16).unwrap();
    assert_eq!(rgba.dimensions(), (8, 16));
    let pv = make_preview(&upright_src, &dir.join("pv.jpg"), 32, 90).unwrap();
    assert_eq!((pv.width, pv.height), (32, 16));

    // Keep → JPEG with a warning; EXIF carried over, Orientation 1.
    let r = convert_file(&rotated, &ConvertOptions::default());
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert_eq!(r.format, Some(ImageFormat::Jpeg));
    assert_eq!(r.quality, Some(100));
    assert!(
        r.warnings.iter().any(|w| w.contains("Camera RAW")),
        "{:?}",
        r.warnings
    );
    let out = Path::new(r.output.as_deref().unwrap());
    assert!(out.extension().unwrap() == "jpg");
    let e = read_exif(out).expect("exif in output");
    assert_eq!(
        ascii_field(&e, exif::Tag::Model).as_deref(),
        Some("Synthetic DNG")
    );
    assert_eq!(orientation_of(&e), Some(1));
    let back = load(out).unwrap();
    assert_eq!((back.image.width(), back.image.height()), (32, 64));

    // Metadata policy applies: StripAll writes no EXIF.
    let opts = ConvertOptions {
        metadata: MetadataPolicy::strip_all(),
        quality: QualityMode::Fixed { quality: 80 },
        output: OutputSpec {
            suffix: "-strip".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let r = convert_file(&rotated, &opts);
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert_eq!(r.quality, Some(80));
    assert!(read_exif(Path::new(r.output.as_deref().unwrap())).is_none());

    // Writing RAW fails cleanly.
    let r = convert_file(
        &rotated,
        &ConvertOptions {
            format: TargetFormat::To {
                format: ImageFormat::Raw,
            },
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Failed);
    assert!(r.message.unwrap().contains("Camera RAW"));

    // Compress with Keep → JPEG too.
    let r = compress_file(&rotated, &CompressOptions::default());
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert_eq!(r.format, Some(ImageFormat::Jpeg));
}

/// Real camera files: `TESSERA_RAW_SAMPLES=/dir cargo test --release -p
/// tessera-imaging --test raw -- --nocapture raw_samples`.
#[test]
fn raw_samples() {
    let Ok(dir) = std::env::var("TESSERA_RAW_SAMPLES") else {
        return;
    };
    let out = tmpdir("raw-samples");
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| ImageFormat::from_path(p) == Some(ImageFormat::Raw))
        .collect();
    entries.sort();
    assert!(!entries.is_empty(), "no RAW files in {dir}");
    for p in entries {
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(
            ImageFormat::detect_path(&p),
            Some(ImageFormat::Raw),
            "{name}"
        );
        let t = Instant::now();
        let info = probe(&p).unwrap_or_else(|e| panic!("{name}: probe: {e}"));
        let t_probe = t.elapsed();
        let t = Instant::now();
        let pv = make_preview(&p, &out.join(format!("{name}-pv.jpg")), 1024, 85)
            .unwrap_or_else(|e| panic!("{name}: preview: {e}"));
        let t_preview = t.elapsed();
        let t = Instant::now();
        let src = load(&p).unwrap_or_else(|e| panic!("{name}: load: {e}"));
        let t_load = t.elapsed();
        let sum = read_summary(&p).unwrap();
        let embedded = {
            let bytes = std::fs::read(&p).unwrap();
            let rs = rawler::rawsource::RawSource::new_from_slice(&bytes);
            let d = rawler::get_decoder(&rs).unwrap();
            let prm = rawler::decoders::RawDecodeParams::default();
            let f = d
                .full_image(&rs, &prm)
                .ok()
                .flatten()
                .map(|i| (i.width(), i.height()));
            let pr = d
                .preview_image(&rs, &prm)
                .ok()
                .flatten()
                .map(|i| (i.width(), i.height()));
            format!("embedded full {f:?} preview {pr:?}")
        };
        println!("  {embedded}");
        println!(
            "{name}: {}x{} o{} probe {:.0} ms, preview {}x{} {:.0} ms, develop {}x{} {:.0} ms; {} {} {:?} {:?} f/{:?} ISO {:?} {:?} mm",
            info.width,
            info.height,
            info.orientation,
            t_probe.as_secs_f64() * 1e3,
            pv.width,
            pv.height,
            t_preview.as_secs_f64() * 1e3,
            src.image.width(),
            src.image.height(),
            t_load.as_secs_f64() * 1e3,
            sum.camera_make.as_deref().unwrap_or("?"),
            sum.camera_model.as_deref().unwrap_or("?"),
            sum.date_taken,
            sum.exposure_time,
            sum.f_number,
            sum.iso,
            sum.focal_length,
        );
        assert_eq!(
            (src.image.width(), src.image.height()),
            (info.width, info.height),
            "{name}: probe dimensions differ from develop"
        );
        assert!(sum.camera_make.is_some(), "{name}: no make");
        let rgb = src.image.to_rgb8();
        let mean: f64 =
            rgb.as_raw().iter().map(|&v| v as f64).sum::<f64>() / rgb.as_raw().len() as f64;
        assert!((20.0..235.0).contains(&mean), "{name}: mean {mean}");
        image::DynamicImage::ImageRgb8(rgb)
            .resize(1024, 1024, image::imageops::FilterType::Triangle)
            .save(out.join(format!("{name}-dev.jpg")))
            .unwrap();
    }
    println!("outputs in {}", out.display());
}
