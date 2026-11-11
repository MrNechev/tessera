//! Encoders for every writable format, plus single-page PDF export.

use std::borrow::Cow;
use std::time::Duration;

use image::{DynamicImage, ImageEncoder, RgbImage, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::error::{guard, Error, Result};
use crate::format::ImageFormat;
use crate::resize::{fit_dimensions, resize_exact};

/// JPEG chroma subsampling.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Chroma {
    /// 4:4:4 at quality >= 90, otherwise 4:2:0.
    #[default]
    Auto,
    S444,
    S422,
    S420,
}

/// Encoder settings.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EncodeOptions {
    pub format: ImageFormat,
    /// 1..=100.
    pub quality: u8,
    /// Request lossless output (best effort: JPEG uses q100 4:4:4, AVIF q100).
    pub lossless: bool,
    /// Progressive JPEG.
    pub progressive: bool,
    pub chroma: Chroma,
    /// 0..=100, higher = slower/smaller.
    pub effort: u8,
    /// Pixel density to record (JPEG JFIF, PNG pHYs, TIFF resolution).
    pub dpi: Option<u32>,
    /// Colour used to flatten transparency for formats without alpha.
    pub background: [u8; 3],
}

impl Default for EncodeOptions {
    fn default() -> Self {
        EncodeOptions {
            format: ImageFormat::Jpeg,
            quality: 90,
            lossless: false,
            progressive: true,
            chroma: Chroma::Auto,
            effort: 60,
            dpi: None,
            background: [255, 255, 255],
        }
    }
}

/// Encode `img` according to `opts`. Metadata is not written here; see
/// [`crate::metadata::embed`].
pub fn encode(img: &DynamicImage, opts: &EncodeOptions) -> Result<Vec<u8>> {
    if img.width() == 0 || img.height() == 0 {
        return Err(Error::InvalidInput("image is empty".into()));
    }
    let q = opts.quality.clamp(1, 100);
    let effort = opts.effort.min(100);
    match opts.format {
        ImageFormat::Jpeg => {
            let (q, chroma) = if opts.lossless {
                (100, Chroma::S444)
            } else {
                (q, opts.chroma)
            };
            encode_jpeg(
                img,
                q,
                chroma,
                opts.progressive,
                effort,
                opts.dpi,
                opts.background,
            )
        }
        ImageFormat::Png => {
            if opts.lossless || q >= 100 {
                png_lossless(img, effort, opts.dpi)
            } else {
                let rgba = img.to_rgba8();
                match png_quantized(&rgba, q.saturating_sub(30), q, 1.0, effort, opts.dpi)? {
                    Some(v) => Ok(v),
                    None => png_lossless(img, effort, opts.dpi),
                }
            }
        }
        ImageFormat::Webp => encode_webp(img, opts.lossless, q, effort),
        ImageFormat::Avif => {
            encode_avif(img, if opts.lossless { 100 } else { q }, avif_speed(effort))
        }
        ImageFormat::Tiff => crate::tiff_out::encode(img, effort, opts.dpi),
        ImageFormat::Bmp => encode_simple(img, ImageFormat::Bmp),
        ImageFormat::Qoi => encode_simple(img, ImageFormat::Qoi),
        ImageFormat::Tga => encode_simple(img, ImageFormat::Tga),
        ImageFormat::Ico => {
            let (w, h) = fit_dimensions(img.width(), img.height(), 256, 256);
            let small = resize_exact(img.clone(), w, h);
            encode_simple(&small, ImageFormat::Ico)
        }
        ImageFormat::Gif => encode_gif(img, if opts.lossless { 100 } else { q }),
        ImageFormat::Heic | ImageFormat::Jxl | ImageFormat::Raw => {
            Err(Error::Unsupported(format!(
                "{} files can be opened but not written; choose another output format",
                opts.format.label()
            )))
        }
    }
}

// ---------------------------------------------------------------------------
// Pixel helpers

/// Composite onto `bg`, returning 8-bit RGB.
pub(crate) fn flatten_rgb(img: &DynamicImage, bg: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let [r, g, b, a] = p.0;
        let a = a as u32;
        let inv = 255 - a;
        let mix = |c: u8, k: u8| ((c as u32 * a + k as u32 * inv + 127) / 255) as u8;
        o.0 = [mix(r, bg[0]), mix(g, bg[1]), mix(b, bg[2])];
    }
    out
}

/// Flatten transparency for formats without alpha (keeps grayscale).
pub(crate) fn flatten(img: &DynamicImage, bg: [u8; 3]) -> DynamicImage {
    if img.color().has_alpha() {
        DynamicImage::ImageRgb8(flatten_rgb(img, bg))
    } else {
        img.clone()
    }
}

fn is_gray(img: &DynamicImage) -> bool {
    matches!(
        img,
        DynamicImage::ImageLuma8(_) | DynamicImage::ImageLuma16(_)
    )
}

// ---------------------------------------------------------------------------
// JPEG

pub(crate) fn auto_chroma(q: u8, chroma: Chroma) -> Chroma {
    match chroma {
        Chroma::Auto if q >= 90 => Chroma::S444,
        Chroma::Auto => Chroma::S420,
        c => c,
    }
}

pub(crate) fn encode_jpeg(
    img: &DynamicImage,
    q: u8,
    chroma: Chroma,
    progressive: bool,
    effort: u8,
    dpi: Option<u32>,
    bg: [u8; 3],
) -> Result<Vec<u8>> {
    let chroma = auto_chroma(q, chroma);
    if is_gray(img) {
        let l = img.to_luma8();
        mozjpeg_compress(
            l.as_raw(),
            l.width(),
            l.height(),
            true,
            q,
            chroma,
            progressive,
            effort < 20,
            dpi,
        )
    } else {
        let rgb = flatten_rgb(img, bg);
        mozjpeg_compress(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            false,
            q,
            chroma,
            progressive,
            effort < 20,
            dpi,
        )
    }
}

/// Fast JPEG for previews (libjpeg-turbo defaults, Huffman optimisation,
/// baseline).
pub(crate) fn jpeg_fast(img: &DynamicImage, q: u8) -> Result<Vec<u8>> {
    let chroma = auto_chroma(q, Chroma::Auto);
    if is_gray(img) {
        let l = img.to_luma8();
        mozjpeg_compress(
            l.as_raw(),
            l.width(),
            l.height(),
            true,
            q,
            chroma,
            false,
            true,
            None,
        )
    } else {
        let rgb = flatten_rgb(img, [255, 255, 255]);
        mozjpeg_compress(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            false,
            q,
            chroma,
            false,
            true,
            None,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn mozjpeg_compress(
    pixels: &[u8],
    w: u32,
    h: u32,
    gray: bool,
    q: u8,
    chroma: Chroma,
    progressive: bool,
    fast: bool,
    dpi: Option<u32>,
) -> Result<Vec<u8>> {
    if w > 65_500 || h > 65_500 {
        return Err(Error::encode(
            "JPEG",
            "image is larger than 65500 pixels on a side",
        ));
    }
    guard("JPEG", false, || {
        let cs = if gray {
            mozjpeg::ColorSpace::JCS_GRAYSCALE
        } else {
            mozjpeg::ColorSpace::JCS_RGB
        };
        let mut c = mozjpeg::Compress::new(cs);
        if fast {
            c.set_fastest_defaults();
        }
        c.set_size(w as usize, h as usize);
        c.set_quality(q as f32);
        if !gray {
            let s = match chroma {
                Chroma::S444 | Chroma::Auto => (1, 1),
                Chroma::S422 => (2, 1),
                Chroma::S420 => (2, 2),
            };
            c.set_chroma_sampling_pixel_sizes(s, s);
        }
        if progressive {
            c.set_progressive_mode();
        } else {
            c.set_optimize_scans(false);
        }
        c.set_optimize_coding(true);
        if let Some(d) = dpi {
            let d = d.clamp(1, 65535) as u16;
            c.set_pixel_density(mozjpeg::PixelDensity {
                unit: mozjpeg::PixelDensityUnit::Inches,
                x: d,
                y: d,
            });
        }
        let mut s = c
            .start_compress(Vec::with_capacity((w as usize * h as usize) / 4))
            .map_err(|e| Error::encode("JPEG", e))?;
        s.write_scanlines(pixels)
            .map_err(|e| Error::encode("JPEG", e))?;
        s.finish().map_err(|e| Error::encode("JPEG", e))
    })
}

// ---------------------------------------------------------------------------
// PNG

fn png_layout(img: &DynamicImage) -> Cow<'_, DynamicImage> {
    match img {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_)
        | DynamicImage::ImageLuma16(_)
        | DynamicImage::ImageLumaA16(_)
        | DynamicImage::ImageRgb16(_)
        | DynamicImage::ImageRgba16(_) => Cow::Borrowed(img),
        other if other.color().has_alpha() => {
            Cow::Owned(DynamicImage::ImageRgba16(other.to_rgba16()))
        }
        other => Cow::Owned(DynamicImage::ImageRgb16(other.to_rgb16())),
    }
}

fn png_raw(
    img: &DynamicImage,
    compression: image::codecs::png::CompressionType,
) -> Result<Vec<u8>> {
    let img = png_layout(img);
    let mut buf = Vec::new();
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        &mut buf,
        compression,
        image::codecs::png::FilterType::Adaptive,
    );
    img.write_with_encoder(enc)
        .map_err(|e| Error::encode("PNG", e))?;
    Ok(buf)
}

/// Fast PNG for previews.
pub(crate) fn png_fast(img: &RgbaImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut buf,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|e| Error::encode("PNG", e))?;
    Ok(buf)
}

fn oxipng_preset(effort: u8) -> u8 {
    match effort {
        0..=24 => 1,
        25..=49 => 2,
        50..=74 => 3,
        75..=89 => 4,
        _ => 5,
    }
}

/// Lossless PNG optimisation; returns the input when nothing is gained.
pub(crate) fn oxipng_optimize(data: &[u8], effort: u8) -> Vec<u8> {
    let mut o = oxipng::Options::from_preset(oxipng_preset(effort));
    o.timeout = Some(Duration::from_secs(60));
    match guard("PNG", false, || {
        oxipng::optimize_from_memory(data, &o).map_err(|e| Error::encode("PNG", e))
    }) {
        Ok(v) if v.len() < data.len() => v,
        _ => data.to_vec(),
    }
}

fn with_dpi(png: Vec<u8>, dpi: Option<u32>) -> Result<Vec<u8>> {
    match dpi {
        Some(d) => crate::container::png::rewrite(
            &png,
            &crate::container::png::Rewrite {
                keep_other: true,
                dpi: Some(d),
                ..Default::default()
            },
        ),
        None => Ok(png),
    }
}

pub(crate) fn png_lossless(img: &DynamicImage, effort: u8, dpi: Option<u32>) -> Result<Vec<u8>> {
    let raw = png_raw(img, image::codecs::png::CompressionType::Fast)?;
    with_dpi(oxipng_optimize(&raw, effort), dpi)
}

/// Palette quantisation result.
pub(crate) struct Quantized {
    pub palette: Vec<imagequant::RGBA>,
    pub indices: Vec<u8>,
}

/// Quantise to <= 256 colours with libimagequant. `None` if the minimum
/// quality cannot be reached.
pub(crate) fn quantize(
    rgba: &RgbaImage,
    min_q: u8,
    target_q: u8,
    dither: f32,
    effort: u8,
) -> Result<Option<Quantized>> {
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let speed = (10 - (effort as i32 * 9 / 100)).clamp(1, 10);
    let mut attr = imagequant::new();
    let qe = |e: imagequant::Error| Error::encode("palette", e);
    attr.set_quality(min_q.min(target_q), target_q.max(1))
        .map_err(qe)?;
    attr.set_speed(speed).map_err(qe)?;
    let pixels: Vec<imagequant::RGBA> = rgba
        .pixels()
        .map(|p| imagequant::RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();
    let mut img = attr.new_image(pixels, w, h, 0.0).map_err(qe)?;
    let mut res = match attr.quantize(&mut img) {
        Ok(r) => r,
        Err(imagequant::Error::QualityTooLow) => return Ok(None),
        Err(e) => return Err(qe(e)),
    };
    res.set_dithering_level(dither.clamp(0.0, 1.0))
        .map_err(qe)?;
    let (palette, indices) = res.remapped(&mut img).map_err(qe)?;
    Ok(Some(Quantized { palette, indices }))
}

/// Indexed (palette) PNG via imagequant, then oxipng.
pub(crate) fn png_quantized(
    rgba: &RgbaImage,
    min_q: u8,
    target_q: u8,
    dither: f32,
    effort: u8,
    dpi: Option<u32>,
) -> Result<Option<Vec<u8>>> {
    let Some(qz) = quantize(rgba, min_q, target_q, dither, effort)? else {
        return Ok(None);
    };
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, rgba.width(), rgba.height());
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(png::BitDepth::Eight);
        let pal: Vec<u8> = qz.palette.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
        enc.set_palette(pal);
        if qz.palette.iter().any(|c| c.a != 255) {
            let trns: Vec<u8> = qz.palette.iter().map(|c| c.a).collect();
            enc.set_trns(trns);
        }
        enc.set_compression(png::Compression::Fast);
        let mut wr = enc.write_header().map_err(|e| Error::encode("PNG", e))?;
        wr.write_image_data(&qz.indices)
            .map_err(|e| Error::encode("PNG", e))?;
        wr.finish().map_err(|e| Error::encode("PNG", e))?;
    }
    Ok(Some(with_dpi(oxipng_optimize(&buf, effort), dpi)?))
}

// ---------------------------------------------------------------------------
// WebP

pub(crate) fn encode_webp(
    img: &DynamicImage,
    lossless: bool,
    q: u8,
    effort: u8,
) -> Result<Vec<u8>> {
    if img.width() > 16383 || img.height() > 16383 {
        return Err(Error::encode(
            "WebP",
            "WebP is limited to 16383 pixels on a side",
        ));
    }
    let alpha = img.color().has_alpha();
    let data: Vec<u8> = if alpha {
        img.to_rgba8().into_raw()
    } else {
        img.to_rgb8().into_raw()
    };
    guard("WebP", false, || {
        let enc = if alpha {
            webp::Encoder::from_rgba(&data, img.width(), img.height())
        } else {
            webp::Encoder::from_rgb(&data, img.width(), img.height())
        };
        let mut cfg = webp::WebPConfig::new()
            .map_err(|_| Error::encode("WebP", "could not initialise encoder"))?;
        let method = ((effort as i32 * 6 + 50) / 100).clamp(0, 6);
        cfg.method = method;
        if lossless {
            cfg.lossless = 1;
            cfg.exact = 1;
            // In lossless mode "quality" is compression effort.
            cfg.quality = effort as f32;
        } else {
            cfg.lossless = 0;
            cfg.quality = q as f32;
            cfg.alpha_compression = 1;
            cfg.alpha_quality = 100;
            cfg.use_sharp_yuv = (effort >= 50) as i32;
        }
        let mem = enc
            .encode_advanced(&cfg)
            .map_err(|e| Error::encode("WebP", format!("{e:?}")))?;
        Ok(mem.to_vec())
    })
}

// ---------------------------------------------------------------------------
// AVIF

/// rav1e speed (1 slowest .. 10 fastest) for an effort value.
pub(crate) fn avif_speed(effort: u8) -> u8 {
    (10 - (effort as i32 * 8 / 100)).clamp(2, 10) as u8
}

pub(crate) fn encode_avif(img: &DynamicImage, q: u8, speed: u8) -> Result<Vec<u8>> {
    guard("AVIF", false, || {
        let mut buf = Vec::new();
        let enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut buf, speed, q);
        let res = if img.color().has_alpha() {
            let rgba = img.to_rgba8();
            enc.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
        } else {
            let rgb = img.to_rgb8();
            enc.write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ExtendedColorType::Rgb8,
            )
        };
        res.map_err(|e| Error::encode("AVIF", e))?;
        Ok(buf)
    })
}

// ---------------------------------------------------------------------------
// GIF

fn encode_gif(img: &DynamicImage, q: u8) -> Result<Vec<u8>> {
    let (w, h) = (img.width(), img.height());
    if w > u16::MAX as u32 || h > u16::MAX as u32 {
        return Err(Error::encode(
            "GIF",
            "GIF is limited to 65535 pixels on a side",
        ));
    }
    // GIF transparency is binary.
    let mut rgba = img.to_rgba8();
    for p in rgba.pixels_mut() {
        if p.0[3] < 128 {
            p.0 = [0, 0, 0, 0];
        } else {
            p.0[3] = 255;
        }
    }
    let qz = quantize(&rgba, 0, q, 1.0, 60)?
        .ok_or_else(|| Error::encode("GIF", "palette quantization failed"))?;
    let transparent = qz.palette.iter().position(|c| c.a == 0).map(|i| i as u8);
    let mut pal: Vec<u8> = qz.palette.iter().flat_map(|c| [c.r, c.g, c.b]).collect();
    // GIF palettes need at least 2 entries.
    while pal.len() < 6 {
        pal.extend_from_slice(&[0, 0, 0]);
    }
    let mut out = Vec::new();
    {
        let ge = |e: gif::EncodingError| Error::encode("GIF", e);
        let mut enc = gif::Encoder::new(&mut out, w as u16, h as u16, &[]).map_err(ge)?;
        let frame = gif::Frame {
            width: w as u16,
            height: h as u16,
            palette: Some(pal),
            transparent,
            buffer: Cow::Owned(qz.indices),
            ..Default::default()
        };
        enc.write_frame(&frame).map_err(ge)?;
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// BMP / QOI / TGA / ICO via the image crate

fn encode_simple(img: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
    let label = format.label();
    // PNG-in-ICO must be RGBA for most readers (including the image crate).
    let alpha = img.color().has_alpha() || format == ImageFormat::Ico;
    let (data, color) = if alpha {
        (img.to_rgba8().into_raw(), image::ExtendedColorType::Rgba8)
    } else {
        (img.to_rgb8().into_raw(), image::ExtendedColorType::Rgb8)
    };
    let (w, h) = (img.width(), img.height());
    let mut buf = Vec::new();
    let res = match format {
        ImageFormat::Bmp => {
            image::codecs::bmp::BmpEncoder::new(&mut buf).write_image(&data, w, h, color)
        }
        ImageFormat::Qoi => {
            image::codecs::qoi::QoiEncoder::new(&mut buf).write_image(&data, w, h, color)
        }
        ImageFormat::Tga => {
            image::codecs::tga::TgaEncoder::new(&mut buf).write_image(&data, w, h, color)
        }
        ImageFormat::Ico => {
            image::codecs::ico::IcoEncoder::new(&mut buf).write_image(&data, w, h, color)
        }
        _ => return Err(Error::encode(label, "unsupported")),
    };
    res.map_err(|e| Error::encode(label, e))?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// PDF

/// One-page PDF sized to the image at `dpi` (points = px * 72 / dpi), with
/// the image embedded as a JPEG (DCTDecode), flattened onto white.
pub fn rgba_to_pdf(img: &RgbaImage, dpi: u32, jpeg_quality: u8) -> Result<Vec<u8>> {
    use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref};
    if img.width() == 0 || img.height() == 0 {
        return Err(Error::InvalidInput("image is empty".into()));
    }
    if dpi == 0 {
        return Err(Error::InvalidInput("DPI must be greater than zero".into()));
    }
    let q = jpeg_quality.clamp(1, 100);
    let dynimg = DynamicImage::ImageRgba8(img.clone());
    let jpeg = encode_jpeg(
        &dynimg,
        q,
        Chroma::Auto,
        false,
        60,
        Some(dpi),
        [255, 255, 255],
    )?;
    let (w, h) = (img.width(), img.height());
    let pw = w as f32 * 72.0 / dpi as f32;
    let ph = h as f32 * 72.0 / dpi as f32;

    let catalog_id = Ref::new(1);
    let page_tree_id = Ref::new(2);
    let page_id = Ref::new(3);
    let image_id = Ref::new(4);
    let content_id = Ref::new(5);
    let info_id = Ref::new(6);
    let image_name = Name(b"Im1");

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id).kids([page_id]).count(1);
    let mut page = pdf.page(page_id);
    page.media_box(Rect::new(0.0, 0.0, pw, ph));
    page.parent(page_tree_id);
    page.contents(content_id);
    page.resources().x_objects().pair(image_name, image_id);
    page.finish();

    let mut xobj = pdf.image_xobject(image_id, &jpeg);
    xobj.filter(Filter::DctDecode);
    xobj.width(w as i32);
    xobj.height(h as i32);
    xobj.color_space().device_rgb();
    xobj.bits_per_component(8);
    xobj.finish();

    let mut content = Content::new();
    content.save_state();
    content.transform([pw, 0.0, 0.0, ph, 0.0, 0.0]);
    content.x_object(image_name);
    content.restore_state();
    pdf.stream(content_id, &content.finish());
    pdf.document_info(info_id)
        .producer(pdf_writer::TextStr("Tessera"));
    Ok(pdf.finish())
}
