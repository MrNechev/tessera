//! Live Photos / motion photos.
//!
//! - Google/Samsung "motion photos" append an MP4 to the JPEG (or HEIC). Their XMP near the
//!   start says so (MotionPhoto / MicroVideo), so only those files are scanned.
//! - Apple Live Photos are an image plus a short .MOV with the same name in the same folder;
//!   the library pairs them when probing.
//!
//! Either way the clip is converted once into a small muted H.264 file for hover playback.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;

use super::probe::path_arg;
use super::{exists, publish, shard, tmp_for, tonemap_prefix, Manager, RemoveOnDrop};
use crate::jobs::{JobError, Priority};
use crate::store::File;

const MAX_MOTION_SCAN: u64 = 64 << 20;

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn last_index(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len())
        .rev()
        .find(|&i| &hay[i..i + needle.len()] == needle)
}

/// The offset of an MP4 appended to an image, or 0.
pub fn embedded_motion(path: &Path) -> i64 {
    let Ok(mut f) = std::fs::File::open(path) else {
        return 0;
    };
    let mut head = Vec::with_capacity(256 << 10);
    if (&mut f).take(256 << 10).read_to_end(&mut head).is_err() {
        return 0;
    }
    if !contains(&head, b"MotionPhoto") && !contains(&head, b"MicroVideo") {
        return 0;
    }
    let Ok(md) = f.metadata() else { return 0 };
    if md.len() > MAX_MOTION_SCAN {
        return 0;
    }
    if f.seek(SeekFrom::Start(0)).is_err() {
        return 0;
    }
    let mut b = Vec::new();
    if f.read_to_end(&mut b).is_err() {
        return 0;
    }
    // the last "ftyp" box whose brand looks like MP4 is the start of the video
    let mut i = b.len() as i64 - 8;
    while i >= 8 {
        let Some(j) = last_index(&b[..i as usize], b"ftyp") else {
            break;
        };
        if j < 4 {
            break;
        }
        let brand = &b[j + 4..(j + 8).min(b.len())];
        if brand.starts_with(b"mp4") || brand.starts_with(b"isom") || brand.starts_with(b"iso") || brand == b"qt  " {
            return (j - 4) as i64;
        }
        i = j as i64;
    }
    0
}

impl Manager {
    pub fn motion_path(&self, key: &str) -> PathBuf {
        self.cache_dir
            .join("motion")
            .join(shard(key))
            .join(format!("{key}.mp4"))
    }

    /// Makes (and waits for) the hover clip of a live/motion photo. `pair_abs` is the paired
    /// video of an Apple Live Photo, or `None` for an embedded clip.
    pub async fn motion(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        pair_abs: Option<PathBuf>,
    ) -> Result<PathBuf, JobError> {
        let out = self.motion_path(&f.key);
        if exists(&out) {
            return Ok(out);
        }
        let job = self.motion_job(f, abs, pair_abs, out.clone());
        self.gifs.wait(&format!("m{}", f.key), job).await?;
        Ok(out)
    }

    /// Queues the hover clip as background work.
    pub fn motion_queue(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        pair_abs: Option<PathBuf>,
    ) -> Result<(), JobError> {
        let out = self.motion_path(&f.key);
        if exists(&out) {
            return Ok(());
        }
        let job = self.motion_job(f, abs, pair_abs, out);
        self.gifs
            .submit(&format!("m{}", f.key), Priority::Low, false, job)
            .map(|_| ())
    }

    fn motion_job(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        pair_abs: Option<PathBuf>,
        out: PathBuf,
    ) -> impl FnOnce(&crate::jobs::JobCtx) -> anyhow::Result<()> + Send + 'static {
        let m = self.clone();
        let fc = f.clone();
        let abs = abs.to_path_buf();
        move |ctx| {
            if exists(&out) {
                return Ok(());
            }
            m.make_motion(ctx, &fc, &abs, pair_abs.as_deref(), &out)
        }
    }

    fn make_motion(
        &self,
        ctx: &crate::jobs::JobCtx,
        f: &File,
        abs: &Path,
        pair_abs: Option<&Path>,
        out: &Path,
    ) -> anyhow::Result<()> {
        if let Some(d) = out.parent() {
            std::fs::create_dir_all(d)?;
        }
        let ex = self.exec(&ctx.cancel, Duration::from_secs(180), ctx.background());
        let mut _src_guard = None;
        let src: PathBuf = match pair_abs {
            Some(p) => p.to_path_buf(),
            None => {
                let off: i64 = f
                    .motion
                    .strip_prefix("embed:")
                    .unwrap_or(&f.motion)
                    .parse()
                    .unwrap_or(0);
                if off <= 0 {
                    bail!("no motion clip");
                }
                // copy the embedded MP4 out (ffmpeg needs a seekable file that starts with it)
                let mut input = std::fs::File::open(abs)?;
                input.seek(SeekFrom::Start(off as u64))?;
                let stem = out.to_string_lossy();
                let stem = stem.strip_suffix(".mp4").unwrap_or(&stem);
                let tmp_in = tmp_for(Path::new(&format!("{stem}-src.mp4")));
                _src_guard = Some(RemoveOnDrop(tmp_in.clone()));
                let mut w = std::fs::File::create(&tmp_in)?;
                std::io::copy(&mut input.take(MAX_MOTION_SCAN), &mut w)?;
                tmp_in
            }
        };
        let hdr = self
            .probe_ff(&ctx.cancel, &src)
            .map(|p| p.hdr)
            .unwrap_or(false);
        let tmp = tmp_for(out);
        let _g = RemoveOnDrop(tmp.clone());
        let vf = format!(
            "{}scale='if(gt(iw,ih),-2,min(720,iw))':'if(gt(iw,ih),min(720,ih),-2)':flags=bicubic,format=yuv420p",
            tonemap_prefix(hdr)
        );
        let args: Vec<String> = [
            "-v", "error", "-nostdin", "-i",
        ]
        .iter()
        .map(|s| s.to_string())
        .chain([path_arg(&src)])
        .chain(
            [
                "-an", "-t", "10", "-vf", &vf, "-c:v", "libx264", "-preset", "veryfast", "-crf",
                "26", "-movflags", "+faststart", "-map_metadata", "-1", "-y",
            ]
            .iter()
            .map(|s| s.to_string()),
        )
        .chain([path_arg(&tmp)])
        .collect();
        ex.run("ffmpeg", &args)?;
        publish(&tmp, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_jpeg() -> Vec<u8> {
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut out)
            .encode_image(&image::RgbImage::new(32, 32))
            .unwrap();
        out
    }

    #[test]
    fn embedded_motion_offset() {
        let img = tiny_jpeg();
        // XMP marker near the start (inside a COM segment is enough for detection)
        let mut file = img[..2].to_vec();
        let xmp = b"GCamera:MotionPhoto=\"1\"";
        file.extend_from_slice(&[0xFF, 0xFE, 0, (xmp.len() + 2) as u8]);
        file.extend_from_slice(xmp);
        file.extend_from_slice(&img[2..]);
        let off = file.len();
        file.extend_from_slice(&[0, 0, 0, 0x18]);
        file.extend_from_slice(b"ftypmp42");
        file.extend_from_slice(&[0u8; 64]);
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("PXL.MP.jpg");
        std::fs::write(&p, &file).unwrap();
        assert_eq!(embedded_motion(&p), off as i64);
        let plain = dir.path().join("plain.jpg");
        let mut b = img.clone();
        b.extend_from_slice(b"....ftypmp42");
        std::fs::write(&plain, &b).unwrap();
        assert_eq!(embedded_motion(&plain), 0, "plain JPEG detected as motion photo");
    }
}
