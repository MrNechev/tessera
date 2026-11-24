//! "Video out": an MP4 slideshow of photos, short clips and title cards, with
//! crossfades, a slow zoom/pan and music, written to `Outputs/Videos/`.
//!
//! The UI sends an ordered list of items; `video_start` checks every path with the
//! write guard, queues the job on the engine's transcode pool and returns at once.
//! The UI follows the job through `video_status` and `video` events (plus the usual
//! `progress` events) and can cancel it. The work:
//!
//! 1. Photos are decoded by Tessera's own image engine (EXIF orientation, HEIC,
//!    RAW, crops, flips) into plain JPEGs no larger than needed, so ffmpeg never
//!    sees a format or orientation it might get wrong. Clips are probed.
//! 2. The slideshow is encoded in chunks (see [`graph`]) and joined without
//!    re-encoding; the music is added in that last step.

pub mod graph;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use self::graph::{Encoder, Frame, Join, Segment, Source, FPS};
use super::{paths, transfer, AreaKind, CmdResult, Tools};
use crate::jobs::Priority;
use crate::media::cmd::{self, Exec};

/// Segments per ffmpeg run (bounds memory: every still of a run is held decoded).
const CHUNK: usize = 6;
/// Most items in one slideshow.
const MAX_ITEMS: usize = 1000;
/// Clip length when the item doesn't say where to stop.
const DEFAULT_CLIP_SECONDS: f64 = 30.0;
/// Crossfade length in frames (0.8 s).
const FADE_FRAMES: u32 = 24;

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VideoSize {
    /// 1920×1080
    #[default]
    Landscape,
    /// 1080×1920 (phones, stories, reels)
    Vertical,
    /// 1080×1080
    Square,
}

impl VideoSize {
    pub fn frame(self) -> Frame {
        match self {
            VideoSize::Landscape => Frame { w: 1920, h: 1080 },
            VideoSize::Vertical => Frame { w: 1080, h: 1920 },
            VideoSize::Square => Frame { w: 1080, h: 1080 },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Transition {
    #[default]
    Crossfade,
    /// Dip to black.
    Black,
    None,
}

/// How a picture whose shape differs from the video's is placed.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Framing {
    /// Fill when the shapes are close, fit (over a blurred copy) when they are not.
    #[default]
    Auto,
    Fill,
    Fit,
}

/// Part of the source picture, as fractions of its (upright) width and height.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One entry of the slideshow, in order.
#[derive(Clone, Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum VideoItem {
    /// A photo: a library reference, or an absolute path the guard allows.
    Image {
        library_id: Option<String>,
        path: String,
        crop: Option<CropRect>,
        #[serde(default)]
        flip_x: bool,
        #[serde(default)]
        flip_y: bool,
        seconds: Option<f64>,
        ken_burns: Option<bool>,
        framing: Option<Framing>,
    },
    /// A video clip (or animated GIF) from `start` to `end` seconds; muted.
    Video {
        library_id: Option<String>,
        path: String,
        start: Option<f64>,
        end: Option<f64>,
        framing: Option<Framing>,
    },
    /// A picture made by the UI (title card, collage page): PNG or JPEG, base64.
    Card {
        data: String,
        seconds: Option<f64>,
        ken_burns: Option<bool>,
        framing: Option<Framing>,
    },
}

/// A library file or a path.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub library_id: Option<String>,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VideoOptions {
    pub size: VideoSize,
    /// Seconds each photo stays on screen.
    pub seconds: f64,
    pub transition: Transition,
    pub ken_burns: bool,
    pub framing: Framing,
    /// Background music (an audio file from a library).
    pub music: Option<FileRef>,
    /// File name without extension.
    pub name: String,
}

impl Default for VideoOptions {
    fn default() -> Self {
        VideoOptions {
            size: VideoSize::Landscape,
            seconds: 4.0,
            transition: Transition::Crossfade,
            ken_burns: true,
            framing: Framing::Auto,
            music: None,
            name: "Slideshow".into(),
        }
    }
}

/// What the UI sees of a job.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoStatus {
    pub job: u32,
    /// queued | running | done | failed | cancelled
    pub state: &'static str,
    /// 0..1
    pub progress: f64,
    pub stage: String,
    /// The finished file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Its path inside Outputs (a library), so the UI can show or reference it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<&'static str>,
    /// Length of the video in seconds (known once the items are prepared).
    pub seconds: f64,
}

/// Whether videos can be made here, and if not, why (shown by the dialog).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoSupport {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder: Option<&'static str>,
    pub gpu: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

struct Job {
    cancel: CancellationToken,
    status: Mutex<VideoStatus>,
    last_event: Mutex<Option<(Instant, &'static str, String)>>,
}

/// The engine's slideshow jobs.
#[derive(Default)]
pub struct Videos {
    next: AtomicU32,
    jobs: Mutex<HashMap<u32, Arc<Job>>>,
    /// Software H.264 encoders of this ffmpeg (asked once).
    encoders: OnceLock<Vec<Encoder>>,
}

impl Job {
    fn status(&self) -> VideoStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Change the status and tell the UIs (progress at most ~5 times a second).
    fn update(&self, t: &Tools, f: impl FnOnce(&mut VideoStatus)) {
        let s = {
            let mut g = self.status.lock().unwrap_or_else(|e| e.into_inner());
            f(&mut g);
            g.clone()
        };
        {
            let mut last = self.last_event.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((at, state, stage)) = last.as_ref() {
                if *state == s.state
                    && *stage == s.stage
                    && at.elapsed() < Duration::from_millis(200)
                {
                    return;
                }
            }
            *last = Some((Instant::now(), s.state, s.stage.clone()));
        }
        t.events.send("video", &s);
        t.progress(
            "video",
            (s.progress * 1000.0).round() as usize,
            1000,
            Some(s.stage.clone()),
        );
    }
}

fn media(t: &Tools) -> CmdResult<&Arc<crate::media::Manager>> {
    t.media
        .get()
        .ok_or_else(|| "The video engine isn't ready yet; try again in a moment.".to_string())
}

/// Why there is no ffmpeg, in words that say what to do about it.
fn missing_ffmpeg(t: &Tools) -> String {
    match t.cfg.mode {
        super::Mode::Server => {
            "ffmpeg was not found on this server, so it can't make videos (the Docker image includes it; otherwise install ffmpeg or set TESSERA_FFMPEG).".into()
        }
        super::Mode::Desktop if Path::new("/.flatpak-info").exists() => {
            "The Flatpak version of Tessera doesn't include ffmpeg yet, so it can't make videos. Use the web app (Docker), or the .deb, .rpm or AppImage version with ffmpeg installed.".into()
        }
        super::Mode::Desktop => {
            "Making videos needs ffmpeg, which wasn't found. Install it (for example “sudo dnf install ffmpeg” or “sudo apt install ffmpeg”) and restart Tessera.".into()
        }
    }
}

/// Software H.264 encoders in this ffmpeg build, best first.
fn software_encoders(t: &Tools, m: &crate::media::Manager) -> Vec<Encoder> {
    if let Some(e) = t.videos.encoders.get() {
        return e.clone();
    }
    let ex = m.exec(&CancellationToken::new(), Duration::from_secs(20), false);
    let args: Vec<String> = ["-hide_banner", "-encoders"].map(String::from).to_vec();
    let Ok((out, _)) = ex.output("ffmpeg", &args) else {
        return Vec::new();
    };
    let list = parse_encoders(&String::from_utf8_lossy(&out));
    let _ = t.videos.encoders.set(list.clone());
    list
}

/// Picks the H.264 encoders out of `ffmpeg -encoders`.
fn parse_encoders(out: &str) -> Vec<Encoder> {
    let has = |name: &str| {
        out.lines()
            .any(|l| l.split_whitespace().nth(1) == Some(name))
    };
    [Encoder::X264, Encoder::OpenH264]
        .into_iter()
        .filter(|e| has(e.name()))
        .collect()
}

/// Encoders to try, best first: the GPU when the engine's test encode worked.
fn encoders(t: &Tools, m: &crate::media::Manager) -> Vec<Encoder> {
    let mut v = Vec::new();
    if m.hw().0 == "vaapi" {
        v.push(Encoder::Vaapi);
    }
    v.extend(software_encoders(t, m));
    v
}

pub fn video_support(t: &Tools) -> VideoSupport {
    let no = |message: String| VideoSupport {
        available: false,
        encoder: None,
        gpu: false,
        message: Some(message),
    };
    let m = match media(t) {
        Ok(m) => m,
        Err(e) => return no(e),
    };
    if !m.has_ffmpeg() {
        return no(missing_ffmpeg(t));
    }
    let list = encoders(t, m);
    match list.first() {
        None => no("This ffmpeg has no H.264 encoder (x264, OpenH264 or VA-API), so it can't make MP4 videos.".into()),
        Some(e) => VideoSupport {
            available: true,
            encoder: Some(e.name()),
            gpu: *e == Encoder::Vaapi,
            message: None,
        },
    }
}

/// A file the job reads, resolved and checked by the guard.
fn resolve(t: &Tools, library_id: &Option<String>, path: &str) -> CmdResult<PathBuf> {
    let p = match library_id.as_deref().filter(|s| !s.is_empty()) {
        Some(id) => PathBuf::from(transfer::resolve_ref(t, id, path)?),
        None => t.guard.readable(&paths::expand(path))?,
    };
    if !p.is_file() {
        return Err(format!("{} was not found", p.display()));
    }
    Ok(p)
}

/// An item with its file resolved (or its picture decoded from base64).
enum Input {
    Image {
        path: PathBuf,
        crop: Option<CropRect>,
        flip_x: bool,
        flip_y: bool,
        seconds: f64,
        motion: bool,
        framing: Framing,
    },
    Video {
        path: PathBuf,
        start: f64,
        end: Option<f64>,
        framing: Framing,
    },
    Card {
        data: Vec<u8>,
        seconds: f64,
        motion: bool,
        framing: Framing,
    },
}

fn clamp_seconds(s: f64) -> f64 {
    if s.is_finite() {
        s.clamp(1.0, 60.0)
    } else {
        4.0
    }
}

fn resolve_items(t: &Tools, items: Vec<VideoItem>, o: &VideoOptions) -> CmdResult<Vec<Input>> {
    let secs = clamp_seconds(o.seconds);
    items
        .into_iter()
        .enumerate()
        .map(|(i, it)| {
            Ok(match it {
                VideoItem::Image {
                    library_id,
                    path,
                    crop,
                    flip_x,
                    flip_y,
                    seconds,
                    ken_burns,
                    framing,
                } => Input::Image {
                    path: resolve(t, &library_id, &path)?,
                    crop,
                    flip_x,
                    flip_y,
                    seconds: seconds.map(clamp_seconds).unwrap_or(secs),
                    motion: ken_burns.unwrap_or(o.ken_burns),
                    framing: framing.unwrap_or(o.framing),
                },
                VideoItem::Video {
                    library_id,
                    path,
                    start,
                    end,
                    framing,
                } => Input::Video {
                    path: resolve(t, &library_id, &path)?,
                    start: start.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(0.0),
                    end: end.filter(|e| e.is_finite() && *e > 0.0),
                    framing: framing.unwrap_or(o.framing),
                },
                VideoItem::Card {
                    data,
                    seconds,
                    ken_burns,
                    framing,
                } => {
                    let b64 = data.split_once(',').map_or(data.as_str(), |(_, b)| b);
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(b64.trim())
                        .map_err(|e| format!("item {}: bad picture data: {e}", i + 1))?;
                    Input::Card {
                        data,
                        seconds: seconds.map(clamp_seconds).unwrap_or(secs.min(4.0)),
                        motion: ken_burns.unwrap_or(false),
                        framing: framing.unwrap_or(Framing::Auto),
                    }
                }
            })
        })
        .collect()
}

/// `Outputs/Videos`, checked by the guard.
fn videos_dir(t: &Tools) -> CmdResult<PathBuf> {
    let root = t
        .guard
        .areas()
        .into_iter()
        .find(|a| a.kind == AreaKind::Outputs)
        .map(|a| a.root)
        .unwrap_or_else(|| t.cfg.outputs_dir.clone());
    let dir = t.guard.writable(&root.join("Videos"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// A file name without path separators or control characters.
fn file_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "/\\:*?\"<>|".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().trim_start_matches('.').trim();
    let s: String = s.chars().take(120).collect();
    if s.is_empty() {
        "Slideshow".into()
    } else {
        s
    }
}

/// Queue a slideshow; returns its first status at once.
pub fn video_start(
    t: &Arc<Tools>,
    items: Vec<VideoItem>,
    o: VideoOptions,
) -> CmdResult<VideoStatus> {
    let m = media(t)?.clone();
    if !m.has_ffmpeg() {
        return Err(missing_ffmpeg(t));
    }
    if items.is_empty() {
        return Err("Nothing to put in the video: choose some photos or clips.".into());
    }
    if items.len() > MAX_ITEMS {
        return Err(format!(
            "A video can have at most {MAX_ITEMS} photos and clips."
        ));
    }
    if encoders(t, &m).is_empty() {
        return Err("This ffmpeg has no H.264 encoder (x264, OpenH264 or VA-API), so it can't make MP4 videos.".into());
    }
    let inputs = resolve_items(t, items, &o)?;
    let music = match &o.music {
        Some(r) if !r.path.is_empty() => Some(resolve(t, &r.library_id, &r.path)?),
        _ => None,
    };
    let dir = videos_dir(t)?;

    let id = t.videos.next.fetch_add(1, Ordering::Relaxed) + 1;
    let job = Arc::new(Job {
        cancel: CancellationToken::new(),
        status: Mutex::new(VideoStatus {
            job: id,
            state: "queued",
            stage: "Waiting".into(),
            ..Default::default()
        }),
        last_event: Mutex::new(None),
    });
    {
        let mut jobs = t.videos.jobs.lock().unwrap_or_else(|e| e.into_inner());
        // Forget finished jobs once there are many (the UI polls only its own).
        if jobs.len() > 32 {
            jobs.retain(|_, j| matches!(j.status().state, "queued" | "running"));
        }
        jobs.insert(id, job.clone());
    }
    let (t2, j2) = (t.clone(), job.clone());
    let submitted = m.transcode.submit(
        &format!("video-out-{id}"),
        Priority::High,
        false,
        move |_ctx| {
            let res = run(&t2, &j2, inputs, music, &o, &dir);
            match &res {
                Ok((path, rel)) => j2.update(&t2, |s| {
                    s.state = "done";
                    s.progress = 1.0;
                    s.stage = "Done".into();
                    s.path = Some(path.to_string_lossy().into_owned());
                    s.rel = rel.clone();
                }),
                Err(_) if j2.cancel.is_cancelled() => j2.update(&t2, |s| {
                    s.state = "cancelled";
                    s.stage = "Cancelled".into();
                }),
                Err(e) => {
                    tracing::warn!(job = id, err = %e, "video failed");
                    j2.update(&t2, |s| {
                        s.state = "failed";
                        s.stage = "Failed".into();
                        s.error = Some(format!("{e:#}"));
                    })
                }
            }
            res.map(|_| ())
        },
    );
    if let Err(e) = submitted {
        t.videos
            .jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        return Err(e.to_string());
    }
    Ok(job.status())
}

pub fn video_status(t: &Tools, job: u32) -> CmdResult<VideoStatus> {
    t.videos
        .jobs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&job)
        .map(|j| j.status())
        .ok_or_else(|| format!("unknown video job {job}"))
}

pub fn video_cancel(t: &Tools, job: u32) -> CmdResult<()> {
    let j = t
        .videos
        .jobs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&job)
        .cloned();
    if let Some(j) = j {
        j.cancel.cancel();
        // A queued job only notices when a worker picks it up; say so now.
        if j.status().state == "queued" {
            j.update(t, |s| {
                s.state = "cancelled";
                s.stage = "Cancelled".into();
            });
        }
    }
    Ok(())
}

/// Removes the job's work folder however the job ends.
struct WorkDir(PathBuf);

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(
    t: &Tools,
    job: &Job,
    inputs: Vec<Input>,
    music: Option<PathBuf>,
    o: &VideoOptions,
    dir: &Path,
) -> anyhow::Result<(PathBuf, Option<String>)> {
    if job.cancel.is_cancelled() {
        bail!("cancelled");
    }
    let m = media(t).map_err(|e| anyhow!(e))?.clone();
    let id = job.status().job;
    let work = WorkDir(
        t.cache_dir()
            .join("video")
            .join(format!("job-{id}-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&work.0);
    std::fs::create_dir_all(&work.0).with_context(|| format!("create {}", work.0.display()))?;
    let ex = m.exec(&job.cancel, Duration::from_secs(24 * 3600), false);
    let frame = o.size.frame();
    job.update(t, |s| {
        s.state = "running";
        s.stage = "Preparing photos".into();
    });

    // 1. Prepare.
    let n = inputs.len();
    let mut segs = Vec::with_capacity(n);
    for (i, input) in inputs.into_iter().enumerate() {
        if job.cancel.is_cancelled() {
            bail!("cancelled");
        }
        job.update(t, |s| {
            s.progress = 0.08 * i as f64 / n as f64;
            s.stage = format!("Preparing {} of {n}", i + 1);
        });
        segs.push(prepare(&ex, &work.0, i, input, frame)?);
    }

    // 2. Timeline.
    let lens: Vec<u32> = segs.iter().map(|s| s.frames).collect();
    let (join, fade) = match o.transition {
        Transition::None => (Join::Cut, 0),
        Transition::Crossfade => (Join::Xfade("fade"), FADE_FRAMES),
        Transition::Black => (Join::Xfade("fadeblack"), FADE_FRAMES),
    };
    let fade = graph::clamp_fade(&lens, fade);
    let total = graph::total_frames(&lens, fade);
    let chunks = graph::plan_chunks(&lens, fade, CHUNK);
    job.update(t, |s| s.seconds = f64::from(total) / f64::from(FPS));

    // 3. Encode the chunks (falling back to the next encoder if one fails).
    let list = encoders(t, &m);
    let mut files = Vec::new();
    let mut last_err = anyhow!("no H.264 encoder");
    for (k, enc) in list.iter().enumerate() {
        job.update(t, |s| s.encoder = Some(enc.name()));
        match encode_chunks(
            t, job, &ex, &m, *enc, &segs, &chunks, fade, join, frame, total, &work.0,
        ) {
            Ok(f) => {
                files = f;
                break;
            }
            Err(e) if job.cancel.is_cancelled() => return Err(e),
            Err(e) => {
                if k + 1 < list.len() {
                    tracing::warn!(encoder = enc.name(), err = %e, "video encoder failed, trying the next one");
                }
                last_err = e;
            }
        }
    }
    if files.is_empty() {
        return Err(last_err);
    }

    // 4. Join, add the music and publish into Outputs.
    job.update(t, |s| {
        s.progress = 0.97;
        s.stage = "Finishing".into();
    });
    let list_path = work.0.join("chunks.txt");
    std::fs::write(&list_path, graph::concat_list(&files))?;
    let stem = file_stem(&o.name);
    let tmp = dir.join(format!(".{stem}.part-{id}.mp4"));
    let args = graph::mux_args(
        &list_path.to_string_lossy(),
        music.as_deref().map(|p| p.to_string_lossy()).as_deref(),
        total,
        &tmp.to_string_lossy(),
    );
    if let Err(e) = ex.run_lines("ffmpeg", &args, |_| {}) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    let out = paths::unique_path(&dir.join(format!("{stem}.mp4")));
    if let Err(e) = std::fs::rename(&tmp, &out) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("save {}", out.display()));
    }
    let rel = t
        .guard
        .areas()
        .into_iter()
        .filter(|a| a.kind == AreaKind::Outputs)
        .find_map(|a| {
            let root = a.root.canonicalize().unwrap_or(a.root);
            out.strip_prefix(&root)
                .ok()
                .map(|r| r.to_string_lossy().into_owned())
        });
    tracing::info!(path = %out.display(), seconds = f64::from(total) / f64::from(FPS), "video ready");
    Ok((out, rel))
}

#[allow(clippy::too_many_arguments)]
fn encode_chunks(
    t: &Tools,
    job: &Job,
    ex: &Exec,
    m: &crate::media::Manager,
    enc: Encoder,
    segs: &[Segment],
    chunks: &[graph::Chunk],
    fade: u32,
    join: Join,
    frame: Frame,
    total: u32,
    work: &Path,
) -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    let mut done = 0u32;
    for (i, c) in chunks.iter().enumerate() {
        let out = work.join(format!("chunk-{i:04}.mp4"));
        let g = graph::chunk_graph(segs, c, fade, join, frame, enc == Encoder::Vaapi);
        let args = graph::chunk_args(&g, enc, frame, m.vaapi_device(), &out.to_string_lossy());
        let base = done;
        ex.run_lines("ffmpeg", &args, |line| {
            if let Some(v) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = v.parse::<i64>() {
                    let f = (us.max(0) as f64 / 1e6 * f64::from(FPS)).min(f64::from(c.frames));
                    job.update(t, |s| {
                        s.progress = 0.08 + 0.89 * (f64::from(base) + f) / f64::from(total.max(1));
                        s.stage = "Encoding".into();
                    });
                }
            }
        })?;
        done += c.frames;
        files.push(out.to_string_lossy().into_owned());
    }
    Ok(files)
}

/// Whether a picture of aspect `a` should be fitted (not cropped) into a frame of aspect `f`.
fn should_fit(framing: Framing, a: f64, f: f64) -> bool {
    match framing {
        Framing::Fill => false,
        Framing::Fit => true,
        // Up to 40 % off (a 4:3 photo in a 16:9 video) still fills nicely.
        Framing::Auto => (a / f).ln().abs() > 1.4f64.ln(),
    }
}

/// Turns an input into a segment: photos become plain JPEGs, cards are written out,
/// clips are probed.
fn prepare(
    ex: &Exec,
    work: &Path,
    i: usize,
    input: Input,
    frame: Frame,
) -> anyhow::Result<Segment> {
    let aspect = f64::from(frame.w) / f64::from(frame.h);
    let frames_for = |s: f64| ((s * f64::from(FPS)).round() as u32).max(FPS / 2);
    match input {
        Input::Image {
            path,
            crop,
            flip_x,
            flip_y,
            seconds,
            motion,
            framing,
        } => {
            let out = work.join(format!("still-{i:04}.jpg"));
            let canvas = if motion {
                (frame.w * 2, frame.h * 2)
            } else {
                (frame.w, frame.h)
            };
            let fit = prepare_still(&path, crop, flip_x, flip_y, canvas, framing, aspect, &out)
                .with_context(|| format!("{}", path.display()))?;
            Ok(Segment {
                path: out.to_string_lossy().into_owned(),
                source: Source::Still {
                    motion: motion.then(|| graph::motion_for(&path.to_string_lossy())),
                },
                frames: frames_for(seconds),
                fit,
            })
        }
        Input::Card {
            data,
            seconds,
            motion,
            framing,
        } => {
            let reader = image::ImageReader::new(std::io::Cursor::new(&data))
                .with_guessed_format()
                .context("title card")?;
            let ext = match reader.format() {
                Some(image::ImageFormat::Png) => "png",
                Some(image::ImageFormat::Jpeg) => "jpg",
                _ => bail!("item {}: the picture must be a PNG or JPEG", i + 1),
            };
            let (w, h) = reader.into_dimensions().context("title card")?;
            let out = work.join(format!("card-{i:04}.{ext}"));
            std::fs::write(&out, &data)?;
            Ok(Segment {
                path: out.to_string_lossy().into_owned(),
                source: Source::Still {
                    motion: motion.then(|| graph::motion_for(&blake3::hash(&data).to_hex())),
                },
                frames: frames_for(seconds),
                fit: should_fit(framing, f64::from(w) / f64::from(h.max(1)), aspect),
            })
        }
        Input::Video {
            path,
            start,
            end,
            framing,
        } => {
            let info = probe_clip(ex, &path).with_context(|| format!("{}", path.display()))?;
            let start = if info.duration > 0.0 && start >= info.duration {
                0.0
            } else {
                start
            };
            let mut stop = end.unwrap_or(start + DEFAULT_CLIP_SECONDS);
            if info.duration > 0.0 {
                stop = stop.min(info.duration);
            }
            let len = (stop - start).max(0.5);
            Ok(Segment {
                path: path.to_string_lossy().into_owned(),
                source: Source::Clip {
                    start,
                    hdr: info.hdr,
                },
                frames: ((len * f64::from(FPS)).floor() as u32).max(FPS / 2),
                fit: should_fit(
                    framing,
                    f64::from(info.w) / f64::from(info.h.max(1)),
                    aspect,
                ),
            })
        }
    }
}

/// Decodes a photo upright, crops and flips it, and writes a JPEG just large enough
/// for `canvas`. Returns whether it is to be fitted (rather than cropped to fill).
#[allow(clippy::too_many_arguments)]
fn prepare_still(
    src: &Path,
    crop: Option<CropRect>,
    flip_x: bool,
    flip_y: bool,
    canvas: (u32, u32),
    framing: Framing,
    aspect: f64,
    out: &Path,
) -> anyhow::Result<bool> {
    let c = crop
        .filter(|c| [c.x, c.y, c.w, c.h].iter().all(|v| v.is_finite()))
        .map(|c| {
            let x = c.x.clamp(0.0, 0.99);
            let y = c.y.clamp(0.0, 0.99);
            CropRect {
                x,
                y,
                w: c.w.clamp(0.01, 1.0 - x),
                h: c.h.clamp(0.01, 1.0 - y),
            }
        })
        .unwrap_or(CropRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        });
    let (cw, ch) = (f64::from(canvas.0), f64::from(canvas.1));
    // Decode only as many pixels as the video can show (the image engine decodes
    // JPEGs at reduced size directly, which is much faster).
    let (bw, bh, fit) = match tessera_imaging::probe(src) {
        Ok(info) => {
            let (w0, h0) = (f64::from(info.width.max(1)), f64::from(info.height.max(1)));
            let (rw, rh) = (w0 * c.w, h0 * c.h);
            let fit = should_fit(framing, rw / rh, aspect);
            let s = if fit {
                (cw / rw).min(ch / rh)
            } else {
                (cw / rw).max(ch / rh)
            }
            .min(1.0);
            ((w0 * s).ceil() as u32, (h0 * s).ceil() as u32, Some(fit))
        }
        Err(_) => (canvas.0 * 2, canvas.1 * 2, None),
    };
    let mut img = tessera_imaging::load_scaled_rgba(src, bw.max(1), bh.max(1))?;
    let (iw, ih) = img.dimensions();
    if c != (CropRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    }) {
        let x = (c.x * f64::from(iw)).round() as u32;
        let y = (c.y * f64::from(ih)).round() as u32;
        let w = ((c.w * f64::from(iw)).round() as u32).clamp(1, iw - x.min(iw - 1));
        let h = ((c.h * f64::from(ih)).round() as u32).clamp(1, ih - y.min(ih - 1));
        img = image::imageops::crop_imm(&img, x.min(iw - 1), y.min(ih - 1), w, h).to_image();
    }
    if flip_x {
        image::imageops::flip_horizontal_in_place(&mut img);
    }
    if flip_y {
        image::imageops::flip_vertical_in_place(&mut img);
    }
    let fit = fit.unwrap_or_else(|| {
        should_fit(
            framing,
            f64::from(img.width()) / f64::from(img.height().max(1)),
            aspect,
        )
    });
    let bytes = tessera_imaging::encode(
        &image::DynamicImage::ImageRgba8(img),
        &tessera_imaging::EncodeOptions {
            format: tessera_imaging::ImageFormat::Jpeg,
            quality: 92,
            ..Default::default()
        },
    )?;
    std::fs::write(out, bytes)?;
    Ok(fit)
}

#[derive(Debug, Default, PartialEq)]
struct ClipInfo {
    w: u32,
    h: u32,
    duration: f64,
    hdr: bool,
}

fn probe_clip(ex: &Exec, path: &Path) -> anyhow::Result<ClipInfo> {
    let args: Vec<String> = [
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,color_transfer:stream_side_data=rotation:stream_tags=rotate:format=duration",
        "-of",
        "json",
    ]
    .map(String::from)
    .into_iter()
    .chain([path.to_string_lossy().into_owned()])
    .collect();
    let out = match ex.output("ffprobe", &args) {
        Ok((out, _)) => out,
        Err(cmd::CmdError::Exit { stderr, .. }) => {
            bail!(
                "ffprobe: {}",
                crate::util::first_line(&String::from_utf8_lossy(&stderr))
            )
        }
        Err(e) => return Err(anyhow!(e)),
    };
    parse_clip(&out).ok_or_else(|| anyhow!("no video stream"))
}

fn parse_clip(json: &[u8]) -> Option<ClipInfo> {
    let v: serde_json::Value = serde_json::from_slice(json).ok()?;
    let s = v.get("streams")?.get(0)?;
    let (mut w, mut h) = (
        s.get("width")?.as_u64()? as u32,
        s.get("height")?.as_u64()? as u32,
    );
    let rotation = s
        .get("side_data_list")
        .and_then(|l| l.as_array())
        .and_then(|l| {
            l.iter()
                .find_map(|d| d.get("rotation").and_then(|r| r.as_f64()))
        })
        .or_else(|| {
            s.get("tags")
                .and_then(|t| t.get("rotate"))
                .and_then(|r| r.as_str())
                .and_then(|r| r.parse().ok())
        })
        .unwrap_or(0.0);
    // ffmpeg turns the frames upright on decoding; the shape is the displayed one.
    if (rotation.abs() as i64) % 180 == 90 {
        std::mem::swap(&mut w, &mut h);
    }
    let transfer = s
        .get("color_transfer")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let duration = v
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);
    Some(ClipInfo {
        w,
        h,
        duration,
        hdr: matches!(transfer, "smpte2084" | "arib-std-b67"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_parse_from_the_ui() {
        let items: Vec<VideoItem> = serde_json::from_str(
            r#"[{"kind":"image","libraryId":"photos","path":"a.jpg","crop":{"x":0.1,"y":0,"w":0.5,"h":1},"flipX":true},
                {"kind":"video","path":"/m/c.mp4","start":1.5,"end":4},
                {"kind":"card","data":"aGk=","seconds":3}]"#,
        )
        .unwrap();
        assert!(
            matches!(&items[0], VideoItem::Image { library_id: Some(l), flip_x: true, crop: Some(_), .. } if l == "photos")
        );
        assert!(
            matches!(&items[1], VideoItem::Video { start: Some(s), end: Some(e), .. } if *s == 1.5 && *e == 4.0)
        );
        assert!(matches!(
            &items[2],
            VideoItem::Card {
                seconds: Some(_),
                ..
            }
        ));
        let o: VideoOptions =
            serde_json::from_str(r#"{"size":"vertical","kenBurns":false,"transition":"none"}"#)
                .unwrap();
        assert_eq!(o.size.frame(), Frame { w: 1080, h: 1920 });
        assert!(!o.ken_burns);
        assert_eq!(o.seconds, 4.0);
    }

    #[test]
    fn auto_framing() {
        let hd = 16.0 / 9.0;
        assert!(!should_fit(Framing::Auto, 1.5, hd), "3:2 fills 16:9");
        assert!(!should_fit(Framing::Auto, 4.0 / 3.0, hd), "4:3 fills 16:9");
        assert!(
            should_fit(Framing::Auto, 3.0 / 4.0, hd),
            "portrait fits into landscape"
        );
        assert!(
            should_fit(Framing::Auto, 1.5, 9.0 / 16.0),
            "landscape fits into vertical"
        );
        assert!(!should_fit(Framing::Fill, 0.5, hd));
        assert!(should_fit(Framing::Fit, hd, hd));
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(
            file_stem("Summer / 2026: \"best\""),
            "Summer - 2026- -best-"
        );
        assert_eq!(file_stem("  ..hidden"), "hidden");
        assert_eq!(file_stem(""), "Slideshow");
    }

    #[test]
    fn encoder_list() {
        let out = " V....D libopenh264          OpenH264 H.264\n V....D h264_vaapi   VAAPI\n A....D aac   AAC\n";
        assert_eq!(parse_encoders(out), vec![Encoder::OpenH264]);
        let out =
            " V....D libx264  x264\n V....D libx264rgb  x264 RGB\n V....D libopenh264 OpenH264\n";
        assert_eq!(parse_encoders(out), vec![Encoder::X264, Encoder::OpenH264]);
    }

    #[test]
    fn clip_probe_reads_rotation_and_hdr() {
        let j = br#"{"streams":[{"width":1920,"height":1080,"color_transfer":"arib-std-b67",
            "side_data_list":[{"rotation":-90}]}],"format":{"duration":"12.5"}}"#;
        assert_eq!(
            parse_clip(j),
            Some(ClipInfo {
                w: 1080,
                h: 1920,
                duration: 12.5,
                hdr: true
            })
        );
        let j = br#"{"streams":[{"width":640,"height":480,"tags":{"rotate":"180"}}],"format":{}}"#;
        assert_eq!(
            parse_clip(j),
            Some(ClipInfo {
                w: 640,
                h: 480,
                duration: 0.0,
                hdr: false
            })
        );
        assert_eq!(parse_clip(br#"{"streams":[]}"#), None);
    }

    /// A real encode with tiny generated pictures, when ffmpeg is installed.
    #[test]
    fn real_encode_when_ffmpeg_exists() {
        if crate::util::look_path("ffmpeg").is_none() || crate::util::look_path("ffprobe").is_none()
        {
            eprintln!("skipped: no ffmpeg");
            return;
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let ex = Exec::new(
            Arc::new(cmd::Tools {
                ffmpeg: "ffmpeg".into(),
                ffprobe: "ffprobe".into(),
            }),
            rt.handle().clone(),
            CancellationToken::new(),
            Duration::from_secs(300),
            false,
        );
        let encs: Vec<Encoder> =
            match ex.output("ffmpeg", &["-hide_banner".into(), "-encoders".into()]) {
                Ok((out, _)) => parse_encoders(&String::from_utf8_lossy(&out)),
                Err(_) => Vec::new(),
            };
        let Some(enc) = encs.first().copied() else {
            eprintln!("skipped: no software H.264 encoder");
            return;
        };
        let dir = std::env::temp_dir().join(format!("tessera-video-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Three photos (one portrait, so one is fitted) and a title card, plus a
        // generated clip and a music track.
        let mut inputs = Vec::new();
        for (i, (w, h)) in [(320u32, 200u32), (200, 300), (300, 200)]
            .into_iter()
            .enumerate()
        {
            let img = image::RgbImage::from_fn(w, h, |x, y| {
                image::Rgb([(x * 255 / w) as u8, (y * 255 / h) as u8, (i * 90) as u8])
            });
            let p = dir.join(format!("p{i}.png"));
            img.save(&p).unwrap();
            inputs.push(Input::Image {
                path: p,
                crop: (i == 2).then_some(CropRect {
                    x: 0.1,
                    y: 0.1,
                    w: 0.6,
                    h: 0.8,
                }),
                flip_x: i == 2,
                flip_y: false,
                seconds: 1.0,
                motion: i != 1,
                framing: Framing::Auto,
            });
        }
        let mut card = Vec::new();
        image::RgbImage::from_pixel(160, 90, image::Rgb([20, 20, 30]))
            .write_to(
                &mut std::io::Cursor::new(&mut card),
                image::ImageFormat::Png,
            )
            .unwrap();
        inputs.insert(
            0,
            Input::Card {
                data: card,
                seconds: 1.0,
                motion: false,
                framing: Framing::Auto,
            },
        );
        let clip = dir.join("clip.mp4");
        let music = dir.join("music.m4a");
        let lavfi = |src: &str, out: &Path, extra: &[&str]| {
            let mut a: Vec<String> = ["-v", "error", "-y", "-f", "lavfi", "-i", src]
                .map(String::from)
                .to_vec();
            a.extend(extra.iter().map(|s| s.to_string()));
            a.push(out.to_string_lossy().into_owned());
            ex.run("ffmpeg", &a).unwrap();
        };
        lavfi("testsrc2=s=240x160:r=25:d=3", &clip, &["-c:v", "mpeg4"]);
        lavfi("sine=frequency=440:duration=2", &music, &["-c:a", "aac"]);
        inputs.push(Input::Video {
            path: clip,
            start: 0.5,
            end: Some(2.0),
            framing: Framing::Auto,
        });

        // A small frame keeps the test fast; the graph is the same as at 1080p.
        let frame = Frame { w: 320, h: 180 };
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let segs: Vec<Segment> = inputs
            .into_iter()
            .enumerate()
            .map(|(i, input)| prepare(&ex, &work, i, input, frame).unwrap())
            .collect();
        assert!(segs.iter().any(|s| s.fit), "the portrait photo is fitted");
        let lens: Vec<u32> = segs.iter().map(|s| s.frames).collect();
        assert_eq!(lens[4], 45, "the clip runs from 0.5 s to 2 s");
        let fade = graph::clamp_fade(&lens, FADE_FRAMES);
        assert_eq!(fade, 15);
        let total = graph::total_frames(&lens, fade);
        // Two chunks, to test joining them.
        let chunks = graph::plan_chunks(&lens, fade, 3);
        let mut files = Vec::new();
        for (i, c) in chunks.iter().enumerate() {
            let out = work.join(format!("chunk-{i}.mp4"));
            let g = graph::chunk_graph(&segs, c, fade, Join::Xfade("fade"), frame, false);
            ex.run_lines(
                "ffmpeg",
                &graph::chunk_args(&g, enc, frame, "", &out.to_string_lossy()),
                |_| {},
            )
            .unwrap_or_else(|e| panic!("{e}\n{}", g.filter));
            files.push(out.to_string_lossy().into_owned());
        }
        let list = work.join("list.txt");
        std::fs::write(&list, graph::concat_list(&files)).unwrap();
        let out = dir.join("out.mp4");
        ex.run_lines(
            "ffmpeg",
            &graph::mux_args(
                &list.to_string_lossy(),
                Some(&music.to_string_lossy()),
                total,
                &out.to_string_lossy(),
            ),
            |_| {},
        )
        .unwrap();

        // The result: H.264 + AAC, the planned length, the frame size.
        let probe: Vec<String> = [
            "-v",
            "error",
            "-count_frames",
            "-show_entries",
            "stream=codec_name,width,height,nb_read_frames:format=duration",
            "-of",
            "json",
        ]
        .map(String::from)
        .into_iter()
        .chain([out.to_string_lossy().into_owned()])
        .collect();
        let (json, _) = ex.output("ffprobe", &probe).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
        let streams = v["streams"].as_array().unwrap();
        let video = streams
            .iter()
            .find(|s| s["codec_name"] == "h264")
            .expect("an H.264 stream");
        assert!(
            streams.iter().any(|s| s["codec_name"] == "aac"),
            "an AAC stream"
        );
        assert_eq!(video["width"], 320);
        assert_eq!(video["height"], 180);
        assert_eq!(
            video["nb_read_frames"]
                .as_str()
                .unwrap()
                .parse::<u32>()
                .unwrap(),
            total
        );
        let secs: f64 = v["format"]["duration"].as_str().unwrap().parse().unwrap();
        assert!(
            (secs - f64::from(total) / f64::from(FPS)).abs() < 0.1,
            "{secs} s"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
