//! High-level single-file jobs: convert and compress.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use image::DynamicImage;
use serde::{Deserialize, Serialize};

use crate::color::{self, ColorMode};
use crate::container::{jpeg, png, webp};
use crate::decode::{
    decode_bytes, decode_color, image_has_transparency, load_bytes, read_file, RawMetadata,
};
use crate::encode::{self, Chroma, EncodeOptions};
use crate::error::{Error, Result};
use crate::exif_util;
use crate::format::{self, ImageFormat};
use crate::metadata::{self, embed, MetadataMode, MetadataPolicy};
use crate::quality::ssim;
use crate::resize::{apply_resize, target_dimensions, ResizeMode};
use crate::smart;

/// Output format selection.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetFormat {
    /// Same as the input (HEIC/JPEG XL/camera RAW fall back to JPEG, or PNG
    /// when transparent, since they cannot be written).
    #[default]
    Keep,
    To {
        format: ImageFormat,
    },
}

/// What to do when the output file already exists.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Append " (1)", " (2)", ...
    #[default]
    Rename,
    Overwrite,
    Skip,
}

/// Where and how to name outputs: `<stem><suffix>.<ext>`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpec {
    /// Output directory (created if missing). `None` = next to the input.
    pub directory: Option<String>,
    pub suffix: String,
    pub conflict: ConflictPolicy,
}

/// Quality for conversions.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum QualityMode {
    /// Keep quality at 100%: lossless where the target supports it.
    #[default]
    Original,
    /// Encoder quality = 100 - percent.
    Reduce { percent: u8 },
    /// Explicit encoder quality 1..=100.
    Fixed { quality: u8 },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    pub format: TargetFormat,
    pub quality: QualityMode,
    pub resize: ResizeMode,
    pub metadata: MetadataPolicy,
    pub output: OutputSpec,
    /// Used to flatten transparency for formats without alpha.
    pub background: [u8; 3],
    /// Colour handling of images with a non-sRGB ICC profile.
    #[serde(default)]
    pub color: ColorMode,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        ConvertOptions {
            format: TargetFormat::Keep,
            quality: QualityMode::Original,
            resize: ResizeMode::None,
            metadata: MetadataPolicy::default(),
            output: OutputSpec::default(),
            background: [255, 255, 255],
            color: ColorMode::Srgb,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Done,
    Skipped,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobResult {
    pub input: String,
    pub output: Option<String>,
    pub status: JobStatus,
    pub input_bytes: u64,
    pub output_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub format: Option<ImageFormat>,
    pub quality: Option<u8>,
    pub ssim: Option<f64>,
    pub message: Option<String>,
    pub warnings: Vec<String>,
}

/// Compression level.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompressLevel {
    /// Only lossless optimisations.
    Lossless,
    /// Indistinguishable at 100% zoom.
    VisuallyLossless,
    #[default]
    Balanced,
    Small,
    Custom {
        quality: u8,
    },
    /// Fit in at most `bytes` (quality search, then downscaling).
    TargetSize {
        bytes: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompressOptions {
    pub level: CompressLevel,
    pub format: TargetFormat,
    pub resize: ResizeMode,
    pub metadata: MetadataPolicy,
    pub output: OutputSpec,
    /// If the result is not smaller than the original, keep the original
    /// (with the metadata policy applied when that can be done losslessly).
    pub keep_if_larger: bool,
    /// Colour handling of images with a non-sRGB ICC profile.
    #[serde(default)]
    pub color: ColorMode,
}

impl Default for CompressOptions {
    fn default() -> Self {
        CompressOptions {
            level: CompressLevel::Balanced,
            format: TargetFormat::Keep,
            resize: ResizeMode::None,
            metadata: MetadataPolicy::default(),
            output: OutputSpec {
                suffix: "-compressed".into(),
                ..Default::default()
            },
            keep_if_larger: true,
            color: ColorMode::Srgb,
        }
    }
}

/// In-memory compression result for before/after previews.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompressPreview {
    pub bytes: Vec<u8>,
    pub format: ImageFormat,
    pub quality: Option<u8>,
    pub ssim: Option<f64>,
    pub original_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
    /// True when the original file was kept because nothing smaller was
    /// produced.
    pub kept_original: bool,
}

// ---------------------------------------------------------------------------
// Internals

struct Input {
    bytes: Vec<u8>,
    format: ImageFormat,
}

fn read_input(path: &Path) -> Result<Input> {
    let bytes = read_file(path)?;
    let format = format::resolve(&bytes, path)
        .ok_or_else(|| Error::UnknownFormat(format!(": {}", path.display())))?;
    Ok(Input { bytes, format })
}

struct Produced {
    bytes: Vec<u8>,
    format: ImageFormat,
    width: u32,
    height: u32,
    quality: Option<u8>,
    ssim: Option<f64>,
    warnings: Vec<String>,
    message: Option<String>,
    kept_original: bool,
}

fn resolve_target(
    target: &TargetFormat,
    src: ImageFormat,
    alpha: bool,
) -> Result<(ImageFormat, Option<String>)> {
    match *target {
        TargetFormat::To { format } => {
            if format.can_encode() {
                Ok((format, None))
            } else {
                Err(Error::Unsupported(format!(
                    "{} files can be opened but not written; choose another output format",
                    format.label()
                )))
            }
        }
        TargetFormat::Keep => {
            if src.can_encode() {
                Ok((src, None))
            } else {
                let f = if alpha {
                    ImageFormat::Png
                } else {
                    ImageFormat::Jpeg
                };
                let msg = if src == ImageFormat::Raw {
                    format!(
                        "Camera RAW can't be written; developed and saved as {}",
                        f.label()
                    )
                } else {
                    format!("{} can't be written; saved as {}", src.label(), f.label())
                };
                Ok((f, Some(msg)))
            }
        }
    }
}

/// Target format guess before decoding (for the early Skip check).
fn expected_target(target: &TargetFormat, src: ImageFormat) -> ImageFormat {
    match *target {
        TargetFormat::To { format } => format,
        TargetFormat::Keep if src.can_encode() => src,
        TargetFormat::Keep => ImageFormat::Jpeg,
    }
}

fn wants(target: &TargetFormat, src: ImageFormat, f: ImageFormat) -> bool {
    match *target {
        TargetFormat::Keep => src == f,
        TargetFormat::To { format } => format == f,
    }
}

/// Whether pixels may be passed through untouched (no decode) under
/// `mode`: always with `Keep`; with `Srgb` unless the profile would be
/// converted *and* the policy drops it (the colours would then shift).
fn untouched_ok(md: &RawMetadata, policy: &MetadataPolicy, mode: ColorMode) -> bool {
    mode == ColorMode::Keep
        || policy.keeps_icc()
        || md
            .icc
            .as_deref()
            .is_none_or(|i| !color::needs_conversion(i))
}

fn conversion_warning(from: Option<String>, warnings: &mut Vec<String>) {
    if let Some(from) = from {
        warnings.push(format!("Converted from {from} to sRGB"));
    }
}

fn upright(w: u32, h: u32, o: u16) -> (u32, u32) {
    if (5..=8).contains(&o) {
        (h, w)
    } else {
        (w, h)
    }
}

/// SSIM between the encoded result and its reference (flattened when the
/// format has no alpha).
fn measure(reference: &DynamicImage, encoded: &[u8], f: ImageFormat, bg: [u8; 3]) -> Option<f64> {
    let dec = load_bytes(encoded, Some(f)).ok()?.image.to_rgba8();
    let r = if f.supports_alpha() {
        reference.to_rgba8()
    } else {
        encode::flatten(reference, bg).to_rgba8()
    };
    if r.dimensions() != dec.dimensions() {
        return None;
    }
    Some(ssim(&r, &dec))
}

/// Apply the metadata policy to `image_bytes` (a JPEG/PNG/WebP whose pixels
/// are *not* rotated), taking metadata from `md`. The original orientation is
/// preserved. When `image_bytes` is the original file and the policy is
/// KeepAll it is returned unchanged.
fn apply_policy_unrotated(
    image_bytes: &[u8],
    f: ImageFormat,
    md: &RawMetadata,
    policy: &MetadataPolicy,
    is_original: bool,
) -> Result<Option<Vec<u8>>> {
    if is_original && policy.mode == MetadataMode::KeepAll {
        return Ok(Some(image_bytes.to_vec()));
    }
    let orientation = md
        .exif
        .as_deref()
        .map(exif_util::read_orientation)
        .unwrap_or(1);
    let exif = md
        .exif
        .as_deref()
        .and_then(|e| metadata::filter_exif_inner(e, policy, orientation, None));
    let icc = md.icc.as_deref().filter(|_| policy.keeps_icc());
    let xmp = md.xmp.as_deref().filter(|_| policy.keeps_xmp());
    let keep_other = policy.mode == MetadataMode::KeepAll;
    Ok(Some(match f {
        ImageFormat::Jpeg => {
            let j = jpeg::parse(image_bytes)?;
            jpeg::rewrite(
                &j,
                exif.as_deref(),
                icc,
                xmp,
                jpeg::Keep { other: keep_other },
            )?
        }
        ImageFormat::Png => png::rewrite(
            image_bytes,
            &png::Rewrite {
                exif: exif.as_deref(),
                icc,
                xmp,
                keep_other,
                dpi: None,
            },
        )?,
        ImageFormat::Webp => webp::rewrite(image_bytes, exif.as_deref(), icc, xmp)?,
        _ => {
            return Ok(
                (policy.mode == MetadataMode::KeepAll && is_original).then(|| image_bytes.to_vec())
            );
        }
    }))
}

/// Raw metadata straight from a JPEG/PNG/WebP container.
fn container_metadata(bytes: &[u8], f: ImageFormat) -> Result<(RawMetadata, u32, u32)> {
    Ok(match f {
        ImageFormat::Jpeg => {
            let i = jpeg::parse(bytes)?.info();
            (
                RawMetadata {
                    exif: i.exif,
                    icc: i.icc,
                    xmp: i.xmp,
                },
                i.width,
                i.height,
            )
        }
        ImageFormat::Png => {
            let i = png::info(&png::parse(bytes)?);
            (
                RawMetadata {
                    exif: i.exif,
                    icc: i.icc,
                    xmp: i.xmp,
                },
                i.width,
                i.height,
            )
        }
        ImageFormat::Webp => {
            let i = webp::info(&webp::parse(bytes)?);
            (
                RawMetadata {
                    exif: i.exif,
                    icc: i.icc,
                    xmp: i.xmp,
                },
                i.width,
                i.height,
            )
        }
        _ => return Err(Error::Unsupported("not a container format".into())),
    })
}

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn atomic_write(dst: &Path, bytes: &[u8]) -> Result<()> {
    let dir = dst
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "output".into());
    let tmp = dir.join(format!(
        ".{}.{}-{}.tessera-tmp",
        name,
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let res = (|| -> std::io::Result<()> {
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, dst)
    })();
    if let Err(e) = res {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::file(dst, e));
    }
    Ok(())
}

enum Written {
    Path(PathBuf),
    Skipped(PathBuf),
}

fn output_dir(input: &Path, spec: &OutputSpec) -> PathBuf {
    match spec.directory.as_deref() {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => input.parent().map(Path::to_path_buf).unwrap_or_default(),
    }
}

fn output_base(input: &Path, spec: &OutputSpec) -> String {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image".into());
    format!("{stem}{}", spec.suffix)
}

fn first_candidate(input: &Path, spec: &OutputSpec, ext: &str) -> PathBuf {
    output_dir(input, spec).join(format!("{}.{ext}", output_base(input, spec)))
}

fn write_output(
    input: &Path,
    spec: &OutputSpec,
    ext: &str,
    bytes: &[u8],
    keep_times: bool,
) -> Result<Written> {
    let dir = output_dir(input, spec);
    if !dir.as_os_str().is_empty() {
        std::fs::create_dir_all(&dir).map_err(|e| Error::file(&dir, e))?;
    }
    // Read times first: the output may replace the input.
    let times = if keep_times {
        std::fs::metadata(input).ok().map(|m| {
            (
                filetime::FileTime::from_last_access_time(&m),
                filetime::FileTime::from_last_modification_time(&m),
            )
        })
    } else {
        None
    };
    let base = output_base(input, spec);
    let candidate = dir.join(format!("{base}.{ext}"));
    let reserve = |p: &Path| OpenOptions::new().write(true).create_new(true).open(p);
    let out = match spec.conflict {
        ConflictPolicy::Overwrite => {
            atomic_write(&candidate, bytes)?;
            candidate
        }
        ConflictPolicy::Skip => match reserve(&candidate) {
            Ok(_) => {
                if let Err(e) = atomic_write(&candidate, bytes) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(e);
                }
                candidate
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Ok(Written::Skipped(candidate))
            }
            Err(e) => return Err(Error::file(&candidate, e)),
        },
        ConflictPolicy::Rename => {
            let mut chosen = None;
            for i in 0..10_000u32 {
                let p = if i == 0 {
                    candidate.clone()
                } else {
                    dir.join(format!("{base} ({i}).{ext}"))
                };
                match reserve(&p) {
                    Ok(_) => {
                        chosen = Some(p);
                        break;
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(Error::file(&p, e)),
                }
            }
            let p = chosen.ok_or_else(|| {
                Error::InvalidInput("could not find a free output file name".into())
            })?;
            if let Err(e) = atomic_write(&p, bytes) {
                let _ = std::fs::remove_file(&p);
                return Err(e);
            }
            p
        }
    };
    if let Some((atime, mtime)) = times {
        let _ = filetime::set_file_times(&out, atime, mtime);
    }
    Ok(Written::Path(out))
}

fn base_result(input: &Path, input_bytes: u64) -> JobResult {
    JobResult {
        input: input.to_string_lossy().into_owned(),
        output: None,
        status: JobStatus::Failed,
        input_bytes,
        output_bytes: 0,
        width: 0,
        height: 0,
        format: None,
        quality: None,
        ssim: None,
        message: None,
        warnings: Vec::new(),
    }
}

/// Shared driver: early skip check, produce, write, report. Never panics.
fn run_job(
    input: &Path,
    output: &OutputSpec,
    keep_times: bool,
    expected: impl FnOnce(&Input) -> ImageFormat,
    produce: impl FnOnce(&Input) -> Result<Produced>,
) -> JobResult {
    let input_bytes = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let mut res = base_result(input, input_bytes);
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<JobResult> {
            let inp = read_input(input)?;
            let mut r = base_result(input, inp.bytes.len() as u64);
            if output.conflict == ConflictPolicy::Skip {
                let p = first_candidate(input, output, expected(&inp).extension());
                if p.exists() {
                    r.status = JobStatus::Skipped;
                    r.output = Some(p.to_string_lossy().into_owned());
                    r.message = Some("Output file already exists".into());
                    return Ok(r);
                }
            }
            let p = produce(&inp)?;
            r.width = p.width;
            r.height = p.height;
            r.format = Some(p.format);
            r.quality = p.quality;
            r.ssim = p.ssim;
            r.warnings = p.warnings;
            r.message = p.message;
            match write_output(input, output, p.format.extension(), &p.bytes, keep_times)? {
                Written::Path(out) => {
                    r.status = JobStatus::Done;
                    r.output_bytes = p.bytes.len() as u64;
                    r.output = Some(out.to_string_lossy().into_owned());
                }
                Written::Skipped(out) => {
                    r.status = JobStatus::Skipped;
                    r.output = Some(out.to_string_lossy().into_owned());
                    r.message = Some("Output file already exists".into());
                }
            }
            Ok(r)
        }));
    match outcome {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            res.message = Some(e.to_string());
            res
        }
        Err(_) => {
            res.message = Some("Internal error while processing this image".into());
            res
        }
    }
}

// ---------------------------------------------------------------------------
// Convert

/// Requested quality: `None` = keep 100% (lossless where possible).
fn requested_quality(q: &QualityMode) -> Option<u8> {
    match *q {
        QualityMode::Original => None,
        QualityMode::Reduce { percent: 0 } => None,
        QualityMode::Reduce { percent } => Some(100u8.saturating_sub(percent).clamp(1, 100)),
        QualityMode::Fixed { quality } => Some(quality.clamp(1, 100)),
    }
}

struct Plan {
    opts: EncodeOptions,
    quality: Option<u8>,
    lossy: bool,
}

fn quality_plan(
    target: ImageFormat,
    req: Option<u8>,
    bg: [u8; 3],
    warnings: &mut Vec<String>,
) -> Plan {
    let mut opts = EncodeOptions {
        format: target,
        background: bg,
        ..Default::default()
    };
    match target {
        ImageFormat::Jpeg => {
            match req {
                None => {
                    warnings.push("JPEG is always lossy; using maximum quality".into());
                    opts.quality = 100;
                    opts.chroma = Chroma::S444;
                }
                Some(q) => opts.quality = q,
            }
            Plan {
                quality: Some(opts.quality),
                opts,
                lossy: true,
            }
        }
        ImageFormat::Webp | ImageFormat::Png => match req {
            None | Some(100) => {
                opts.lossless = true;
                opts.quality = 100;
                Plan {
                    opts,
                    quality: None,
                    lossy: false,
                }
            }
            Some(q) => {
                opts.quality = q;
                Plan {
                    opts,
                    quality: Some(q),
                    lossy: true,
                }
            }
        },
        ImageFormat::Avif => {
            match req {
                None => {
                    warnings.push("AVIF: using maximum quality (near-lossless)".into());
                    opts.quality = 100;
                }
                Some(q) => opts.quality = q,
            }
            Plan {
                quality: Some(opts.quality),
                opts,
                lossy: true,
            }
        }
        ImageFormat::Gif => {
            opts.quality = req.unwrap_or(100);
            Plan {
                quality: req,
                opts,
                lossy: true,
            }
        }
        other => {
            if req.is_some() {
                warnings.push(format!(
                    "{} is a lossless format; the quality setting was ignored",
                    other.label()
                ));
            }
            opts.lossless = true;
            opts.quality = 100;
            Plan {
                opts,
                quality: None,
                lossy: false,
            }
        }
    }
}

/// Convert one file. Never panics; failures are reported in the result.
pub fn convert_file(input: &Path, opts: &ConvertOptions) -> JobResult {
    run_job(
        input,
        &opts.output,
        opts.metadata.keeps_file_times(),
        |inp| expected_target(&opts.format, inp.format),
        |inp| convert_core(inp, opts),
    )
}

fn convert_core(inp: &Input, opts: &ConvertOptions) -> Result<Produced> {
    // Lossless pass-through: JPEG -> JPEG at original quality without resizing
    // never re-encodes; only metadata segments are rewritten. The pixels are
    // not rotated, so the original Orientation tag is preserved.
    if inp.format == ImageFormat::Jpeg
        && requested_quality(&opts.quality).is_none()
        && wants(&opts.format, inp.format, ImageFormat::Jpeg)
    {
        let (md, w, h) = container_metadata(&inp.bytes, ImageFormat::Jpeg)?;
        let o = md
            .exif
            .as_deref()
            .map(exif_util::read_orientation)
            .unwrap_or(1);
        let (uw, uh) = upright(w, h, o);
        if target_dimensions(uw, uh, &opts.resize) == (uw, uh)
            && untouched_ok(&md, &opts.metadata, opts.color)
        {
            let bytes =
                apply_policy_unrotated(&inp.bytes, ImageFormat::Jpeg, &md, &opts.metadata, true)?
                    .unwrap_or_else(|| inp.bytes.clone());
            let quality = jpeg::parse(&inp.bytes)?
                .info()
                .luma_qtable
                .map(|t| jpeg::estimate_quality(&t));
            return Ok(Produced {
                bytes,
                format: ImageFormat::Jpeg,
                width: uw,
                height: uh,
                quality,
                ssim: Some(1.0),
                warnings: Vec::new(),
                message: Some("Copied without re-encoding (lossless)".into()),
                kept_original: true,
            });
        }
    }

    let (src, from) = decode_color(&inp.bytes, inp.format, opts.color)?;
    let mut warnings = Vec::new();
    let alpha = image_has_transparency(&src.image);
    let (target, w) = resolve_target(&opts.format, inp.format, alpha)?;
    warnings.extend(w);
    conversion_warning(from, &mut warnings);
    let img = apply_resize(src.image, &opts.resize);
    if target == ImageFormat::Ico && (img.width() > 256 || img.height() > 256) {
        warnings.push("ICO is limited to 256×256; the image was downscaled".into());
    }
    if alpha && !target.supports_alpha() {
        warnings.push(format!(
            "{} has no transparency; transparent areas were filled with the background colour",
            target.label()
        ));
    }
    let plan = quality_plan(
        target,
        requested_quality(&opts.quality),
        opts.background,
        &mut warnings,
    );
    let encoded = encode::encode(&img, &plan.opts)?;
    let (ow, oh) = if target == ImageFormat::Ico {
        crate::resize::fit_dimensions(img.width(), img.height(), 256, 256)
    } else {
        (img.width(), img.height())
    };
    let ssim = if plan.lossy {
        measure(&img, &encoded, target, opts.background)
    } else if target == ImageFormat::Ico && (ow, oh) != (img.width(), img.height()) {
        None
    } else {
        Some(1.0)
    };
    let planned = metadata::plan(&src.metadata, &opts.metadata, (ow, oh));
    let (bytes, w2) = embed(
        encoded,
        target,
        planned.exif.as_deref(),
        planned.icc.as_deref(),
        planned.xmp.as_deref(),
    )?;
    warnings.extend(w2);
    Ok(Produced {
        bytes,
        format: target,
        width: ow,
        height: oh,
        quality: plan.quality,
        ssim,
        warnings,
        message: None,
        kept_original: false,
    })
}

// ---------------------------------------------------------------------------
// Compress

/// Compress one file. Never panics; failures are reported in the result.
pub fn compress_file(input: &Path, opts: &CompressOptions) -> JobResult {
    run_job(
        input,
        &opts.output,
        opts.metadata.keeps_file_times(),
        |inp| expected_target(&opts.format, inp.format),
        |inp| compress_core(inp, opts),
    )
}

/// Run the compression pipeline without writing a file.
pub fn compress_preview(input: &Path, opts: &CompressOptions) -> Result<CompressPreview> {
    let inp = read_input(input)?;
    let p = compress_core(&inp, opts)?;
    Ok(CompressPreview {
        bytes: p.bytes,
        format: p.format,
        quality: p.quality,
        ssim: p.ssim,
        original_bytes: inp.bytes.len() as u64,
        width: p.width,
        height: p.height,
        warnings: p.warnings,
        kept_original: p.kept_original,
    })
}

fn compress_core(inp: &Input, opts: &CompressOptions) -> Result<Produced> {
    let mut produced = compress_inner(inp, opts)?;
    if opts.keep_if_larger && !produced.kept_original {
        if let Some(orig) = original_candidate(inp, opts)? {
            if orig.bytes.len() <= produced.bytes.len() {
                produced = orig;
            }
        }
    }
    Ok(produced)
}

/// The original file (metadata policy applied when possible), as a fallback
/// for `keep_if_larger`. `None` when resizing was requested or the policy
/// can't be applied without re-encoding.
fn original_candidate(inp: &Input, opts: &CompressOptions) -> Result<Option<Produced>> {
    let f = inp.format;
    if !f.can_encode() {
        return Ok(None);
    }
    let (md, w, h, o) = match f {
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Webp => {
            let (md, w, h) = container_metadata(&inp.bytes, f)?;
            let o = md
                .exif
                .as_deref()
                .map(exif_util::read_orientation)
                .unwrap_or(1);
            (md, w, h, o)
        }
        _ => {
            if opts.metadata.mode != MetadataMode::KeepAll {
                return Ok(None);
            }
            let s = decode_bytes(&inp.bytes, f)?;
            (RawMetadata::default(), s.image.width(), s.image.height(), 1)
        }
    };
    let (uw, uh) = upright(w, h, o);
    if target_dimensions(uw, uh, &opts.resize) != (uw, uh)
        || !untouched_ok(&md, &opts.metadata, opts.color)
    {
        return Ok(None);
    }
    let Some(bytes) = apply_policy_unrotated(&inp.bytes, f, &md, &opts.metadata, true)? else {
        return Ok(None);
    };
    let quality = if f == ImageFormat::Jpeg {
        jpeg::parse(&inp.bytes)?
            .info()
            .luma_qtable
            .map(|t| jpeg::estimate_quality(&t))
    } else {
        None
    };
    Ok(Some(Produced {
        bytes,
        format: f,
        width: uw,
        height: uh,
        quality,
        ssim: Some(1.0),
        warnings: Vec::new(),
        message: Some("Already optimized; kept the original".into()),
        kept_original: true,
    }))
}

fn compress_inner(inp: &Input, opts: &CompressOptions) -> Result<Produced> {
    let policy = &opts.metadata;
    let lossless_container = matches!(inp.format, ImageFormat::Jpeg | ImageFormat::Png);
    if opts.level == CompressLevel::Lossless
        && lossless_container
        && wants(&opts.format, inp.format, inp.format)
    {
        let (md, w, h) = container_metadata(&inp.bytes, inp.format)?;
        let o = md
            .exif
            .as_deref()
            .map(exif_util::read_orientation)
            .unwrap_or(1);
        let (uw, uh) = upright(w, h, o);
        if target_dimensions(uw, uh, &opts.resize) == (uw, uh)
            && untouched_ok(&md, policy, opts.color)
        {
            return lossless_optimize(inp, &md, policy, uw, uh);
        }
    }

    let (src, from) = decode_color(&inp.bytes, inp.format, opts.color)?;
    let mut warnings = Vec::new();
    let alpha = image_has_transparency(&src.image);
    let (target, w) = resolve_target(&opts.format, inp.format, alpha)?;
    warnings.extend(w);
    conversion_warning(from, &mut warnings);
    let img = apply_resize(src.image, &opts.resize);
    let bg = [255, 255, 255];
    if alpha && !target.supports_alpha() {
        warnings.push(format!(
            "{} has no transparency; transparent areas were filled with white",
            target.label()
        ));
    }
    let reference = if target.supports_alpha() {
        img.clone()
    } else {
        encode::flatten(&img, bg)
    };
    let lossy_target = matches!(
        target,
        ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Avif
    );

    let mut out_img = reference.clone();
    let (encoded, quality, mut ssim_val): (Vec<u8>, Option<u8>, Option<f64>) = match opts.level {
        CompressLevel::Lossless => {
            let mut wv = Vec::new();
            let plan = quality_plan(target, None, bg, &mut wv);
            warnings.extend(wv);
            let e = encode::encode(&reference, &plan.opts)?;
            (e, plan.quality, (!plan.lossy).then_some(1.0))
        }
        CompressLevel::VisuallyLossless | CompressLevel::Balanced | CompressLevel::Small => {
            let level = match opts.level {
                CompressLevel::VisuallyLossless => smart::VISUALLY_LOSSLESS,
                CompressLevel::Balanced => smart::BALANCED,
                _ => smart::SMALL,
            };
            if lossy_target {
                let r = smart::smart_lossy(&reference, target, &level, bg)?;
                let mut best = (r.bytes, Some(r.quality), r.ssim);
                // A lossless WebP can beat lossy for graphics coming from a
                // lossless source.
                if target == ImageFormat::Webp
                    && inp.format.supports_lossless()
                    && inp.format != ImageFormat::Webp
                {
                    let ll = encode::encode_webp(&reference, true, 100, 60)?;
                    if ll.len() < best.0.len() {
                        best = (ll, None, Some(1.0));
                    }
                }
                best
            } else if target == ImageFormat::Png {
                png_smart(&reference, &level)?
            } else {
                let mut wv = Vec::new();
                let plan = quality_plan(target, None, bg, &mut wv);
                let e = encode::encode(&reference, &plan.opts)?;
                (e, plan.quality, (!plan.lossy).then_some(1.0))
            }
        }
        CompressLevel::Custom { quality } => {
            let q = quality.clamp(1, 100);
            let plan = quality_plan(target, Some(q), bg, &mut warnings);
            let e = encode::encode(&reference, &plan.opts)?;
            (e, plan.quality, (!plan.lossy).then_some(1.0))
        }
        CompressLevel::TargetSize { bytes: budget } => {
            let pre = metadata::plan(
                &src.metadata,
                policy,
                (reference.width(), reference.height()),
            );
            let overhead = pre.exif.as_ref().map_or(0, |v| v.len() + 16)
                + pre.icc.as_ref().map_or(0, |v| v.len() + 32)
                + pre.xmp.as_ref().map_or(0, |v| v.len() + 40)
                + 64;
            let budget_px = budget.saturating_sub(overhead as u64).max(1);
            let r = smart::fit_size(&reference, target, budget_px, bg)?;
            if !r.reached {
                warnings.push(
                    "Could not reach the requested file size; this is the smallest result".into(),
                );
            }
            if (r.image.width(), r.image.height()) != (reference.width(), reference.height()) {
                warnings.push(format!(
                    "Downscaled to {}×{} to reach the requested file size",
                    r.image.width(),
                    r.image.height()
                ));
            }
            out_img = r.image;
            (r.bytes, r.quality, r.quality.is_none().then_some(1.0))
        }
    };
    if target == ImageFormat::Ico {
        let (w, h) = crate::resize::fit_dimensions(out_img.width(), out_img.height(), 256, 256);
        if (w, h) != (out_img.width(), out_img.height()) {
            warnings.push("ICO is limited to 256×256; the image was downscaled".into());
            out_img = crate::resize::resize_exact(out_img, w, h);
        }
    }
    if ssim_val.is_none() {
        ssim_val = measure(&out_img, &encoded, target, bg);
    }
    let (ow, oh) = (out_img.width(), out_img.height());
    let planned = metadata::plan(&src.metadata, policy, (ow, oh));
    let (bytes, w2) = embed(
        encoded,
        target,
        planned.exif.as_deref(),
        planned.icc.as_deref(),
        planned.xmp.as_deref(),
    )?;
    warnings.extend(w2);
    Ok(Produced {
        bytes,
        format: target,
        width: ow,
        height: oh,
        quality,
        ssim: ssim_val,
        warnings,
        message: None,
        kept_original: false,
    })
}

/// PNG smart compression: lossless oxipng, plus a palette candidate that is
/// kept only when it is smaller and passes the level's SSIM threshold.
fn png_smart(
    reference: &DynamicImage,
    level: &smart::Level,
) -> Result<(Vec<u8>, Option<u8>, Option<f64>)> {
    let lossless = encode::png_lossless(reference, 60, None)?;
    let rgba = reference.to_rgba8();
    if let Some(q) = encode::png_quantized(&rgba, 0, level.png_quant, 1.0, 60, None)? {
        if q.len() < lossless.len() {
            if let Some(s) = measure(reference, &q, ImageFormat::Png, [255, 255, 255]) {
                if s >= level.threshold {
                    return Ok((q, Some(level.png_quant), Some(s)));
                }
            }
        }
    }
    Ok((lossless, None, Some(1.0)))
}

/// Lossless optimisation of a JPEG (coefficient transcoding) or PNG (oxipng)
/// without touching pixels; metadata policy applied with the original
/// orientation preserved.
fn lossless_optimize(
    inp: &Input,
    md: &RawMetadata,
    policy: &MetadataPolicy,
    w: u32,
    h: u32,
) -> Result<Produced> {
    let mut warnings = Vec::new();
    let (optimized, quality) = match inp.format {
        ImageFormat::Jpeg => {
            let q = jpeg::parse(&inp.bytes)?
                .info()
                .luma_qtable
                .map(|t| jpeg::estimate_quality(&t));
            match crate::jpegtran::optimize(&inp.bytes, true) {
                Ok(v) => (Some(v), q),
                Err(e) => {
                    warnings.push(format!(
                        "Lossless JPEG optimization failed ({e}); metadata changes only"
                    ));
                    (None, q)
                }
            }
        }
        _ => {
            let v = encode::oxipng_optimize(&inp.bytes, 70);
            (Some(v), None)
        }
    };
    let bytes = match &optimized {
        Some(v) => apply_policy_unrotated(v, inp.format, md, policy, false)?,
        None => apply_policy_unrotated(&inp.bytes, inp.format, md, policy, true)?,
    }
    .unwrap_or_else(|| inp.bytes.clone());
    Ok(Produced {
        bytes,
        format: inp.format,
        width: w,
        height: h,
        quality,
        ssim: Some(1.0),
        warnings,
        message: Some("Optimized losslessly".into()),
        kept_original: false,
    })
}
