//! Shared helpers for integration tests: synthetic images, EXIF blocks and
//! temporary directories. No fixture files are needed.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use tessera_imaging::{embed, encode, EncodeOptions, ImageFormat};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Fresh empty directory under the system temp dir.
pub fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir()
        .join("tessera-imaging-tests")
        .join(format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 16) as u32
    }
}

/// Photo-like content: smooth gradients, fine texture/noise, hard edges and
/// some saturated colour detail.
pub fn photo_like(w: u32, h: u32, seed: u64) -> RgbImage {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    // Mild sensor-like grain (photos rarely have strong white noise).
    let noise: Vec<i32> = (0..(w * h)).map(|_| (rng.next() % 7) as i32 - 3).collect();
    RgbImage::from_fn(w, h, |x, y| {
        let fx = x as f32 / w as f32;
        let fy = y as f32 / h as f32;
        // Sky-like gradient top, textured "ground" bottom.
        let mut r = 90.0 + 120.0 * fy + 20.0 * (fx * 12.0).sin();
        let mut g = 140.0 + 60.0 * (1.0 - fy) + 25.0 * (fy * 9.0 + fx * 3.0).cos();
        let mut b = 200.0 - 120.0 * fy;
        if fy > 0.55 {
            let t = ((x / 3 + y / 2) % 7) as f32 * 6.0;
            r = 80.0 + t + 40.0 * (fx * 40.0).sin().abs();
            g = 110.0 + t * 0.7;
            b = 60.0 + t * 0.4;
        }
        // A few hard-edged shapes.
        let cx = fx - 0.3;
        let cy = fy - 0.35;
        if cx * cx + cy * cy < 0.012 {
            r = 230.0;
            g = 40.0;
            b = 40.0;
        }
        if (0.6..0.8).contains(&fx) && (0.2..0.45).contains(&fy) {
            r = 30.0;
            g = 30.0;
            b = 35.0 + 100.0 * ((x / 4) % 2) as f32;
        }
        let n = noise[(y * w + x) as usize] as f32;
        let c = |v: f32| (v + n).clamp(0.0, 255.0) as u8;
        Rgb([c(r), c(g), c(b)])
    })
}

/// 64x32 upright reference with four solid quadrants (TL red, TR green, BL
/// blue, BR white) to detect rotations/flips.
pub fn quadrants() -> RgbImage {
    RgbImage::from_fn(64, 32, |x, y| match (x < 32, y < 16) {
        (true, true) => Rgb([220, 20, 20]),
        (false, true) => Rgb([20, 200, 20]),
        (true, false) => Rgb([20, 20, 220]),
        (false, false) => Rgb([250, 250, 250]),
    })
}

/// Classify a pixel as one of the quadrant colours.
pub fn colour_name(p: &Rgba<u8>) -> &'static str {
    let [r, g, b, _] = p.0;
    if r > 180 && g > 180 && b > 180 {
        "white"
    } else if r > 150 && g < 100 && b < 100 {
        "red"
    } else if g > 150 && r < 100 && b < 100 {
        "green"
    } else if b > 150 && r < 100 && g < 100 {
        "blue"
    } else {
        "?"
    }
}

/// Assert an RGBA image shows the quadrant pattern upright.
pub fn assert_upright_quadrants(img: &RgbaImage) {
    let (w, h) = img.dimensions();
    assert!(w > h, "expected landscape, got {w}x{h}");
    let at = |fx: f32, fy: f32| {
        colour_name(img.get_pixel((fx * w as f32) as u32, (fy * h as f32) as u32))
    };
    assert_eq!(at(0.25, 0.25), "red", "top-left");
    assert_eq!(at(0.75, 0.25), "green", "top-right");
    assert_eq!(at(0.25, 0.75), "blue", "bottom-left");
    assert_eq!(at(0.75, 0.75), "white", "bottom-right");
}

fn ascii(tag: Tag, s: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![s.as_bytes().to_vec()]),
    }
}

fn rat(tag: Tag, v: &[(u32, u32)]) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Rational(
            v.iter()
                .map(|&(num, denom)| Rational { num, denom })
                .collect(),
        ),
    }
}

/// A realistic EXIF block (big-endian) with camera, date, exposure, GPS and
/// author fields plus a fake MakerNote.
pub fn exif_block(orientation: u16) -> Vec<u8> {
    let fields = vec![
        ascii(Tag::Make, "TestCam"),
        ascii(Tag::Model, "TC-1"),
        ascii(Tag::Software, "FirmWare 1.0"),
        ascii(Tag::DateTime, "2023:07:14 12:00:00"),
        ascii(Tag::Artist, "Jane Doe"),
        ascii(Tag::Copyright, "(c) Jane Doe"),
        Field {
            tag: Tag::Orientation,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![orientation]),
        },
        ascii(Tag::DateTimeOriginal, "2023:07:14 10:20:30"),
        ascii(Tag::DateTimeDigitized, "2023:07:14 10:20:30"),
        rat(Tag::ExposureTime, &[(1, 125)]),
        rat(Tag::FNumber, &[(28, 10)]),
        Field {
            tag: Tag::PhotographicSensitivity,
            ifd_num: In::PRIMARY,
            value: Value::Short(vec![200]),
        },
        rat(Tag::FocalLength, &[(43, 10)]),
        ascii(Tag::LensModel, "TestLens 4.3mm"),
        Field {
            tag: Tag::ExifVersion,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(b"0232".to_vec(), 0),
        },
        Field {
            tag: Tag::MakerNote,
            ifd_num: In::PRIMARY,
            value: Value::Undefined(b"TESTMAKERNOTE\0\x01\x02".to_vec(), 0),
        },
        Field {
            tag: Tag::PixelXDimension,
            ifd_num: In::PRIMARY,
            value: Value::Long(vec![1234]),
        },
        Field {
            tag: Tag::PixelYDimension,
            ifd_num: In::PRIMARY,
            value: Value::Long(vec![567]),
        },
        ascii(Tag::GPSLatitudeRef, "N"),
        rat(Tag::GPSLatitude, &[(48, 1), (51, 1), (2400, 100)]),
        ascii(Tag::GPSLongitudeRef, "E"),
        rat(Tag::GPSLongitude, &[(2, 1), (21, 1), (300, 100)]),
    ];
    let mut w = Writer::new();
    for f in &fields {
        w.push_field(f);
    }
    let mut cur = std::io::Cursor::new(Vec::new());
    w.write(&mut cur, false).unwrap();
    cur.into_inner()
}

/// Minimal ICC v2 profile header with a 'desc' tag (not a functional
/// profile; enough for metadata plumbing tests).
pub fn fake_icc(desc: &str) -> Vec<u8> {
    let mut tag = Vec::new();
    tag.extend_from_slice(b"desc");
    tag.extend_from_slice(&[0, 0, 0, 0]);
    tag.extend_from_slice(&((desc.len() + 1) as u32).to_be_bytes());
    tag.extend_from_slice(desc.as_bytes());
    tag.push(0);
    while tag.len() % 4 != 0 {
        tag.push(0);
    }
    let tag_off = 128 + 4 + 12;
    let total = tag_off + tag.len();
    let mut icc = vec![0u8; 128];
    icc[0..4].copy_from_slice(&(total as u32).to_be_bytes());
    icc[8] = 2; // version 2.x
    icc[12..16].copy_from_slice(b"mntr");
    icc[16..20].copy_from_slice(b"RGB ");
    icc[20..24].copy_from_slice(b"XYZ ");
    icc[36..40].copy_from_slice(b"acsp");
    icc.extend_from_slice(&1u32.to_be_bytes());
    icc.extend_from_slice(b"desc");
    icc.extend_from_slice(&(tag_off as u32).to_be_bytes());
    icc.extend_from_slice(&(tag.len() as u32).to_be_bytes());
    icc.extend_from_slice(&tag);
    icc
}

pub const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/" tiff:Orientation="6"/></rdf:RDF></x:xmpmeta>"#;

/// Encode `img` to `format` and attach metadata.
pub fn encode_with(
    img: &DynamicImage,
    format: ImageFormat,
    quality: u8,
    exif: Option<&[u8]>,
    icc: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Vec<u8> {
    let opts = EncodeOptions {
        format,
        quality,
        ..Default::default()
    };
    let bytes = encode(img, &opts).unwrap();
    embed(bytes, format, exif, icc, xmp).unwrap().0
}

pub fn write(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// Read EXIF from any container kamadak-exif understands.
pub fn read_exif(path: &Path) -> Option<exif::Exif> {
    let f = std::fs::File::open(path).ok()?;
    exif::Reader::new()
        .read_from_container(&mut std::io::BufReader::new(f))
        .ok()
}

pub fn ascii_field(e: &exif::Exif, tag: Tag) -> Option<String> {
    match &e.get_field(tag, In::PRIMARY)?.value {
        Value::Ascii(v) => v.first().map(|s| String::from_utf8_lossy(s).into_owned()),
        _ => None,
    }
}

pub fn orientation_of(e: &exif::Exif) -> Option<u32> {
    e.get_field(Tag::Orientation, In::PRIMARY)?
        .value
        .get_uint(0)
}
