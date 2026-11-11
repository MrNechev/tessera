//! ICC colour management: embedded non-sRGB profiles are converted to sRGB.
//! Profiles are generated with moxcms, so no fixtures are needed.

mod common;

use std::path::Path;
use std::time::Instant;

use common::*;
use image::{DynamicImage, ImageBuffer, Luma, Rgb, RgbImage};
use moxcms::ColorProfile;
use tessera_imaging::*;

fn p3_icc() -> Vec<u8> {
    ColorProfile::new_display_p3().encode().unwrap()
}

/// 32x16: left half a moderately saturated red, right half neutral grey.
fn swatch() -> RgbImage {
    RgbImage::from_fn(32, 16, |x, _| {
        if x < 16 {
            Rgb([200, 100, 90])
        } else {
            Rgb([128, 128, 128])
        }
    })
}

fn png_with(img: &DynamicImage, icc: Option<&[u8]>) -> Vec<u8> {
    encode_with(img, ImageFormat::Png, 100, None, icc, None)
}

#[test]
fn icc_srgb_detection() {
    assert!(icc_is_srgb(&ColorProfile::new_srgb().encode().unwrap()));
    assert!(!icc_is_srgb(&p3_icc()));
    assert!(!icc_is_srgb(
        &ColorProfile::new_adobe_rgb().encode().unwrap()
    ));
    assert!(!icc_is_srgb(
        &ColorProfile::new_pro_photo_rgb().encode().unwrap()
    ));
    // Unparseable profiles: only the description can tell.
    assert!(icc_is_srgb(&fake_icc("sRGB IEC61966-2.1")));
    assert!(!icc_is_srgb(&fake_icc("Display P3")));
}

#[test]
fn load_converts_display_p3_to_srgb() {
    let dir = tmpdir("color-load");
    let icc = p3_icc();
    let p = write(
        &dir,
        "p3.png",
        &png_with(&DynamicImage::ImageRgb8(swatch()), Some(&icc)),
    );

    let src = load(&p).unwrap();
    assert!(src.color_converted);
    assert!(
        src.metadata.icc.is_none(),
        "profile dropped after conversion"
    );
    let px = src.image.to_rgb8();
    let red = px.get_pixel(4, 4).0;
    // P3 → sRGB makes in-gamut colours more saturated.
    assert!(red[0] > 205 && red[1] < 98 && red[2] < 88, "{red:?}");
    let grey = px.get_pixel(24, 4).0;
    for c in grey {
        assert!((127..=129).contains(&c), "neutral stays neutral: {grey:?}");
    }

    let kept = load_with(
        &p,
        &LoadOptions {
            color: ColorMode::Keep,
        },
    )
    .unwrap();
    assert!(!kept.color_converted);
    assert_eq!(kept.metadata.icc.as_deref(), Some(&icc[..]));
    assert_eq!(kept.image.to_rgb8(), swatch());

    // Previews and scaled loads are sRGB too (JPEG DCT-scaled path as well).
    let rgba = load_scaled_rgba(&p, 0, 0).unwrap();
    assert_eq!(rgba.get_pixel(4, 4).0[..3], red[..]);
    let jpeg = encode_with(
        &DynamicImage::ImageRgb8(swatch()),
        ImageFormat::Jpeg,
        95,
        None,
        Some(&icc),
        None,
    );
    let jp = write(&dir, "p3.jpg", &jpeg);
    let small = load_scaled_rgba(&jp, 16, 16).unwrap();
    let r = small.get_pixel(2, 2).0;
    assert!(r[0] > 205 && r[1] < 100, "{r:?}");

    let sum = read_summary(&p).unwrap();
    assert!(sum.has_icc && !sum.icc_is_srgb && !sum.raw);
}

#[test]
fn srgb_profile_is_left_alone() {
    let dir = tmpdir("color-srgb");
    let icc = ColorProfile::new_srgb().encode().unwrap();
    let p = write(
        &dir,
        "srgb.png",
        &png_with(&DynamicImage::ImageRgb8(swatch()), Some(&icc)),
    );
    let src = load(&p).unwrap();
    assert!(!src.color_converted);
    assert_eq!(src.metadata.icc.as_deref(), Some(&icc[..]));
    assert_eq!(src.image.to_rgb8(), swatch());
    let sum = read_summary(&p).unwrap();
    assert!(sum.has_icc && sum.icc_is_srgb);
    // Untagged counts as sRGB.
    let p = write(
        &dir,
        "plain.png",
        &png_with(&DynamicImage::ImageRgb8(swatch()), None),
    );
    assert!(read_summary(&p).unwrap().icc_is_srgb);
}

#[test]
fn sixteen_bit_alpha_and_grey_paths() {
    // 16-bit RGBA with alpha: converted, alpha untouched.
    let icc = ColorProfile::new_adobe_rgb().encode().unwrap();
    let img = ImageBuffer::<image::Rgba<u16>, _>::from_fn(8, 8, |_, y| {
        image::Rgba([40000, 30000, 20000, if y < 4 { 65535 } else { 1000 }])
    });
    let bytes = png_with(&DynamicImage::ImageRgba16(img.clone()), Some(&icc));
    let src = load_bytes(&bytes, None).unwrap();
    assert!(src.color_converted);
    let DynamicImage::ImageRgba16(out) = &src.image else {
        panic!("expected RGBA16, got {:?}", src.image.color());
    };
    let p = out.get_pixel(0, 6).0;
    assert_eq!(p[3], 1000);
    assert!(p[0] > 40000 && p[2] < 20000, "{p:?}");

    // Linear-gamma grey profile → sRGB grey is brighter in the midtones.
    let grey = ColorProfile::new_gray_with_gamma(1.0).encode().unwrap();
    let g = ImageBuffer::<Luma<u8>, _>::from_pixel(8, 8, Luma([50]));
    let bytes = png_with(&DynamicImage::ImageLuma8(g), Some(&grey));
    let src = load_bytes(&bytes, None).unwrap();
    assert!(src.color_converted);
    let v = src.image.to_luma8().get_pixel(0, 0).0[0];
    assert!(v > 110, "linear 50/255 is ~124 in sRGB, got {v}");
}

#[test]
fn convert_reports_conversion_and_honours_keep() {
    let dir = tmpdir("color-jobs");
    let icc = p3_icc();
    let p = write(
        &dir,
        "p3.png",
        &png_with(&DynamicImage::ImageRgb8(swatch()), Some(&icc)),
    );
    let to_jpeg = TargetFormat::To {
        format: ImageFormat::Jpeg,
    };

    let r = convert_file(
        &p,
        &ConvertOptions {
            format: to_jpeg,
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert!(
        r.warnings
            .iter()
            .any(|w| w.starts_with("Converted from ") && w.ends_with(" to sRGB")),
        "{:?}",
        r.warnings
    );
    let out = Path::new(r.output.as_deref().unwrap());
    let s = read_summary(out).unwrap();
    assert!(!s.has_icc, "converted output carries no profile");
    let px = load(out).unwrap().image.to_rgb8().get_pixel(4, 4).0;
    assert!(px[0] > 203 && px[1] < 100, "{px:?}");

    let r = convert_file(
        &p,
        &ConvertOptions {
            format: to_jpeg,
            color: ColorMode::Keep,
            output: OutputSpec {
                suffix: "-keep".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert!(!r.warnings.iter().any(|w| w.contains("sRGB")));
    let out = Path::new(r.output.as_deref().unwrap());
    assert!(read_summary(out).unwrap().has_icc);

    // Compress (PNG → PNG, lossless) converts too.
    let r = compress_file(
        &p,
        &CompressOptions {
            level: CompressLevel::Lossless,
            keep_if_larger: false,
            metadata: MetadataPolicy::strip_all(),
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert!(r.warnings.iter().any(|w| w.contains("to sRGB")));

    let prev = compress_preview(
        &p,
        &CompressOptions {
            level: CompressLevel::Custom { quality: 90 },
            format: to_jpeg,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(prev.warnings.iter().any(|w| w.contains("to sRGB")));
}

#[test]
fn jpeg_passthrough_respects_colour() {
    let dir = tmpdir("color-passthrough");
    let icc = p3_icc();
    let jpeg = encode_with(
        &DynamicImage::ImageRgb8(swatch()),
        ImageFormat::Jpeg,
        92,
        None,
        Some(&icc),
        None,
    );
    let p = write(&dir, "p3.jpg", &jpeg);
    // Profile kept → pixels can stay untouched (colours remain correct).
    let r = convert_file(&p, &ConvertOptions::default());
    assert_eq!(r.status, JobStatus::Done);
    assert_eq!(
        r.message.as_deref(),
        Some("Copied without re-encoding (lossless)")
    );
    // Profile stripped → must convert, so it is re-encoded.
    let r = convert_file(
        &p,
        &ConvertOptions {
            metadata: MetadataPolicy::strip_all(),
            output: OutputSpec {
                suffix: "-strip".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(r.status, JobStatus::Done, "{:?}", r.message);
    assert_ne!(
        r.message.as_deref(),
        Some("Copied without re-encoding (lossless)")
    );
    assert!(r.warnings.iter().any(|w| w.contains("to sRGB")));
    // ... unless colours are explicitly kept.
    let r = convert_file(
        &p,
        &ConvertOptions {
            metadata: MetadataPolicy::strip_all(),
            color: ColorMode::Keep,
            output: OutputSpec {
                suffix: "-strip-keep".into(),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(
        r.message.as_deref(),
        Some("Copied without re-encoding (lossless)")
    );
}

#[test]
fn colour_serde() {
    assert_eq!(serde_json::to_string(&ColorMode::Srgb).unwrap(), "\"srgb\"");
    assert_eq!(serde_json::to_string(&ColorMode::Keep).unwrap(), "\"keep\"");
    // Old JSON without `color` still deserialises (default sRGB).
    let mut v = serde_json::to_value(ConvertOptions::default()).unwrap();
    assert_eq!(v["color"], "srgb");
    v.as_object_mut().unwrap().remove("color");
    let o: ConvertOptions = serde_json::from_value(v).unwrap();
    assert_eq!(o.color, ColorMode::Srgb);
    let mut v = serde_json::to_value(CompressOptions::default()).unwrap();
    v.as_object_mut().unwrap().remove("color");
    let o: CompressOptions = serde_json::from_value(v).unwrap();
    assert_eq!(o.color, ColorMode::Srgb);
    let s = serde_json::to_value(MetadataSummary::default()).unwrap();
    assert!(s.get("iccIsSrgb").is_some() && s.get("raw").is_some());
}

/// 24 MP 8-bit RGB Display P3 → sRGB.
#[test]
fn conversion_speed() {
    let (w, h) = (6000u32, 4000u32);
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    }));
    let bytes = encode_with(&img, ImageFormat::Png, 100, None, Some(&p3_icc()), None);
    let t = Instant::now();
    let keep = load_bytes_with(
        &bytes,
        None,
        &LoadOptions {
            color: ColorMode::Keep,
        },
    )
    .unwrap();
    let decode = t.elapsed();
    let t = Instant::now();
    let conv = load_bytes(&bytes, None).unwrap();
    let total = t.elapsed();
    assert!(conv.color_converted && !keep.color_converted);
    let cost = total.saturating_sub(decode);
    println!(
        "24 MP P3→sRGB: decode {:.0} ms, conversion ≈ {:.0} ms",
        decode.as_secs_f64() * 1e3,
        cost.as_secs_f64() * 1e3
    );
    if !cfg!(debug_assertions) {
        assert!(cost.as_secs_f64() < 1.0, "conversion too slow: {cost:?}");
    }
}
