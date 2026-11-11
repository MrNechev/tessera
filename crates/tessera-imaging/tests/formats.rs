mod common;

use std::path::Path;

use common::*;
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use tessera_imaging::*;

const ENCODABLE: [ImageFormat; 10] = [
    ImageFormat::Jpeg,
    ImageFormat::Png,
    ImageFormat::Webp,
    ImageFormat::Avif,
    ImageFormat::Tiff,
    ImageFormat::Bmp,
    ImageFormat::Gif,
    ImageFormat::Ico,
    ImageFormat::Qoi,
    ImageFormat::Tga,
];

#[test]
fn extension_mapping() {
    assert_eq!(ImageFormat::from_extension("JPG"), Some(ImageFormat::Jpeg));
    assert_eq!(
        ImageFormat::from_extension(".jpeg"),
        Some(ImageFormat::Jpeg)
    );
    assert_eq!(ImageFormat::from_extension("tif"), Some(ImageFormat::Tiff));
    assert_eq!(ImageFormat::from_extension("HEIF"), Some(ImageFormat::Heic));
    assert_eq!(ImageFormat::from_extension("xyz"), None);
    assert_eq!(
        ImageFormat::from_path(Path::new("/a/b/photo.WebP")),
        Some(ImageFormat::Webp)
    );
    for f in ENCODABLE {
        assert_eq!(ImageFormat::from_extension(f.extension()), Some(f));
        assert!(!f.mime().is_empty());
        assert!(f.can_encode());
    }
    assert!(!ImageFormat::Heic.can_encode());
    assert!(!ImageFormat::Jxl.can_encode());
    assert!(ImageFormat::Jxl.can_decode());
    assert_eq!(ImageFormat::Heic.can_decode(), cfg!(feature = "heif"));
    assert_eq!(input_extensions().contains(&"heic"), cfg!(feature = "heif"));
    assert!(input_extensions().contains(&"jxl"));
}

#[test]
fn serde_names() {
    assert_eq!(
        serde_json::to_string(&ImageFormat::Webp).unwrap(),
        "\"webp\""
    );
    let caps = capabilities();
    assert_eq!(caps.len(), 13);
    let v = serde_json::to_value(&caps[0]).unwrap();
    assert!(v.get("canDecode").is_some() && v.get("canEncode").is_some());
    assert_eq!(serde_json::to_string(&Chroma::S444).unwrap(), "\"s444\"");
    assert_eq!(
        serde_json::to_string(&ResizeMode::LongEdge { pixels: 10 }).unwrap(),
        r#"{"kind":"longEdge","pixels":10}"#
    );
    assert_eq!(
        serde_json::to_string(&TargetFormat::To {
            format: ImageFormat::Png
        })
        .unwrap(),
        r#"{"kind":"to","format":"png"}"#
    );
    assert_eq!(
        serde_json::to_string(&MetadataMode::KeepAll).unwrap(),
        "\"keepAll\""
    );
    assert_eq!(
        serde_json::to_string(&ConflictPolicy::Skip).unwrap(),
        "\"skip\""
    );
    assert_eq!(
        serde_json::to_string(&CompressLevel::TargetSize { bytes: 5 }).unwrap(),
        r#"{"kind":"targetSize","bytes":5}"#
    );
    assert_eq!(
        serde_json::to_string(&QualityMode::Reduce { percent: 20 }).unwrap(),
        r#"{"kind":"reduce","percent":20}"#
    );
    let o: ConvertOptions =
        serde_json::from_str(&serde_json::to_string(&ConvertOptions::default()).unwrap()).unwrap();
    assert_eq!(o, ConvertOptions::default());
}

fn test_image(alpha: bool) -> DynamicImage {
    let base = photo_like(96, 64, 3);
    if alpha {
        let mut rgba = DynamicImage::ImageRgb8(base).to_rgba8();
        for (x, _y, p) in rgba.enumerate_pixels_mut() {
            if x < 20 {
                p.0[3] = 0;
            } else if x < 40 {
                p.0[3] = 128;
            }
        }
        DynamicImage::ImageRgba8(rgba)
    } else {
        DynamicImage::ImageRgb8(base)
    }
}

#[test]
fn detect_and_roundtrip_every_encodable_format() {
    for &f in &ENCODABLE {
        for alpha in [false, true] {
            let img = test_image(alpha);
            let opts = EncodeOptions {
                format: f,
                quality: 92,
                lossless: true,
                ..Default::default()
            };
            let bytes = encode(&img, &opts).unwrap_or_else(|e| panic!("{f:?}: {e}"));
            if f != ImageFormat::Tga {
                assert_eq!(ImageFormat::detect(&bytes), Some(f), "detect {f:?}");
            }
            let src = load_bytes(&bytes, Some(f)).unwrap_or_else(|e| panic!("decode {f:?}: {e}"));
            assert_eq!(src.format, f);
            assert_eq!(src.image.dimensions(), (96, 64), "{f:?}");
            let a = img.to_rgba8();
            let b = src.image.to_rgba8();
            match f {
                ImageFormat::Png
                | ImageFormat::Webp
                | ImageFormat::Tiff
                | ImageFormat::Qoi
                | ImageFormat::Tga
                | ImageFormat::Ico => {
                    assert_eq!(a, b, "{f:?} alpha={alpha} must be lossless");
                }
                ImageFormat::Bmp => {
                    if !alpha {
                        assert_eq!(a, b, "BMP must be lossless");
                    }
                }
                _ => {
                    let s = ssim(&a, &b);
                    assert!(s > 0.80, "{f:?} alpha={alpha}: ssim {s}");
                }
            }
            if alpha && f.supports_alpha() {
                // Fully transparent region stays transparent.
                assert!(b.get_pixel(5, 5).0[3] < 20, "{f:?} lost transparency");
            }
        }
    }
}

#[test]
fn sixteen_bit_png_and_tiff_stay_lossless() {
    let img = DynamicImage::ImageRgb16(image::ImageBuffer::from_fn(40, 30, |x, y| {
        image::Rgb([(x * 1500) as u16, (y * 2000) as u16, 12345])
    }));
    for f in [ImageFormat::Png, ImageFormat::Tiff] {
        let bytes = encode(
            &img,
            &EncodeOptions {
                format: f,
                lossless: true,
                ..Default::default()
            },
        )
        .unwrap();
        let back = load_bytes(&bytes, Some(f)).unwrap().image;
        assert_eq!(back.to_rgb16(), img.to_rgb16(), "{f:?}");
    }
}

#[test]
fn jpeg_progressive_and_baseline() {
    let img = test_image(false);
    let prog = encode(
        &img,
        &EncodeOptions {
            progressive: true,
            ..Default::default()
        },
    )
    .unwrap();
    let base = encode(
        &img,
        &EncodeOptions {
            progressive: false,
            ..Default::default()
        },
    )
    .unwrap();
    let has = |b: &[u8], m: u8| b.windows(2).any(|w| w == [0xFF, m]);
    assert!(has(&prog, 0xC2), "progressive SOF2 expected");
    assert!(
        has(&base, 0xC0) && !has(&base, 0xC2),
        "baseline SOF0 expected"
    );
}

#[test]
fn png_dpi_and_quantization() {
    let img = test_image(false);
    let lossless = encode(
        &img,
        &EncodeOptions {
            format: ImageFormat::Png,
            lossless: true,
            dpi: Some(300),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(lossless.windows(4).any(|w| w == b"pHYs"));
    let q = encode(
        &img,
        &EncodeOptions {
            format: ImageFormat::Png,
            quality: 60,
            ..Default::default()
        },
    )
    .unwrap();
    let dec = load_bytes(&q, None).unwrap().image.to_rgba8();
    let mut colours = std::collections::HashSet::new();
    for p in dec.pixels() {
        colours.insert(p.0);
    }
    assert!(colours.len() <= 256, "{} colours", colours.len());
}

#[test]
fn ico_is_downscaled() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(600, 300, Rgba([1, 2, 3, 255])));
    let bytes = encode(
        &img,
        &EncodeOptions {
            format: ImageFormat::Ico,
            ..Default::default()
        },
    )
    .unwrap();
    let back = load_bytes(&bytes, Some(ImageFormat::Ico)).unwrap();
    assert_eq!(back.image.dimensions(), (256, 128));
}

#[test]
fn heic_and_jxl_cannot_be_encoded() {
    let img = test_image(false);
    for f in [ImageFormat::Heic, ImageFormat::Jxl] {
        let e = encode(
            &img,
            &EncodeOptions {
                format: f,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not written"), "{e}");
    }
}

#[test]
fn pdf_page_size() {
    let img = RgbaImage::from_pixel(100, 50, Rgba([10, 200, 30, 255]));
    let pdf = rgba_to_pdf(&img, 300, 90).unwrap();
    assert!(pdf.starts_with(b"%PDF"));
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("/MediaBox [0 0 24 12]"),
        "{}",
        &text[..text.len().min(600)]
    );
    assert!(text.contains("/DCTDecode"));
    assert!(rgba_to_pdf(&img, 0, 90).is_err());
}

#[test]
fn ssim_behaviour() {
    let a = DynamicImage::ImageRgb8(photo_like(300, 200, 1)).to_rgba8();
    assert!((ssim(&a, &a) - 1.0).abs() < 1e-9);
    let jpeg = encode(
        &DynamicImage::ImageRgba8(a.clone()),
        &EncodeOptions {
            quality: 30,
            ..Default::default()
        },
    )
    .unwrap();
    let b = load_bytes(&jpeg, None).unwrap().image.to_rgba8();
    let s = ssim(&a, &b);
    assert!(s < 0.99 && s > 0.5, "{s}");
    let other = RgbaImage::new(10, 10);
    assert_eq!(ssim(&a, &other), 0.0);
}

#[test]
fn resize_modes() {
    use tessera_imaging::{apply_resize, target_dimensions};
    assert_eq!(
        target_dimensions(4000, 3000, &ResizeMode::Percent { percent: 50.0 }),
        (2000, 1500)
    );
    assert_eq!(
        target_dimensions(4000, 3000, &ResizeMode::Percent { percent: 150.0 }),
        (4000, 3000)
    );
    assert_eq!(
        target_dimensions(
            4000,
            3000,
            &ResizeMode::Fit {
                width: 1000,
                height: 1000
            }
        ),
        (1000, 750)
    );
    assert_eq!(
        target_dimensions(
            4000,
            3000,
            &ResizeMode::Fit {
                width: 0,
                height: 300
            }
        ),
        (400, 300)
    );
    assert_eq!(
        target_dimensions(
            300,
            200,
            &ResizeMode::Fit {
                width: 1000,
                height: 1000
            }
        ),
        (300, 200)
    );
    assert_eq!(
        target_dimensions(3000, 4000, &ResizeMode::LongEdge { pixels: 2000 }),
        (1500, 2000)
    );
    let img = DynamicImage::ImageRgb8(photo_like(400, 300, 2));
    let r = apply_resize(img.clone(), &ResizeMode::LongEdge { pixels: 100 });
    assert_eq!(r.dimensions(), (100, 75));
    let r = apply_resize(img, &ResizeMode::LongEdge { pixels: 1000 });
    assert_eq!(r.dimensions(), (400, 300));
}
