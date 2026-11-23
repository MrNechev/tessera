//! Sprite sheets: a grid of small frames sampled evenly across a video, used for hover
//! scrubbing in the library and for the clip timeline. One JPEG per video; the layout is
//! derived from the duration so the client can compute it (see [`sprite_layout`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::bail;

use super::kinds::KIND_VIDEO;
use super::probe::path_arg;
use super::{exists, publish, shard, tmp_for, tonemap_prefix, Manager, RemoveOnDrop};
use crate::jobs::{JobCtx, JobError, Priority};
use crate::store::File;

pub const SPRITE_COLS: i64 = 10;
pub const SPRITE_TILE_W: i64 = 160;

/// How many frames a video's sprite has and the tile height.
/// Mirrored in the web UI (`spriteLayout` in api.ts).
pub fn sprite_layout(f: &File) -> (i64, i64) {
    let n = ((f.duration / 2.0) as i64).clamp(10, 60);
    let mut tile_h = 90;
    if f.width > 0 && f.height > 0 {
        tile_h = SPRITE_TILE_W * f.height / f.width;
        tile_h -= tile_h % 2;
        tile_h = tile_h.clamp(40, 320);
    }
    (n, tile_h)
}

impl Manager {
    pub fn sprite_path(&self, key: &str) -> PathBuf {
        self.cache_dir
            .join("sprite")
            .join(shard(key))
            .join(format!("{key}.jpg"))
    }

    /// Makes (and waits for) a video's sprite sheet.
    pub async fn sprite(self: &Arc<Self>, f: &File, abs: &Path) -> Result<PathBuf, JobError> {
        let out = self.sprite_path(&f.key);
        if exists(&out) {
            return Ok(out);
        }
        if f.kind != KIND_VIDEO || f.duration <= 0.0 {
            return Err(JobError::failed("not a video"));
        }
        let job = self.sprite_job(f, abs, out.clone());
        self.thumbs.wait(&format!("s{}", f.key), job).await?;
        Ok(out)
    }

    /// Queues a sprite sheet as background work.
    pub fn sprite_queue(self: &Arc<Self>, f: &File, abs: &Path) -> Result<(), JobError> {
        let out = self.sprite_path(&f.key);
        if exists(&out) {
            return Ok(());
        }
        if f.kind != KIND_VIDEO || f.duration <= 0.0 {
            return Err(JobError::failed("not a video"));
        }
        let job = self.sprite_job(f, abs, out);
        self.thumbs
            .submit(&format!("s{}", f.key), Priority::Low, false, job)
            .map(|_| ())
    }

    fn sprite_job(
        self: &Arc<Self>,
        f: &File,
        abs: &Path,
        out: PathBuf,
    ) -> impl FnOnce(&JobCtx) -> anyhow::Result<()> + Send + 'static {
        let m = self.clone();
        let fc = f.clone();
        let abs = abs.to_path_buf();
        move |ctx| {
            if exists(&out) {
                return Ok(());
            }
            m.make_sprite(ctx, &fc, &abs, &out)
        }
    }

    fn make_sprite(&self, ctx: &JobCtx, f: &File, abs: &Path, out: &Path) -> anyhow::Result<()> {
        let parent = out.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)?;
        let ex = self.exec(&ctx.cancel, Duration::from_secs(300), ctx.background());
        let (n, th) = sprite_layout(f);
        let rows = (n + SPRITE_COLS - 1) / SPRITE_COLS;
        let mut sheet =
            image::RgbImage::new((SPRITE_COLS * SPRITE_TILE_W) as u32, (rows * th) as u32);
        let dir = tempdir_in(parent, ".sprite-")?;
        let _g = RemoveDirOnDrop(dir.clone());
        let vf = format!(
            "{}scale={SPRITE_TILE_W}:{th}:force_original_aspect_ratio=decrease,pad={SPRITE_TILE_W}:{th}:-1:-1:color=black",
            tonemap_prefix(f.hdr)
        );
        let mut got = 0;
        for i in 0..n {
            // sample the middle of each slot; keyframe seeking keeps this fast on long files
            let t = f.duration * (i as f64 + 0.5) / n as f64;
            let p = dir.join(format!("{i:03}.jpg"));
            let args: Vec<String> = vec![
                "-v".into(),
                "error".into(),
                "-nostdin".into(),
                "-ss".into(),
                format!("{t:.3}"),
                "-i".into(),
                path_arg(abs),
                "-frames:v".into(),
                "1".into(),
                "-vf".into(),
                vf.clone(),
                "-q:v".into(),
                "5".into(),
                "-y".into(),
                path_arg(&p),
            ];
            if let Err(e) = ex.run("ffmpeg", &args) {
                if super::cmd::is_cancelled(&e) {
                    return Err(e);
                }
                continue;
            }
            let Ok(img) = image::open(&p) else { continue };
            let img = img.to_rgb8();
            let (x, y) = ((i % SPRITE_COLS) * SPRITE_TILE_W, (i / SPRITE_COLS) * th);
            for (px, py, c) in img.enumerate_pixels() {
                let (dx, dy) = (x as u32 + px, y as u32 + py);
                if (px as i64) < SPRITE_TILE_W
                    && (py as i64) < th
                    && dx < sheet.width()
                    && dy < sheet.height()
                {
                    sheet.put_pixel(dx, dy, *c);
                }
            }
            got += 1;
        }
        if got == 0 {
            bail!("no frames");
        }
        let tmp = tmp_for(out);
        let _t = RemoveOnDrop(tmp.clone());
        {
            let w = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(w, 78).encode_image(&sheet)?;
        }
        publish(&tmp, out)
    }
}

fn tempdir_in(parent: &Path, prefix: &str) -> anyhow::Result<PathBuf> {
    for _ in 0..16 {
        let mut b = [0u8; 6];
        rand::RngCore::fill_bytes(&mut rand::rng(), &mut b);
        let p = parent.join(format!("{prefix}{}", hex::encode(b)));
        match std::fs::create_dir(&p) {
            Ok(()) => return Ok(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("could not create a temporary directory")
}

struct RemoveDirOnDrop(PathBuf);

impl Drop for RemoveDirOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout() {
        let f = File {
            duration: 300.0,
            width: 1920,
            height: 1080,
            ..Default::default()
        };
        assert_eq!(sprite_layout(&f), (60, 90));
        let f = File {
            duration: 5.0,
            width: 1080,
            height: 1920,
            ..Default::default()
        };
        assert_eq!(sprite_layout(&f), (10, 284));
        assert_eq!(sprite_layout(&File::default()), (10, 90));
    }
}
