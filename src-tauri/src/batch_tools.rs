//! Batch resize, watermark and rename.
//!
//! The UI computes every output name (see `src/lib/tools/rename.ts`) and sends
//! one job per file. Files whose pixels don't change are copied (or moved)
//! byte for byte, so a pure rename is lossless.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::ErrorKind;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

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

/// Content-sniffed format; TIFF-based camera RAW files (NEF, DNG, ARW…)
/// are reported as RAW rather than TIFF.
fn source_format(path: &Path) -> Option<ImageFormat> {
    ImageFormat::detect_path(path)
}

/// Output paths claimed by the jobs of one `batch_process` run.
///
/// Jobs run in parallel and pick their output before the slow decode and
/// encode, so the choice is coordinated here: two jobs never write the same
/// file, and no job writes over (or moves a file onto) another job's input.
pub struct Claims {
    inputs: HashSet<PathBuf>,
    claimed: Mutex<HashSet<PathBuf>>,
}

impl Claims {
    pub fn new(jobs: &[BatchJob]) -> Self {
        Claims {
            inputs: jobs
                .iter()
                .map(|j| normalize(Path::new(&j.input)))
                .collect(),
            claimed: Mutex::default(),
        }
    }
}

fn dir_or_dot(dir: &Path) -> &Path {
    if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    }
}

/// `path` with its directory canonicalised (the file itself may not exist).
fn normalize(path: &Path) -> PathBuf {
    let dir = dir_or_dot(path.parent().unwrap_or(Path::new("")));
    match (fs::canonicalize(dir), path.file_name()) {
        (Ok(d), Some(n)) => d.join(n),
        _ => path.to_path_buf(),
    }
}

/// A chosen output path. When an empty placeholder file was created to
/// reserve the name, it is removed again on drop (error, cancellation,
/// panic) unless [`Reserved::commit`] was called after the output was
/// written.
struct Reserved {
    path: PathBuf,
    placeholder: bool,
    /// The output is the input file itself.
    in_place: bool,
}

impl Reserved {
    fn new(path: PathBuf, placeholder: bool) -> Self {
        Reserved {
            path,
            placeholder,
            in_place: false,
        }
    }

    fn commit(&mut self) {
        self.placeholder = false;
    }
}

impl Drop for Reserved {
    fn drop(&mut self) {
        if self.placeholder {
            let _ = fs::remove_file(&self.path);
        }
    }
}

enum Target {
    Path(Reserved),
    Skip(&'static str),
    Fail(String),
}

const EXISTS: &str = "A file with that name already exists";
const SAME_NAME: &str = "Another file in this batch has the same output name";

/// Choose and reserve the output path of one job. Free names are reserved
/// atomically with an empty placeholder file (`create_new`), so neither
/// parallel jobs nor other programs can take them in the meantime.
fn reserve_target(
    input: &Path,
    name: &str,
    ext: &str,
    opts: &BatchOptions,
    pixels_change: bool,
    claims: &Claims,
) -> Target {
    let dir = match &opts.directory {
        Some(d) => paths::expand(d),
        None => input.parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    if let Err(e) = fs::create_dir_all(dir_or_dot(&dir)) {
        return Target::Fail(format!("Could not create {}: {e}", dir.display()));
    }
    let dir_n = fs::canonicalize(dir_or_dot(&dir)).unwrap_or_else(|_| dir.clone());
    let me = normalize(input);
    let file = |i: u32| {
        if i == 0 {
            format!("{name}.{ext}")
        } else {
            format!("{name} ({i}).{ext}")
        }
    };
    let candidate = dir.join(file(0));
    let cand_n = dir_n.join(file(0));
    let mut claimed = claims
        .claimed
        .lock()
        .unwrap_or_else(PoisonError::into_inner);

    // "Keep both": the first name that no job of this batch uses and that
    // doesn't exist on disk.
    let keep_both = |claimed: &mut HashSet<PathBuf>| -> Target {
        for i in 0..10_000u32 {
            let n = dir_n.join(file(i));
            if claimed.contains(&n) || claims.inputs.contains(&n) {
                continue;
            }
            let p = dir.join(file(i));
            match OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(_) => {
                    claimed.insert(n);
                    return Target::Path(Reserved::new(p, true));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                Err(e) => return Target::Fail(format!("Could not create {}: {e}", p.display())),
            }
        }
        Target::Fail("Could not find a free output file name".into())
    };

    if cand_n == me {
        // Never replace an original with an edited version unless the user
        // explicitly chose "Replace".
        if !pixels_change || opts.conflict == ConflictPolicy::Overwrite {
            if !claimed.insert(cand_n) {
                return Target::Fail(SAME_NAME.into());
            }
            let mut r = Reserved::new(candidate, false);
            r.in_place = true;
            return Target::Path(r);
        }
        return keep_both(&mut claimed);
    }
    if claimed.contains(&cand_n) {
        return match opts.conflict {
            ConflictPolicy::Rename => keep_both(&mut claimed),
            ConflictPolicy::Overwrite | ConflictPolicy::Skip => Target::Fail(SAME_NAME.into()),
        };
    }
    if claims.inputs.contains(&cand_n) {
        // Another original of this batch, possibly not processed yet: never
        // write over it or rename a file onto it (e.g. swapping 1.jpg and
        // 2.jpg would otherwise lose one of them).
        return match opts.conflict {
            ConflictPolicy::Skip => Target::Skip(EXISTS),
            ConflictPolicy::Rename | ConflictPolicy::Overwrite => keep_both(&mut claimed),
        };
    }
    match opts.conflict {
        ConflictPolicy::Overwrite => {
            claimed.insert(cand_n);
            Target::Path(Reserved::new(candidate, false))
        }
        ConflictPolicy::Skip => match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => {
                claimed.insert(cand_n);
                Target::Path(Reserved::new(candidate, true))
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => Target::Skip(EXISTS),
            Err(e) => Target::Fail(format!("Could not create {}: {e}", candidate.display())),
        },
        ConflictPolicy::Rename => keep_both(&mut claimed),
    }
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

pub fn process(
    job: &BatchJob,
    opts: &BatchOptions,
    mark: Option<&RgbaImage>,
    claims: &Claims,
    cancel: &AtomicBool,
) -> JobResult {
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
    let mut reserved = match reserve_target(input, &job.name, &ext, opts, pixels_change, claims) {
        Target::Path(r) => r,
        Target::Skip(msg) => return result(input, JobStatus::Skipped, Some(msg.into())),
        Target::Fail(msg) => return result(input, JobStatus::Failed, Some(msg)),
    };
    let target = reserved.path.clone();
    let mut res = result(input, JobStatus::Done, None);

    // Lossless path: same pixels, same metadata → copy or move the file.
    if !pixels_change && metadata_unchanged {
        if reserved.in_place {
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
                reserved.commit();
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
    // Same plan as convert/compress: EXIF and XMP orientation reset (the
    // pixels are upright), new pixel size, ICC only when it fits the output.
    let planned = tessera_imaging::plan_metadata(&src.metadata, &opts.metadata, (w, h));
    let (bytes, warnings) = match tessera_imaging::embed(
        encoded,
        out_fmt,
        planned.exif.as_deref(),
        planned.icc.as_deref(),
        planned.xmp.as_deref(),
    ) {
        Ok(v) => v,
        Err(e) => return result(input, JobStatus::Failed, Some(e.to_string())),
    };
    if cancel.load(Ordering::SeqCst) {
        // Dropping the reservation removes the placeholder.
        return cancelled(input);
    }
    if let Err(e) = paths::write_atomic(&target, &bytes) {
        return result(
            input,
            JobStatus::Failed,
            Some(format!("Could not write {}: {e}", target.display())),
        );
    }
    reserved.commit();
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

/// [`process`], turning a panic (a decoder bug on a malformed file) into a
/// failed result instead of aborting the whole batch.
fn process_guarded(
    job: &BatchJob,
    opts: &BatchOptions,
    mark: Option<&RgbaImage>,
    claims: &Claims,
    cancel: &AtomicBool,
) -> JobResult {
    catch_unwind(AssertUnwindSafe(|| {
        process(job, opts, mark, claims, cancel)
    }))
    .unwrap_or_else(|_| {
        result(
            Path::new(&job.input),
            JobStatus::Failed,
            Some("Internal error while processing this image".into()),
        )
    })
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
        let claims = Claims::new(&jobs);
        emit_progress(&app, "batch", 0, total, None);
        Ok(pool().install(|| {
            jobs.par_iter()
                .map(|job| {
                    let r = if state.cancel_batch.load(Ordering::SeqCst) {
                        cancelled(Path::new(&job.input))
                    } else {
                        process_guarded(job, &options, mark.as_ref(), &claims, &state.cancel_batch)
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
pub async fn save_temp_image(app: AppHandle, request: Request<'_>) -> CmdResult<String> {
    let InvokeBody::Raw(data) = request.body() else {
        return Err("image must be sent as raw bytes".into());
    };
    let data = data.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = paths::cache_dir(&app).join("watermarks");
        fs::create_dir_all(&dir).map_err(err)?;
        let name = blake3::hash(&data).to_hex()[..16].to_string();
        let path = dir.join(format!("{name}.png"));
        paths::write_atomic(&path, &data).map_err(err)?;
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("tessera-batch-tools-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn image_file(path: &Path, format: ImageFormat, shade: u8) -> PathBuf {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            16,
            12,
            image::Rgba([shade, 100, 50, 255]),
        ));
        let opts = EncodeOptions {
            format,
            quality: 90,
            ..Default::default()
        };
        fs::write(path, tessera_imaging::encode(&img, &opts).unwrap()).unwrap();
        path.to_path_buf()
    }

    fn options(conflict: ConflictPolicy, directory: Option<&Path>) -> BatchOptions {
        BatchOptions {
            resize: ResizeMode::Percent { percent: 50.0 },
            watermark: None,
            format: TargetFormat::To {
                format: ImageFormat::Jpeg,
            },
            quality: 85,
            metadata: MetadataPolicy::keep_all(),
            directory: directory.map(|d| d.to_string_lossy().into_owned()),
            conflict,
            transfer: Transfer::Copy,
            background: white(),
        }
    }

    fn job(input: &Path, name: &str) -> BatchJob {
        BatchJob {
            input: input.to_string_lossy().into_owned(),
            name: name.into(),
        }
    }

    fn run(jobs: &[BatchJob], opts: &BatchOptions) -> Vec<JobResult> {
        let claims = Claims::new(jobs);
        let cancel = AtomicBool::new(false);
        jobs.par_iter()
            .map(|j| process_guarded(j, opts, None, &claims, &cancel))
            .collect()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn same_output_name_keeps_both() {
        let dir = tmpdir("keep-both");
        let a = image_file(&dir.join("IMG_0001.png"), ImageFormat::Png, 10);
        let b = image_file(&dir.join("IMG_0001.bmp"), ImageFormat::Bmp, 200);
        let jobs = [job(&a, "IMG_0001-edited"), job(&b, "IMG_0001-edited")];
        let res = run(&jobs, &options(ConflictPolicy::Rename, None));
        assert!(res.iter().all(|r| r.status == JobStatus::Done), "{res:?}");
        let outs: HashSet<_> = res.iter().map(|r| r.output.clone().unwrap()).collect();
        assert_eq!(outs.len(), 2);
        for o in &outs {
            assert!(fs::metadata(o).unwrap().len() > 0, "{o}");
        }
        assert_eq!(
            names(&dir),
            vec![
                "IMG_0001-edited (1).jpg".to_string(),
                "IMG_0001-edited.jpg".into(),
                "IMG_0001.bmp".into(),
                "IMG_0001.png".into(),
            ]
        );
    }

    #[test]
    fn same_output_name_from_many_folders_never_overwrites() {
        let root = tmpdir("many");
        let out = root.join("out");
        let jobs: Vec<BatchJob> = (0..12u8)
            .map(|i| {
                let d = root.join(format!("sub{i}"));
                fs::create_dir_all(&d).unwrap();
                job(&image_file(&d.join("x.png"), ImageFormat::Png, i * 20), "x")
            })
            .collect();
        let res = run(&jobs, &options(ConflictPolicy::Rename, Some(&out)));
        assert!(res.iter().all(|r| r.status == JobStatus::Done), "{res:?}");
        assert_eq!(names(&out).len(), 12);
        assert!(names(&out).iter().all(|n| !n.starts_with('.')));
    }

    #[test]
    fn same_output_name_with_overwrite_or_skip_is_an_error() {
        for policy in [ConflictPolicy::Overwrite, ConflictPolicy::Skip] {
            let dir = tmpdir(&format!("dup-{policy:?}"));
            let a = image_file(&dir.join("a.png"), ImageFormat::Png, 10);
            let b = image_file(&dir.join("b.png"), ImageFormat::Png, 200);
            let jobs = [job(&a, "same"), job(&b, "same")];
            let res = run(&jobs, &options(policy, None));
            let done = res.iter().filter(|r| r.status == JobStatus::Done).count();
            let failed: Vec<_> = res
                .iter()
                .filter(|r| r.status == JobStatus::Failed)
                .collect();
            assert_eq!((done, failed.len()), (1, 1), "{policy:?}: {res:?}");
            assert_eq!(failed[0].message.as_deref(), Some(SAME_NAME));
            assert_eq!(names(&dir), vec!["a.png", "b.png", "same.jpg"]);
        }
    }

    #[test]
    fn skip_keeps_existing_files() {
        let dir = tmpdir("skip");
        let a = image_file(&dir.join("a.png"), ImageFormat::Png, 10);
        fs::write(dir.join("taken.jpg"), b"keep me").unwrap();
        let res = run(&[job(&a, "taken")], &options(ConflictPolicy::Skip, None));
        assert_eq!(res[0].status, JobStatus::Skipped);
        assert_eq!(fs::read(dir.join("taken.jpg")).unwrap(), b"keep me");
    }

    #[test]
    fn renaming_onto_another_input_never_loses_a_file() {
        for policy in [ConflictPolicy::Rename, ConflictPolicy::Overwrite] {
            let dir = tmpdir(&format!("swap-{policy:?}"));
            let one = image_file(&dir.join("1.jpg"), ImageFormat::Jpeg, 10);
            let two = image_file(&dir.join("2.jpg"), ImageFormat::Jpeg, 200);
            let before: HashSet<Vec<u8>> =
                [fs::read(&one).unwrap(), fs::read(&two).unwrap()].into();
            let mut opts = options(policy, None);
            opts.resize = ResizeMode::None;
            opts.format = TargetFormat::Keep;
            opts.transfer = Transfer::Move;
            let res = run(&[job(&one, "2"), job(&two, "1")], &opts);
            assert!(res.iter().all(|r| r.status == JobStatus::Done), "{res:?}");
            let after: HashSet<Vec<u8>> = names(&dir)
                .iter()
                .map(|n| fs::read(dir.join(n)).unwrap())
                .collect();
            assert_eq!(names(&dir).len(), 2, "{:?}", names(&dir));
            assert_eq!(after, before);
        }
    }

    #[test]
    fn failure_and_cancel_remove_the_placeholder() {
        let dir = tmpdir("cleanup");
        // Sniffs as JPEG but can't be decoded.
        let bad = dir.join("bad.jpg");
        fs::write(
            &bad,
            [0xFF, 0xD8, 0xFF, 0xE0, 0, 16, b'J', b'F', b'I', b'F', 0],
        )
        .unwrap();
        let res = run(&[job(&bad, "out")], &options(ConflictPolicy::Rename, None));
        assert_eq!(res[0].status, JobStatus::Failed, "{res:?}");
        assert_eq!(names(&dir), vec!["bad.jpg"]);

        let good = image_file(&dir.join("good.png"), ImageFormat::Png, 10);
        let jobs = [job(&good, "out")];
        let claims = Claims::new(&jobs);
        let cancel = AtomicBool::new(true);
        let r = process(
            &jobs[0],
            &options(ConflictPolicy::Skip, None),
            None,
            &claims,
            &cancel,
        );
        assert_eq!(r.message.as_deref(), Some("Cancelled"));
        assert_eq!(names(&dir), vec!["bad.jpg", "good.png"]);
    }

    #[test]
    fn xmp_orientation_is_reset_and_cmyk_profile_dropped() {
        let dir = tmpdir("metadata");
        let img =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(16, 12, image::Rgba([1, 2, 3, 255])));
        let enc = EncodeOptions {
            format: ImageFormat::Jpeg,
            quality: 90,
            ..Default::default()
        };
        let mut cmyk = vec![0u8; 128];
        cmyk[16..20].copy_from_slice(b"CMYK");
        let xmp = br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:tiff="http://ns.adobe.com/tiff/1.0/" tiff:Orientation="6"/></rdf:RDF></x:xmpmeta>"#;
        let (bytes, _) = tessera_imaging::embed(
            tessera_imaging::encode(&img, &enc).unwrap(),
            ImageFormat::Jpeg,
            None,
            None,
            Some(xmp),
        )
        .unwrap();
        let src = dir.join("src.jpg");
        fs::write(&src, bytes).unwrap();
        let res = run(&[job(&src, "out")], &options(ConflictPolicy::Rename, None));
        assert_eq!(res[0].status, JobStatus::Done, "{res:?}");
        let out = tessera_imaging::load(&dir.join("out.jpg")).unwrap();
        let x = String::from_utf8(out.metadata.xmp.unwrap()).unwrap();
        assert!(x.contains("tiff:Orientation=\"1\""), "{x}");
        // A CMYK profile is never planned for RGB output.
        let md = tessera_imaging::RawMetadata {
            exif: None,
            icc: Some(cmyk),
            xmp: None,
        };
        let planned = tessera_imaging::plan_metadata(&md, &MetadataPolicy::keep_all(), (1, 1));
        assert!(planned.icc.is_none());
    }

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
