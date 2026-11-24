//! Pure builders for the slideshow's ffmpeg command lines (no I/O, unit-tested).
//!
//! Every length is counted in frames at [`FPS`], so transitions, chunk
//! boundaries and the total length are exact; seconds are only printed at the end.
//!
//! A slideshow is encoded in *chunks* of a few segments each: ffmpeg decodes every
//! still image of a graph up front (one frame per input, held until it is needed),
//! so one graph for 100 photos at 4K would hold gigabytes. Chunks are encoded with
//! the final encoder and joined with the concat demuxer without re-encoding.
//! With a crossfade, a chunk also renders the start of the next chunk's first
//! segment (to draw the transition into it) and the next chunk skips those frames.

use std::fmt::Write as _;

/// Frames per second of every slideshow.
pub const FPS: u32 = 30;

/// Output size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub w: u32,
    pub h: u32,
}

/// A slow zoom/pan ("Ken Burns"): zoom goes from `z0` to `z1`; `fx`/`fy` place the
/// visible window inside the free space (0 = left/top edge, 1 = right/bottom edge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub z0: f64,
    pub z1: f64,
    pub fx0: f64,
    pub fx1: f64,
    pub fy0: f64,
    pub fy1: f64,
}

/// The moves a photo can get. Picked by a hash of the item, so the same slideshow
/// always moves the same way (re-renders don't surprise).
const MOTIONS: [Motion; 8] = [
    // zoom in on the centre
    Motion {
        z0: 1.0,
        z1: 1.18,
        fx0: 0.5,
        fx1: 0.5,
        fy0: 0.5,
        fy1: 0.5,
    },
    // zoom out from the centre
    Motion {
        z0: 1.18,
        z1: 1.0,
        fx0: 0.5,
        fx1: 0.5,
        fy0: 0.5,
        fy1: 0.5,
    },
    // pan left → right
    Motion {
        z0: 1.15,
        z1: 1.15,
        fx0: 0.0,
        fx1: 1.0,
        fy0: 0.5,
        fy1: 0.5,
    },
    // pan right → left
    Motion {
        z0: 1.15,
        z1: 1.15,
        fx0: 1.0,
        fx1: 0.0,
        fy0: 0.5,
        fy1: 0.5,
    },
    // zoom in towards the upper left third
    Motion {
        z0: 1.0,
        z1: 1.2,
        fx0: 0.5,
        fx1: 0.25,
        fy0: 0.5,
        fy1: 0.3,
    },
    // zoom in towards the lower right third
    Motion {
        z0: 1.0,
        z1: 1.2,
        fx0: 0.5,
        fx1: 0.75,
        fy0: 0.5,
        fy1: 0.7,
    },
    // pan top → bottom
    Motion {
        z0: 1.15,
        z1: 1.15,
        fx0: 0.5,
        fx1: 0.5,
        fy0: 0.0,
        fy1: 1.0,
    },
    // zoom out from the upper part
    Motion {
        z0: 1.2,
        z1: 1.0,
        fx0: 0.5,
        fx1: 0.5,
        fy0: 0.3,
        fy1: 0.5,
    },
];

/// The deterministic move for an item (`key`: its path or content hash).
pub fn motion_for(key: &str) -> Motion {
    let h = blake3::hash(key.as_bytes());
    MOTIONS[h.as_bytes()[0] as usize % MOTIONS.len()]
}

/// What a segment shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A still image (one frame), shown for the segment's length.
    Still { motion: Option<Motion> },
    /// A video clip from `start` seconds; HDR clips are tone-mapped to SDR.
    Clip { start: f64, hdr: bool },
}

/// One slide of the slideshow, ready for ffmpeg.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub path: String,
    pub source: Source,
    /// Length on screen, in frames (including the transitions it takes part in).
    pub frames: u32,
    /// Fit inside the frame over a blurred copy of itself, instead of filling it (cropping).
    pub fit: bool,
}

/// Global start frame of every segment: segments overlap by `fade` frames.
pub fn starts(lens: &[u32], fade: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(lens.len());
    let mut at = 0u32;
    for &l in lens {
        out.push(at);
        at = at + l - fade;
    }
    out
}

/// Total frames of the slideshow.
pub fn total_frames(lens: &[u32], fade: u32) -> u32 {
    let sum: u32 = lens.iter().sum();
    sum - fade * (lens.len().saturating_sub(1) as u32)
}

/// The longest transition every segment allows: a segment must be at least two
/// transitions long (one in, one out), or chunks could not be cut between them.
pub fn clamp_fade(lens: &[u32], fade: u32) -> u32 {
    if lens.len() < 2 {
        return 0;
    }
    let shortest = lens.iter().copied().min().unwrap_or(0);
    fade.min(shortest / 2)
}

/// A part of the slideshow encoded by one ffmpeg run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    /// Segments rendered (inclusive range).
    pub first: usize,
    pub last: usize,
    /// Frames dropped at the start (already shown by the previous chunk's transition).
    pub skip: u32,
    /// Frames this chunk contributes to the slideshow.
    pub frames: u32,
}

/// Split the slideshow into chunks of at most `per` segments of their own.
pub fn plan_chunks(lens: &[u32], fade: u32, per: usize) -> Vec<Chunk> {
    let n = lens.len();
    if n == 0 {
        return Vec::new();
    }
    let per = per.max(1);
    let st = starts(lens, fade);
    let total = total_frames(lens, fade);
    let firsts: Vec<usize> = (0..n).step_by(per).collect();
    // Where each chunk's own output begins on the global timeline.
    let begin = |j: usize| -> u32 {
        if j == 0 {
            0
        } else {
            st[firsts[j]] + fade
        }
    };
    (0..firsts.len())
        .map(|j| {
            let first = firsts[j];
            let is_last = j + 1 == firsts.len();
            let end = if is_last { total } else { begin(j + 1) };
            let last = if is_last {
                n - 1
            } else if fade > 0 {
                // draw the transition into the next chunk's first segment
                firsts[j + 1]
            } else {
                firsts[j + 1] - 1
            };
            Chunk {
                first,
                last,
                skip: begin(j) - st[first],
                frames: end - begin(j),
            }
        })
        .collect()
}

/// Seconds for a frame count, as ffmpeg reads them.
pub fn secs(frames: u32) -> String {
    num(f64::from(frames) / f64::from(FPS))
}

/// A short decimal: `1.5`, `0.033333`, `2`.
pub fn num(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

/// How the segments are joined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// Cuts.
    Cut,
    /// xfade with this transition (`fade` = crossfade, `fadeblack` = dip to black).
    Xfade(&'static str),
}

/// One ffmpeg run's inputs and filter graph.
#[derive(Clone, Debug, PartialEq)]
pub struct ChunkGraph {
    /// `-ss … -t … -i path` for each segment, in order.
    pub inputs: Vec<String>,
    /// The `-filter_complex` graph; its output is `[vout]`.
    pub filter: String,
}

/// Scale `input` to exactly `w`×`h` into `[out]`: crop to fill, or fit over a blurred,
/// darkened copy (portrait photos in a landscape video keep their whole picture).
fn frame_to(g: &mut String, input: &str, out: &str, w: u32, h: u32, fit: bool, tag: &str) {
    if !fit {
        let _ = write!(
            g,
            "{input}scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos,crop={w}:{h},setsar=1[{out}];"
        );
        return;
    }
    // The background is blurred at 1/8 size (cheap, and smooth once scaled up).
    let even = |v: u32| (v / 8).max(16) & !1;
    let (bw, bh) = (even(w), even(h));
    let _ = write!(
        g,
        "{input}split=2[{tag}a][{tag}b];\
         [{tag}a]scale={bw}:{bh}:force_original_aspect_ratio=increase,crop={bw}:{bh},gblur=sigma=6,\
         scale={w}:{h},lutyuv=y=val*0.72,setsar=1[{tag}bg];\
         [{tag}b]scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos,setsar=1[{tag}fg];\
         [{tag}bg][{tag}fg]overlay=(W-w)/2:(H-h)/2[{out}];"
    );
}

fn zoompan(m: &Motion, frames: u32, f: Frame) -> String {
    // Progress through the move: 0 on the first frame, 1 on the last.
    let p = format!("on/{}", frames.saturating_sub(1).max(1));
    let lerp = |a: f64, b: f64| {
        if (a - b).abs() < 1e-9 {
            num(a)
        } else {
            format!("({}+({})*{p})", num(a), num(b - a))
        }
    };
    format!(
        "zoompan=z='{}':x='(iw-iw/zoom)*{}':y='(ih-ih/zoom)*{}':d={frames}:s={}x{}:fps={FPS}",
        lerp(m.z0, m.z1),
        lerp(m.fx0, m.fx1),
        lerp(m.fy0, m.fy1),
        f.w,
        f.h
    )
}

/// The graph for one chunk: every segment scaled to the frame, moved or looped to its
/// length, joined, trimmed to the chunk's part and handed to the encoder (uploaded to
/// the GPU when `hw_upload`).
pub fn chunk_graph(
    segs: &[Segment],
    chunk: &Chunk,
    fade: u32,
    join: Join,
    frame: Frame,
    hw_upload: bool,
) -> ChunkGraph {
    let part = &segs[chunk.first..=chunk.last];
    let mut inputs = Vec::new();
    let mut g = String::new();
    for (k, s) in part.iter().enumerate() {
        let n = s.frames.max(1);
        match &s.source {
            Source::Still { motion } => {
                inputs.extend(["-i".to_string(), s.path.clone()]);
                // Zooming works on a twice-as-large picture: zoompan positions are whole
                // pixels, and half-pixel steps don't shimmer.
                let (cw, ch) = if motion.is_some() {
                    (frame.w * 2, frame.h * 2)
                } else {
                    (frame.w, frame.h)
                };
                frame_to(
                    &mut g,
                    &format!("[{k}:v]"),
                    &format!("f{k}"),
                    cw,
                    ch,
                    s.fit,
                    &format!("s{k}"),
                );
                let _ = write!(g, "[f{k}]");
                match motion {
                    Some(m) => g.push_str(&zoompan(m, n, frame)),
                    // One frame repeated: loop it and give the copies their timestamps.
                    None => {
                        let _ = write!(g, "loop=loop={}:size=1:start=0,setpts=N/({FPS}*TB)", n - 1);
                    }
                }
                let _ = write!(g, ",fps={FPS},format=yuv420p,setsar=1[v{k}];");
            }
            Source::Clip { start, hdr } => {
                // Read a little more than needed; tpad below covers clips that end early.
                inputs.extend([
                    "-ss".to_string(),
                    num(*start),
                    "-t".to_string(),
                    num(f64::from(n) / f64::from(FPS) + 0.5),
                    "-i".to_string(),
                    s.path.clone(),
                ]);
                let tone = crate::media::tonemap_prefix(*hdr);
                let _ = write!(g, "[{k}:v]{tone}fps={FPS},setpts=PTS-STARTPTS[p{k}];");
                frame_to(
                    &mut g,
                    &format!("[p{k}]"),
                    &format!("f{k}"),
                    frame.w,
                    frame.h,
                    s.fit,
                    &format!("s{k}"),
                );
                let _ = write!(
                    g,
                    "[f{k}]tpad=stop_mode=clone:stop_duration=2,trim=end_frame={n},setpts=PTS-STARTPTS,\
                     fps={FPS},format=yuv420p,setsar=1[v{k}];"
                );
            }
        }
    }
    // Join.
    let mut cur = "[v0]".to_string();
    match join {
        Join::Xfade(kind) if fade > 0 && part.len() > 1 => {
            let mut offset = 0u32;
            for k in 1..part.len() {
                offset += part[k - 1].frames - fade;
                let _ = write!(
                    g,
                    "{cur}[v{k}]xfade=transition={kind}:duration={}:offset={}[x{k}];",
                    secs(fade),
                    secs(offset)
                );
                cur = format!("[x{k}]");
            }
        }
        _ if part.len() > 1 => {
            for k in 0..part.len() {
                let _ = write!(g, "[v{k}]");
            }
            let _ = write!(g, "concat=n={}:v=1:a=0[cat];", part.len());
            cur = "[cat]".into();
        }
        _ => {}
    }
    let upload = if hw_upload {
        "format=nv12,hwupload"
    } else {
        "format=yuv420p"
    };
    let _ = write!(
        g,
        "{cur}trim=start_frame={}:end_frame={},setpts=PTS-STARTPTS,{upload}[vout]",
        chunk.skip,
        chunk.skip + chunk.frames
    );
    ChunkGraph { inputs, filter: g }
}

/// The H.264 encoders Tessera can use, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoder {
    /// The GPU (VA-API), when the engine's test encode worked.
    Vaapi,
    /// x264 (most ffmpeg builds).
    X264,
    /// Cisco's OpenH264 (Fedora's ffmpeg-free has no x264).
    OpenH264,
}

impl Encoder {
    pub fn name(self) -> &'static str {
        match self {
            Encoder::Vaapi => "h264_vaapi",
            Encoder::X264 => "libx264",
            Encoder::OpenH264 => "libopenh264",
        }
    }

    /// Video codec options for this encoder at this frame size.
    pub fn args(self, frame: Frame) -> Vec<String> {
        // OpenH264 has no constant-quality mode: about 0.13 bits per pixel
        // (8 Mbit/s at 1080p30) looks clean for photos.
        let rate =
            (u64::from(frame.w) * u64::from(frame.h) * u64::from(FPS) * 13 / 100).max(1_000_000);
        let v: Vec<String> = match self {
            Encoder::Vaapi => ["-c:v", "h264_vaapi", "-qp", "20", "-profile:v", "high"]
                .map(String::from)
                .to_vec(),
            Encoder::X264 => [
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                "20",
                "-profile:v",
                "high",
                "-pix_fmt",
                "yuv420p",
            ]
            .map(String::from)
            .to_vec(),
            Encoder::OpenH264 => vec![
                "-c:v".into(),
                "libopenh264".into(),
                "-b:v".into(),
                rate.to_string(),
                "-pix_fmt".into(),
                "yuv420p".into(),
            ],
        };
        v
    }
}

/// Arguments for one chunk (output path last).
pub fn chunk_args(
    g: &ChunkGraph,
    enc: Encoder,
    frame: Frame,
    vaapi_device: &str,
    out: &str,
) -> Vec<String> {
    let mut a: Vec<String> = [
        "-v",
        "error",
        "-nostdin",
        "-progress",
        "pipe:1",
        "-nostats",
        "-y",
    ]
    .map(String::from)
    .to_vec();
    if enc == Encoder::Vaapi {
        a.extend(["-vaapi_device".into(), vaapi_device.to_string()]);
    }
    a.extend(g.inputs.iter().cloned());
    a.extend(["-filter_complex".into(), g.filter.clone()]);
    a.extend(["-map", "[vout]", "-an", "-r"].map(String::from));
    a.push(FPS.to_string());
    a.extend(enc.args(frame));
    a.extend(["-f".into(), "mp4".into(), out.to_string()]);
    a
}

/// The concat demuxer's list of chunk files.
pub fn concat_list(files: &[String]) -> String {
    files
        .iter()
        .map(|f| format!("file '{}'\n", f.replace('\'', "'\\''")))
        .collect()
}

/// Joins the chunks without re-encoding and adds the music (looped if it is shorter
/// than the slideshow, faded in briefly and out over the last seconds).
pub fn mux_args(list: &str, music: Option<&str>, total: u32, out: &str) -> Vec<String> {
    let mut a: Vec<String> = [
        "-v",
        "error",
        "-nostdin",
        "-progress",
        "pipe:1",
        "-nostats",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    a.push(list.to_string());
    let seconds = f64::from(total) / f64::from(FPS);
    if let Some(m) = music {
        a.extend(["-stream_loop", "-1", "-i"].map(String::from));
        a.push(m.to_string());
    }
    a.extend(["-map", "0:v:0", "-c:v", "copy"].map(String::from));
    if music.is_some() {
        let fade_out = (seconds / 3.0).min(3.0);
        let fade_in = (seconds / 4.0).min(1.0);
        a.extend(["-map".into(), "1:a:0".into(), "-af".into()]);
        a.push(format!(
            "atrim=duration={},afade=t=in:st=0:d={},afade=t=out:st={}:d={}",
            num(seconds),
            num(fade_in),
            num((seconds - fade_out).max(0.0)),
            num(fade_out)
        ));
        a.extend(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"].map(String::from));
    }
    a.extend(["-t".into(), num(seconds)]);
    a.extend(["-movflags", "+faststart", "-f", "mp4"].map(String::from));
    a.push(out.to_string());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: Frame = Frame { w: 1920, h: 1080 };

    fn still(path: &str, frames: u32, motion: Option<Motion>) -> Segment {
        Segment {
            path: path.into(),
            source: Source::Still { motion },
            frames,
            fit: false,
        }
    }

    #[test]
    fn timeline_overlaps_by_the_transition() {
        let lens = [150, 150, 90];
        assert_eq!(starts(&lens, 24), vec![0, 126, 252]);
        assert_eq!(total_frames(&lens, 24), 390 - 48);
        assert_eq!(total_frames(&lens, 0), 390);
        assert_eq!(total_frames(&[], 24), 0);
        // a 40-frame clip allows at most a 20-frame crossfade; a single segment none
        assert_eq!(clamp_fade(&[150, 40, 150], 24), 20);
        assert_eq!(clamp_fade(&[150], 24), 0);
    }

    #[test]
    fn chunks_cover_the_timeline_exactly_once() {
        for fade in [0u32, 24] {
            let lens: Vec<u32> = (0..13).map(|i| 90 + i * 7).collect();
            let chunks = plan_chunks(&lens, fade, 4);
            assert_eq!(chunks.len(), 4);
            let sum: u32 = chunks.iter().map(|c| c.frames).sum();
            assert_eq!(sum, total_frames(&lens, fade));
            assert_eq!(chunks[0].first, 0);
            assert_eq!(chunks[0].skip, 0);
            for w in chunks.windows(2) {
                if fade > 0 {
                    // the next chunk's first segment is drawn for its transition, then skipped
                    assert_eq!(w[0].last, w[1].first);
                    assert_eq!(w[1].skip, fade);
                } else {
                    assert_eq!(w[0].last + 1, w[1].first);
                    assert_eq!(w[1].skip, 0);
                }
            }
            // a chunk's part fits inside what it renders
            for c in &chunks {
                let rendered = total_frames(&lens[c.first..=c.last], fade);
                assert!(c.skip + c.frames <= rendered, "{c:?} renders {rendered}");
            }
            assert_eq!(chunks.last().unwrap().last, 12);
        }
    }

    #[test]
    fn chunk_boundaries_fall_where_transitions_end() {
        let lens = [150, 150, 150, 150];
        let c = plan_chunks(&lens, 24, 2);
        assert_eq!(
            c,
            vec![
                // segments 0, 1 and the crossfade into 2: ends 24 frames into segment 2
                Chunk {
                    first: 0,
                    last: 2,
                    skip: 0,
                    frames: 252 + 24
                },
                Chunk {
                    first: 2,
                    last: 3,
                    skip: 24,
                    frames: 150 + 126 - 24
                },
            ]
        );
    }

    #[test]
    fn crossfade_graph() {
        let segs = vec![
            still("/w/a.jpg", 150, None),
            still("/w/b.jpg", 150, Some(MOTIONS[0])),
            Segment {
                path: "/m/clip.mp4".into(),
                source: Source::Clip {
                    start: 2.5,
                    hdr: false,
                },
                frames: 90,
                fit: true,
            },
        ];
        let chunk = &plan_chunks(&[150, 150, 90], 24, 8)[0];
        let g = chunk_graph(&segs, chunk, 24, Join::Xfade("fade"), HD, false);
        assert_eq!(
            g.inputs,
            [
                "-i",
                "/w/a.jpg",
                "-i",
                "/w/b.jpg",
                "-ss",
                "2.5",
                "-t",
                "3.5",
                "-i",
                "/m/clip.mp4"
            ]
        );
        let expect = concat!(
            "[0:v]scale=1920:1080:force_original_aspect_ratio=increase:flags=lanczos,crop=1920:1080,setsar=1[f0];",
            "[f0]loop=loop=149:size=1:start=0,setpts=N/(30*TB),fps=30,format=yuv420p,setsar=1[v0];",
            "[1:v]scale=3840:2160:force_original_aspect_ratio=increase:flags=lanczos,crop=3840:2160,setsar=1[f1];",
            "[f1]zoompan=z='(1+(0.18)*on/149)':x='(iw-iw/zoom)*0.5':y='(ih-ih/zoom)*0.5':d=150:s=1920x1080:fps=30,",
            "fps=30,format=yuv420p,setsar=1[v1];",
            "[2:v]fps=30,setpts=PTS-STARTPTS[p2];",
            "[p2]split=2[s2a][s2b];",
            "[s2a]scale=240:134:force_original_aspect_ratio=increase,crop=240:134,gblur=sigma=6,",
            "scale=1920:1080,lutyuv=y=val*0.72,setsar=1[s2bg];",
            "[s2b]scale=1920:1080:force_original_aspect_ratio=decrease:flags=lanczos,setsar=1[s2fg];",
            "[s2bg][s2fg]overlay=(W-w)/2:(H-h)/2[f2];",
            "[f2]tpad=stop_mode=clone:stop_duration=2,trim=end_frame=90,setpts=PTS-STARTPTS,fps=30,format=yuv420p,setsar=1[v2];",
            "[v0][v1]xfade=transition=fade:duration=0.8:offset=4.2[x1];",
            "[x1][v2]xfade=transition=fade:duration=0.8:offset=8.4[x2];",
            "[x2]trim=start_frame=0:end_frame=342,setpts=PTS-STARTPTS,format=yuv420p[vout]",
        );
        assert_eq!(g.filter, expect);
    }

    #[test]
    fn cut_graph_and_gpu_upload() {
        let segs = vec![
            still("/w/a.jpg", 60, None),
            still("/w/b.jpg", 60, None),
            still("/w/c.jpg", 60, None),
        ];
        let chunks = plan_chunks(&[60, 60, 60], 0, 2);
        let g = chunk_graph(&segs, &chunks[1], 0, Join::Cut, HD, true);
        assert_eq!(g.inputs, ["-i", "/w/c.jpg"]);
        assert!(g.filter.ends_with(
            "[v0]trim=start_frame=0:end_frame=60,setpts=PTS-STARTPTS,format=nv12,hwupload[vout]"
        ));
        let g = chunk_graph(&segs, &chunks[0], 0, Join::Cut, HD, false);
        assert!(g
            .filter
            .contains("[v0][v1]concat=n=2:v=1:a=0[cat];[cat]trim=start_frame=0:end_frame=120"));
        let a = chunk_args(
            &g,
            Encoder::Vaapi,
            HD,
            "/dev/dri/renderD128",
            "/w/chunk.mp4",
        );
        assert_eq!(&a[7..9], ["-vaapi_device", "/dev/dri/renderD128"]);
        assert!(a.windows(2).any(|w| w == ["-c:v", "h264_vaapi"]));
        assert_eq!(a.last().unwrap(), "/w/chunk.mp4");
    }

    #[test]
    fn motion_is_deterministic() {
        assert_eq!(motion_for("/lib/a.jpg"), motion_for("/lib/a.jpg"));
        let distinct: std::collections::HashSet<String> = (0..40)
            .map(|i| format!("{:?}", motion_for(&format!("/lib/{i}.jpg"))))
            .collect();
        assert!(distinct.len() > 3, "moves should vary between photos");
    }

    #[test]
    fn mux_fades_the_music_out() {
        let a = mux_args(
            "/w/list.txt",
            Some("/music/song.mp3"),
            300,
            "/out/.x.part.mp4",
        );
        let af = &a[a.iter().position(|x| x == "-af").unwrap() + 1];
        assert_eq!(
            af,
            "atrim=duration=10,afade=t=in:st=0:d=1,afade=t=out:st=7:d=3"
        );
        assert!(a
            .windows(4)
            .any(|w| w == ["-stream_loop", "-1", "-i", "/music/song.mp3"]));
        assert!(a.windows(2).any(|w| w == ["-movflags", "+faststart"]));
        let silent = mux_args("/w/list.txt", None, 300, "/out/x.mp4");
        assert!(!silent.iter().any(|x| x == "-af" || x == "1:a:0"));
        assert_eq!(
            concat_list(&["/w/it's.mp4".into()]),
            "file '/w/it'\\''s.mp4'\n"
        );
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(num(2.0), "2");
        assert_eq!(num(0.8), "0.8");
        assert_eq!(secs(1), "0.033333");
        assert_eq!(num(-0.18), "-0.18");
    }
}
