//! Duplicate detection (identical content hash, similar perceptual hash), live-photo
//! pairing helpers and the camera list.

use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};

use super::{escape_like, Camera, File, Store, FILE_COLS};

/// A set of files that are the same (identical content) or look the same (perceptual hash
/// within a few bits).
#[derive(Debug, Clone)]
pub struct DupGroup {
    /// identical | similar
    pub kind: &'static str,
    pub files: Vec<File>,
}

impl Store {
    /// Images whose thumbnail-derived hash is still to compute.
    pub fn missing_phash(&self, limit: i64) -> anyhow::Result<Vec<File>> {
        self.query(
            &format!("SELECT {FILE_COLS} FROM files WHERE kind IN ('image','gif') AND probed=1 AND probe_error='' AND phash='' LIMIT ?"),
            &[limit.into()],
        )
    }

    /// Identical files (same content hash) and visually similar images (dHash Hamming
    /// distance ≤ `max_dist`). Similar groups that are entirely identical copies are not
    /// repeated.
    pub fn duplicates(&self, max_dist: u32) -> anyhow::Result<Vec<DupGroup>> {
        let mut out = Vec::new();
        let exact = self.query(
            &format!(
                "SELECT {FILE_COLS} FROM files WHERE chash<>'' AND chash IN
		(SELECT chash FROM files WHERE chash<>'' GROUP BY chash HAVING count(*)>1) ORDER BY chash, mtime"
            ),
            &[],
        )?;
        let mut i = 0;
        while i < exact.len() {
            let mut j = i;
            while j < exact.len() && exact[j].chash == exact[i].chash {
                j += 1;
            }
            out.push(DupGroup {
                kind: "identical",
                files: exact[i..j].to_vec(),
            });
            i = j;
        }

        let imgs = self.query(
            &format!("SELECT {FILE_COLS} FROM files WHERE phash<>'' AND kind IN ('image','gif')"),
            &[],
        )?;
        let hashes: Vec<u64> = imgs
            .iter()
            .map(|f| u64::from_str_radix(&f.phash, 16).unwrap_or(0))
            .collect();
        // Multi-index hashing: two hashes within 7 bits share at least one of their 8
        // bytes, so only files sharing a byte at the same position are compared.
        let mut parent: Vec<usize> = (0..imgs.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for pos in 0..8 {
            let mut buckets: HashMap<u8, Vec<usize>> = HashMap::new();
            for (i, &h) in hashes.iter().enumerate() {
                if h == 0 || h == u64::MAX {
                    // flat images (all one colour) say nothing
                    continue;
                }
                buckets.entry((h >> (8 * pos)) as u8).or_default().push(i);
            }
            for b in buckets.values() {
                if b.len() > 2000 {
                    // degenerate bucket: skip rather than go quadratic
                    continue;
                }
                for x in 0..b.len() {
                    for y in x + 1..b.len() {
                        if (hashes[b[x]] ^ hashes[b[y]]).count_ones() <= max_dist {
                            let rx = find(&mut parent, b[x]);
                            let ry = find(&mut parent, b[y]);
                            parent[rx] = ry;
                        }
                    }
                }
            }
        }
        let mut groups: HashMap<usize, Vec<File>> = HashMap::new();
        for (i, f) in imgs.into_iter().enumerate() {
            let r = find(&mut parent, i);
            groups.entry(r).or_default().push(f);
        }
        let mut similar = Vec::new();
        for (_, mut g) in groups {
            if g.len() < 2 {
                continue;
            }
            let same = g[1..]
                .iter()
                .all(|f| !f.chash.is_empty() && f.chash == g[0].chash);
            if same {
                continue;
            }
            g.sort_by_key(|f| f.mtime);
            similar.push(DupGroup {
                kind: "similar",
                files: g,
            });
        }
        similar.sort_by(|a, b| b.files.len().cmp(&a.files.len()));
        out.extend(similar);
        Ok(out)
    }

    /// The paired clip of an Apple Live Photo: a video with the same name (minus
    /// extension) in the same folder.
    pub fn sibling_video(&self, library: &str, dir: &str, stem: &str) -> anyhow::Result<Option<String>> {
        let c = self.pool.get()?;
        Ok(c.query_row(
            "SELECT path FROM files WHERE library=? AND dir=? AND kind='video' AND name LIKE ? ESCAPE '\\' AND duration<=6 LIMIT 1",
            params![library, dir, format!("{}.%", escape_like(stem))],
            |r| r.get(0),
        )
        .optional()?)
    }

    /// Records a live photo pairing found after probing.
    pub fn set_motion(&self, id: i64, motion: &str) -> anyhow::Result<()> {
        let c = self.pool.get()?;
        c.execute("UPDATE files SET motion=? WHERE id=?", params![motion, id])?;
        Ok(())
    }

    /// Camera models with counts (search filter).
    pub fn cameras(&self) -> anyhow::Result<Vec<Camera>> {
        let c = self.pool.get()?;
        let mut st = c.prepare_cached("SELECT camera, count(*) FROM files WHERE camera<>'' GROUP BY camera ORDER BY count(*) DESC LIMIT 200")?;
        let rows = st.query_map([], |r| {
            Ok(Camera {
                name: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}
