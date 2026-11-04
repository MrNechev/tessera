mod common;

use std::path::{Path, PathBuf};

use common::*;
use exif::Tag;
use image::DynamicImage;
use tessera_imaging::*;

fn source_jpeg(dir: &Path) -> PathBuf {
    let img = DynamicImage::ImageRgb8(image::imageops::rotate270(&quadrants()));
    let icc = fake_icc("Test RGB");
    let bytes = encode_with(
        &img,
        ImageFormat::Jpeg,
        95,
        Some(&exif_block(6)),
        Some(&icc),
        Some(XMP.as_bytes()),
    );
    write(dir, "src.jpg", &bytes)
}

fn convert(
    src: &Path,
    dir: &Path,
    target: ImageFormat,
    policy: MetadataPolicy,
    quality: QualityMode,
    tag: &str,
) -> PathBuf {
    let opts = ConvertOptions {
        format: TargetFormat::To { format: target },
        quality,
        metadata: policy,
        output: OutputSpec {
            directory: Some(dir.join("out").to_string_lossy().into()),
            suffix: format!("-{tag}"),
            conflict: ConflictPolicy::Overwrite,
        },
        ..Default::default()
    };
    let r = convert_file(src, &opts);
    assert_eq!(
        r.status,
        JobStatus::Done,
        "{target:?} {tag}: {:?}",
        r.message
    );
    PathBuf::from(r.output.unwrap())
}

fn custom(location: bool) -> MetadataPolicy {
    MetadataPolicy {
        mode: MetadataMode::Custom,
        keep_location: location,
        ..MetadataPolicy::default()
    }
}

#[test]
fn source_summary() {
    let dir = tmpdir("meta-summary");
    let src = source_jpeg(&dir);
    let s = read_summary(&src).unwrap();
    assert_eq!(s.camera_make.as_deref(), Some("TestCam"));
    assert_eq!(s.camera_model.as_deref(), Some("TC-1"));
    assert_eq!(s.lens.as_deref(), Some("TestLens 4.3mm"));
    assert_eq!(s.date_taken.as_deref(), Some("2023-07-14T10:20:30"));
    assert_eq!(s.exposure_time.as_deref(), Some("1/125 s"));
    assert_eq!(s.f_number, Some(2.8));
    assert_eq!(s.iso, Some(200));
    assert!((s.gps_latitude.unwrap() - 48.856_667).abs() < 1e-4);
    assert!((s.gps_longitude.unwrap() - 2.350_833).abs() < 1e-4);
    assert_eq!(s.artist.as_deref(), Some("Jane Doe"));
    assert_eq!(s.orientation, 6);
    assert_eq!((s.width, s.height), (64, 32));
    assert!(s.has_exif && s.has_icc && s.has_xmp);
    assert_eq!(s.icc_description.as_deref(), Some("Test RGB"));
    assert!(s.fields.iter().any(|f| f.group == "location"));
    let json = serde_json::to_value(&s).unwrap();
    assert!(json.get("gpsLatitude").is_some() && json.get("iccDescription").is_some());
}

#[test]
fn policy_matrix_reencode() {
    let dir = tmpdir("meta-matrix");
    let src = source_jpeg(&dir);
    let reduce = QualityMode::Reduce { percent: 10 };
    for target in [
        ImageFormat::Jpeg,
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Tiff,
    ] {
        // KeepAll keeps everything, orientation reset to 1.
        let out = convert(
            &src,
            &dir,
            target,
            MetadataPolicy::keep_all(),
            reduce,
            "keep",
        );
        let e = read_exif(&out).unwrap_or_else(|| panic!("{target:?} keep: exif"));
        assert_eq!(
            ascii_field(&e, Tag::Make).as_deref(),
            Some("TestCam"),
            "{target:?}"
        );
        assert_eq!(ascii_field(&e, Tag::Model).as_deref(), Some("TC-1"));
        assert_eq!(
            ascii_field(&e, Tag::DateTimeOriginal).as_deref(),
            Some("2023:07:14 10:20:30")
        );
        assert!(
            e.get_field(Tag::GPSLatitude, exif::In::PRIMARY).is_some(),
            "{target:?} GPS"
        );
        assert_eq!(orientation_of(&e), Some(1));
        let s = read_summary(&out).unwrap();
        assert!(s.has_icc, "{target:?} icc");
        assert!(s.has_xmp, "{target:?} xmp");
        // Decodes fine with the image crate and upright.
        let img = image::open(&out).unwrap_or_else(|e| panic!("{target:?}: {e}"));
        assert_eq!((img.width(), img.height()), (64, 32));
        assert_upright_quadrants(&img.to_rgba8());
        // XMP orientation reset.
        let src_img = load(&out).unwrap();
        let xmp = String::from_utf8(src_img.metadata.xmp.unwrap()).unwrap();
        assert!(xmp.contains("tiff:Orientation=\"1\""), "{xmp}");

        // Custom without location: date + camera stay, GPS gone.
        let out = convert(&src, &dir, target, custom(false), reduce, "nogps");
        let e = read_exif(&out).unwrap();
        assert_eq!(ascii_field(&e, Tag::Make).as_deref(), Some("TestCam"));
        assert_eq!(
            ascii_field(&e, Tag::DateTimeOriginal).as_deref(),
            Some("2023:07:14 10:20:30")
        );
        assert!(
            e.get_field(Tag::GPSLatitude, exif::In::PRIMARY).is_none(),
            "{target:?} GPS must be gone"
        );
        assert!(e.get_field(Tag::MakerNote, exif::In::PRIMARY).is_none());
        assert_eq!(orientation_of(&e), Some(1));
        assert_eq!(
            e.get_field(Tag::PixelXDimension, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0)),
            Some(64)
        );
        let s = read_summary(&out).unwrap();
        assert!(s.has_icc && !s.has_xmp, "{target:?}");

        // Custom keeping only the date.
        let only_date = MetadataPolicy {
            mode: MetadataMode::Custom,
            keep_date_time: true,
            keep_camera: false,
            keep_exposure: false,
            keep_location: false,
            keep_author: false,
            keep_icc: false,
            keep_file_times: false,
        };
        let out = convert(&src, &dir, target, only_date, reduce, "date");
        let e = read_exif(&out).unwrap();
        assert!(ascii_field(&e, Tag::Make).is_none());
        assert!(ascii_field(&e, Tag::Artist).is_none());
        assert!(e.get_field(Tag::ExposureTime, exif::In::PRIMARY).is_none());
        assert_eq!(
            ascii_field(&e, Tag::DateTimeOriginal).as_deref(),
            Some("2023:07:14 10:20:30")
        );
        assert!(!read_summary(&out).unwrap().has_icc);

        // StripAll leaves nothing.
        let out = convert(
            &src,
            &dir,
            target,
            MetadataPolicy::strip_all(),
            reduce,
            "strip",
        );
        let s = read_summary(&out).unwrap();
        assert!(!s.has_exif && !s.has_icc && !s.has_xmp, "{target:?}: {s:?}");
        assert!(read_exif(&out).is_none() || target == ImageFormat::Tiff);
        let img = load(&out).unwrap();
        assert_upright_quadrants(&img.image.to_rgba8());
    }
}

#[test]
fn filter_exif_modes() {
    let raw = exif_block(6);
    let keep = filter_exif(&raw, &MetadataPolicy::keep_all()).unwrap();
    assert_eq!(keep.len(), raw.len(), "KeepAll is a byte copy");
    let diff = keep.iter().zip(&raw).filter(|(a, b)| a != b).count();
    assert!(diff <= 2, "only the orientation value changes");
    let e = exif::Reader::new().read_raw(keep).unwrap();
    assert_eq!(orientation_of(&e), Some(1));
    assert!(e.get_field(Tag::MakerNote, exif::In::PRIMARY).is_some());

    assert!(filter_exif(&raw, &MetadataPolicy::strip_all()).is_none());

    let none = MetadataPolicy {
        mode: MetadataMode::Custom,
        keep_date_time: false,
        keep_camera: false,
        keep_exposure: false,
        keep_location: false,
        keep_author: false,
        ..MetadataPolicy::default()
    };
    assert!(
        filter_exif(&raw, &none).is_none(),
        "nothing but structure left"
    );

    let sized = filter_exif_sized(&raw, &custom(true), 640, 480).unwrap();
    let e = exif::Reader::new().read_raw(sized).unwrap();
    assert_eq!(
        e.get_field(Tag::PixelXDimension, exif::In::PRIMARY)
            .unwrap()
            .value
            .get_uint(0),
        Some(640)
    );
    assert!(e.get_field(Tag::GPSLongitude, exif::In::PRIMARY).is_some());
    assert!(filter_exif(b"garbage", &MetadataPolicy::keep_all()).is_none());
}

#[test]
fn embed_produces_valid_containers() {
    let img = DynamicImage::ImageRgb8(photo_like(80, 60, 9));
    let exif = exif_block(1);
    let icc = fake_icc("Embed");
    for f in [
        ImageFormat::Jpeg,
        ImageFormat::Png,
        ImageFormat::Webp,
        ImageFormat::Tiff,
    ] {
        let bytes = encode_with(&img, f, 90, Some(&exif), Some(&icc), Some(XMP.as_bytes()));
        // Decodable by the image crate (TIFF via our own loader).
        let back = load_bytes(&bytes, Some(f)).unwrap();
        assert_eq!((back.image.width(), back.image.height()), (80, 60));
        assert_eq!(
            back.metadata.icc.as_deref(),
            Some(icc.as_slice()),
            "{f:?} icc"
        );
        assert!(back.metadata.xmp.is_some(), "{f:?} xmp");
        let e = exif::Reader::new()
            .read_from_container(&mut std::io::Cursor::new(&bytes))
            .unwrap_or_else(|err| panic!("{f:?}: {err}"));
        assert_eq!(ascii_field(&e, Tag::Model).as_deref(), Some("TC-1"));
        if f != ImageFormat::Tiff {
            image::load_from_memory(&bytes).unwrap_or_else(|err| panic!("{f:?}: {err}"));
        }
        if f == ImageFormat::Webp {
            assert_eq!(&bytes[12..16], b"VP8X");
            let flags = bytes[20];
            assert_eq!(flags & 0x2C, 0x2C, "ICC+EXIF+XMP flags");
            let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
            assert_eq!(riff + 8, bytes.len());
        }
        // Re-embedding nothing strips it again.
        let (stripped, _) = embed(bytes, f, None, None, None).unwrap();
        let back = load_bytes(&stripped, Some(f)).unwrap();
        assert_eq!(back.metadata, RawMetadata::default(), "{f:?}");
    }
    // Lossless WebP with alpha keeps its alpha flag when made extended.
    let mut rgba = img.to_rgba8();
    rgba.get_pixel_mut(0, 0).0[3] = 0;
    let ll = encode(
        &DynamicImage::ImageRgba8(rgba),
        &EncodeOptions {
            format: ImageFormat::Webp,
            lossless: true,
            ..Default::default()
        },
    )
    .unwrap();
    let (with, _) = embed(ll, ImageFormat::Webp, Some(&exif), None, None).unwrap();
    assert_eq!(with[20] & 0x10, 0x10, "alpha flag");
    assert_eq!(
        load_bytes(&with, None)
            .unwrap()
            .image
            .to_rgba8()
            .get_pixel(0, 0)
            .0[3],
        0
    );

    let (_, warn) = embed(
        encode(
            &img,
            &EncodeOptions {
                format: ImageFormat::Bmp,
                ..Default::default()
            },
        )
        .unwrap(),
        ImageFormat::Bmp,
        Some(&exif),
        None,
        None,
    )
    .unwrap();
    assert_eq!(warn.len(), 1);
}

#[test]
fn file_times_are_copied() {
    let dir = tmpdir("meta-times");
    let src = source_jpeg(&dir);
    let t = filetime::FileTime::from_unix_time(1_000_000_000, 0);
    filetime::set_file_times(&src, t, t).unwrap();
    let out = convert(
        &src,
        &dir,
        ImageFormat::Png,
        MetadataPolicy::keep_all(),
        QualityMode::Original,
        "times",
    );
    let m = std::fs::metadata(&out).unwrap();
    assert_eq!(filetime::FileTime::from_last_modification_time(&m), t);
    let out = convert(
        &src,
        &dir,
        ImageFormat::Png,
        MetadataPolicy::strip_all(),
        QualityMode::Original,
        "notimes",
    );
    let m = std::fs::metadata(&out).unwrap();
    assert_ne!(filetime::FileTime::from_last_modification_time(&m), t);
}
