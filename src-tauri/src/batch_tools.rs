//! Batch resize, watermark and rename.
//!
//! The UI computes every output name (see `src/lib/tools/rename.ts`) and sends
//! one job per file. Files whose pixels don't change are copied (or moved)
//! byte for byte, so a pure rename is lossless.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use image::{DynamicImage, GenericImageView, RgbaImage};
use rayon::prelude::*;
use serde::Deserialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager};
use tessera_imaging::{
    Chroma, ConflictPolicy, EncodeOptions, ImageFormat, JobResult, JobStatus, MetadataMode,
    MetadataPolicy, ResizeMode, TargetFormat,
};

use crate::batch::{cancelled, pool};
use crate::{emit_progress, err, paths, AppState, CmdResult};

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Tl,
    T,
    Tr,
    L,
    C,
    R,
    Bl,
    B,
    Br,
    Tile,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Watermark {
    /// PNG/any image file with the watermark (text watermarks are rendered
    /// by the UI into a temporary PNG).
    pub image: String,
    /// Watermark width as a fraction of the output image width.
    pub size: f32,
    /// 0..1
    pub opacity: f32,
    pub position: Position,
    /// Distance from the edges as a fraction of the shorter side.
    pub margin: f32,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Transfer {
    #[default]
    Copy,
    /// Rename the originals (only for files whose pixels don't change).
    Move,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchOptions {
    pub resize: ResizeMode,
    pub watermark: Option<Watermark>,
    pub format: TargetFormat,
    /// Encoder quality for lossy formats (1–100).
    pub quality: u8,
    pub metadata: MetadataPolicy,
    /// None = next to each original.
    pub directory: Option<String>,
    pub conflict: ConflictPolicy,
    #[serde(default)]
    pub transfer: Transfer,
    #[serde(default = "white")]
    pub background: [u8; 3],
}

fn white() -> [u8; 3] {
    [255, 255, 255]
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchJob {
    pub input: String,
    /// New file name without extension.
    pub name: String,
}

/// Top-left positions of every watermark copy on an image of size (w, h).
pub fn placements(w: u32, h: u32, ww: u32, wh: u32, pos: Position, margin: f32) -> Vec<(i64, i64)> {
    let m = (margin * w.min(h) as f32).round() as i64;
    let (w, h, ww, wh) = (w as i64, h as i64, ww as i64, wh as i64);
    let left = m;
    let cx = (w - ww) / 2;
    let right = w - ww - m;
    let top = m;
    let cy = (h - wh) / 2;
    let bottom = h - wh - m;
    match pos {
        Position::Tl => vec![(left, top)],
        Position::T => vec![(cx, top)],
        Position::Tr => vec![(right, top)],
        Position::L => vec![(left, cy)],
        Position::C => vec![(cx, cy)],
        Position::R => vec![(right, cy)],
        Position::Bl => vec![(left, bottom)],
        Position::B => vec![(cx, bottom)],
        Position::Br => vec![(right, bottom)],
        Position::Tile => {
            // Staggered grid; matches the preview in the UI.
            let sx = ((ww as f32) * 1.6).max(1.0) as i64;
            let sy = ((wh as f32) * 3.0).max(1.0) as i64;
            let mut out = Vec::new();
            let mut row = 0;
            let mut y = -wh / 2;
            while y < h {
                let offset = if row % 2 == 1 { sx / 2 } else { 0 };
                let mut x = -ww / 2 - offset;
                while x < w {
                    out.push((x, y));
                    x += sx;
                }
                y += sy;
                row += 1;
            }
            out
        }
    }
}

/// Scale `mark` to `size` × image width (never taller than 90 % of the
/// image), then alpha-blend it at every placement.
pub fn apply_watermark(img: DynamicImage, mark: &RgbaImage, wm: &Watermark) -> DynamicImage {
    let (w, h) = img.dimensions();
    if mark.width() == 0 || mark.height() == 0 || w == 0 || h == 0 {
        return img;
    }
    let aspect = mark.width() as f32 / mark.height() as f32;
    let mut ww = (wm.size.clamp(0.01, 1.0) * w as f32).round().max(1.0);
    let mut wh = (ww / aspect).round().max(1.0);
    if wh > h as f32 * 0.9 {
        wh = (h as f32 * 0.9).floor().max(1.0);
        ww = (wh * aspect).round().max(1.0);
    }
    let scaled = image::imageops::resize(
        mark,
        ww as u32,
        wh as u32,
        image::imageops::FilterType::Lanczos3,
    );
    let opacity = wm.opacity.clamp(0.0, 1.0);
    let mut base = img.into_rgba8();
    for (ox, oy) in placements(
        w,
        h,
        scaled.width(),
        scaled.height(),
        wm.position,
        wm.margin,
    ) {
        blend(&mut base, &scaled, ox, oy, opacity);
    }
    DynamicImage::ImageRgba8(base)
}

fn blend(base: &mut RgbaImage, top: &RgbaImage, ox: i64, oy: i64, opacity: f32) {
    let (bw, bh) = (base.width() as i64, base.height() as i64);
    for ty in 0..top.height() as i64 {
        let y = oy + ty;
        if y < 0 || y >= bh {
            continue;
        }
        for tx in 0..top.width() as i64 {
            let x = ox + tx;
            if x < 0 || x >= bw {
                continue;
            }
            let t = top.get_pixel(tx as u32, ty as u32).0;
            let a = (t[3] as f32 / 255.0) * opacity;
            if a <= 0.0 {
                continue;
            }
            let p = base.get_pixel_mut(x as u32, y as u32);
            for (dst, src) in p.0.iter_mut().zip(t.iter()).take(3) {
                *dst = (*src as f32 * a + *dst as f32 * (1.0 - a)).round() as u8;
            }
            p.0[3] = ((a + (p.0[3] as f32 / 255.0) * (1.0 - a)) * 255.0).round() as u8;
        }
    }
}

fn result(input: &Path, status: JobStatus, message: Option<String>) -> JobResult {
    JobResult {
        input: input.to_string_lossy().into_owned(),
        output: None,
        status,
        input_bytes: fs::metadata(input).map(|m| m.len()).unwrap_or(0),
        output_bytes: 0,
        width: 0,
        height: 0,
        format: None,
        quality: None,
        ssim: None,
        message,
        warnings: Vec::new(),
    }
}

fn source_format(path: &Path) -> Option<ImageFormat> {
    let head = fs::read(path)
        .ok()
        .map(|b| b.into_iter().take(64).collect::<Vec<u8>>());
    head.as_deref()
        .and_then(ImageFormat::detect)
        .or_else(|| ImageFormat::from_path(path))
}

fn target_path(
    input: &Path,
    name: &str,
    ext: &str,
    opts: &BatchOptions,
    pixels_change: bool,
) -> Option<PathBuf> {
    let dir = match &opts.directory {
        Some(d) => paths::expand(d),
        None => input.parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let candidate = dir.join(format!("{name}.{ext}"));
    if candidate == input {
        // Never replace an original with an edited version unless the user
        // explicitly chose "Replace".
        return if !pixels_change || opts.conflict == ConflictPolicy::Overwrite {
            Some(candidate)
        } else {
            Some(paths::unique_path(&candidate))
        };
    }
    if candidate.exists() {
        match opts.conflict {
            ConflictPolicy::Skip => return None,
            ConflictPolicy::Overwrite => {}
            ConflictPolicy::Rename => return Some(paths::unique_path(&candidate)),
        }
    }
    Some(candidate)
}

fn copy_times(from: &Path, to: &Path) {
    if let Ok(m) = fs::metadata(from) {
        let _ = filetime::set_file_times(
            to,
            filetime::FileTime::from_last_access_time(&m),
            filetime::FileTime::from_last_modification_time(&m),
        );
    }
}

pub fn process(job: &BatchJob, opts: &BatchOptions, mark: Option<&RgbaImage>) -> JobResult {
    let input = Path::new(&job.input);
    let Some(src_fmt) = source_format(input) else {
        return result(
            input,
            JobStatus::Failed,
            Some("Not a supported image".into()),
        );
    };
    let out_fmt = match opts.format {
        TargetFormat::Keep if src_fmt.can_encode() => src_fmt,
        TargetFormat::Keep => ImageFormat::Jpeg,
        TargetFormat::To { format } => format,
    };
    if !out_fmt.can_encode() {
        return result(
            input,
            JobStatus::Failed,
            Some(format!("{} can't be written", out_fmt.label())),
        );
    }
    let pixels_change =
        !matches!(opts.resize, ResizeMode::None) || opts.watermark.is_some() || out_fmt != src_fmt;
    let metadata_unchanged = opts.metadata.mode == MetadataMode::KeepAll;
    let ext = if out_fmt == src_fmt {
        // Keep the original extension's spelling (e.g. .JPG, .jpeg).
        input
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| out_fmt.extension().to_string())
    } else {
        out_fmt.extension().to_string()
    };
    let Some(target) = target_path(input, &job.name, &ext, opts, pixels_change) else {
        return result(
            input,
            JobStatus::Skipped,
            Some("A file with that name already exists".into()),
        );
    };
    let mut res = result(input, JobStatus::Done, None);

    // Lossless path: same pixels, same metadata → copy or move the file.
    if !pixels_change && metadata_unchanged {
        if target == input {
            res.status = JobStatus::Skipped;
            res.message = Some("Name and content unchanged".into());
            return res;
        }
        let outcome = if opts.transfer == Transfer::Move {
            fs::rename(input, &target)
                .or_else(|_| fs::copy(input, &target).and_then(|_| fs::remove_file(input)))
        } else {
            fs::copy(input, &target)
                .map(|_| ())
                .inspect(|_| copy_times(input, &target))
        };
        return match outcome {
            Ok(()) => {
                res.output = Some(target.to_string_lossy().into_owned());
                res.output_bytes = fs::metadata(&target).map(|m| m.len()).unwrap_or(0);
                res.format = Some(src_fmt);
                res.message = Some(if opts.transfer == Transfer::Move {
                    "Renamed".into()
                } else {
                    "Copied (lossless)".into()
                });
                res
            }
            Err(e) => result(input, JobStatus::Failed, Some(e.to_string())),
        };
    }

    let src = match tessera_imaging::load(input) {
        Ok(s) => s,
        Err(e) => return result(input, JobStatus::Failed, Some(e.to_string())),
    };
    let mut img = tessera_imaging::apply_resize(src.image, &opts.resize);
    if let (Some(wm), Some(mark)) = (&opts.watermark, mark) {
        img = apply_watermark(img, mark, wm);
    }
    let (w, h) = img.dimensions();
    let enc = EncodeOptions {
        format: out_fmt,
        quality: opts.quality.clamp(1, 100),
        lossless: opts.quality >= 100,
        progressive: true,
        chroma: Chroma::Auto,
        effort: 50,
        dpi: None,
        background: opts.background,
    };
    let encoded = match tessera_imaging::encode(&img, &enc) {
        Ok(b) => b,
        Err(e) => return result(input, JobStatus::Failed, Some(e.to_string())),
    };
    let exif = src
        .metadata
        .exif
        .as_deref()
        .and_then(|e| tessera_imaging::filter_exif_sized(e, &opts.metadata, w, h));
    let icc = if opts.metadata.keeps_icc() {
        src.metadata.icc.as_deref()
    } else {
        None
    };
    let xmp = if opts.metadata.keeps_xmp() {
        src.metadata.xmp.as_deref()
    } else {
        None
    };
    let (bytes, warnings) =
        match tessera_imaging::embed(encoded, out_fmt, exif.as_deref(), icc, xmp) {
            Ok(v) => v,
            Err(e) => return result(input, JobStatus::Failed, Some(e.to_string())),
        };
    if let Err(e) = paths::write_atomic(&target, &bytes) {
        return result(
            input,
            JobStatus::Failed,
            Some(format!("Could not write {}: {e}", target.display())),
        );
    }
    if opts.metadata.keeps_file_times() {
        copy_times(input, &target);
    }
    if opts.transfer == Transfer::Move {
        res.warnings.push(
            "Originals are only renamed when the pixels don't change; the original was kept."
                .into(),
        );
    }
    res.output = Some(target.to_string_lossy().into_owned());
    res.output_bytes = bytes.len() as u64;
    res.width = w;
    res.height = h;
    res.format = Some(out_fmt);
    res.quality = Some(enc.quality);
    res.warnings.extend(warnings);
    res
}

#[tauri::command]
pub async fn batch_process(
    app: AppHandle,
    jobs: Vec<BatchJob>,
    options: BatchOptions,
) -> CmdResult<Vec<JobResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        let mark = match &options.watermark {
            Some(wm) => Some(
                tessera_imaging::load(Path::new(&wm.image))
                    .map_err(|e| format!("Watermark image: {e}"))?
                    .image
                    .into_rgba8(),
            ),
            None => None,
        };
        let state = app.state::<AppState>();
        state.cancel_batch.store(false, Ordering::SeqCst);
        let total = jobs.len();
        let done = AtomicUsize::new(0);
        emit_progress(&app, "batch", 0, total, None);
        Ok(pool().install(|| {
            jobs.par_iter()
                .map(|job| {
                    let r = if state.cancel_batch.load(Ordering::SeqCst) {
                        cancelled(Path::new(&job.input))
                    } else {
                        process(job, &options, mark.as_ref())
                    };
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    emit_progress(&app, "batch", n, total, Some(job.input.clone()));
                    r
                })
                .collect()
        }))
    })
    .await
    .map_err(err)?
}

/// Store a PNG made by the UI (a text watermark) and return its path.
#[tauri::command]
pub fn save_temp_image(app: AppHandle, request: Request<'_>) -> CmdResult<String> {
    let InvokeBody::Raw(data) = request.body() else {
        return Err("image must be sent as raw bytes".into());
    };
    let dir = paths::cache_dir(&app).join("watermarks");
    fs::create_dir_all(&dir).map_err(err)?;
    let name = blake3::hash(data).to_hex()[..16].to_string();
    let path = dir.join(format!("{name}.png"));
    paths::write_atomic(&path, data).map_err(err)?;
    Ok(path.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_placements_respect_margins() {
        let p = placements(1000, 500, 100, 50, Position::Br, 0.1);
        assert_eq!(p, vec![(1000 - 100 - 50, 500 - 50 - 50)]);
        let c = placements(1000, 500, 100, 50, Position::C, 0.0);
        assert_eq!(c, vec![(450, 225)]);
    }

    #[test]
    fn tiles_cover_the_image() {
        let p = placements(1000, 1000, 100, 40, Position::Tile, 0.0);
        assert!(p.len() > 20);
        assert!(p.iter().any(|&(x, y)| x <= 0 && y <= 0));
        assert!(p.iter().any(|&(x, y)| x + 100 >= 1000 && y + 40 >= 900));
    }

    #[test]
    fn watermark_blends_with_opacity() {
        let base =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(200, 100, image::Rgba([0, 0, 0, 255])));
        let mark = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        let wm = Watermark {
            image: String::new(),
            size: 0.5,
            opacity: 0.5,
            position: Position::C,
            margin: 0.0,
        };
        let out = apply_watermark(base, &mark, &wm).into_rgba8();
        // 100 px wide square watermark centred: (50..150, 0..100) clipped to 90 % height → 90 px.
        let centre = out.get_pixel(100, 50).0;
        assert!((centre[0] as i32 - 128).abs() <= 1, "{centre:?}");
        assert_eq!(out.get_pixel(5, 50).0[0], 0);
    }
}
