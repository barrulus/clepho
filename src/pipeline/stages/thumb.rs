//! Thumbnail stage: produce a max-edge thumbnail PNG to the cache dir.

use crate::db::clock::Clock;
use crate::pipeline::stages::{pending_default, PendingPhoto, Stage, StageId, StageOutcome};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

#[allow(dead_code)]
pub struct ThumbStage {
    pub cache_dir: Arc<PathBuf>,
    pub max_edge: u32,
}

#[allow(dead_code)]
impl ThumbStage {
    pub fn new(cache_dir: PathBuf, max_edge: u32) -> Self {
        Self {
            cache_dir: Arc::new(cache_dir),
            max_edge,
        }
    }

    /// Two-level shard so directories stay small: ab/cd/<full_hash>.png
    fn thumb_path(&self, file_hash: &str) -> PathBuf {
        let (a, rest) = file_hash.split_at(2);
        let (b, _) = rest.split_at(2);
        self.cache_dir
            .join(a)
            .join(b)
            .join(format!("{}.png", file_hash))
    }
}

impl Stage for ThumbStage {
    fn id(&self) -> StageId {
        StageId::Thumb
    }

    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>> {
        pending_default(conn, "thumb_done_at", Some("exif_done_at"), folder, limit)
    }

    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        _clock: &dyn Clock,
    ) -> Result<StageOutcome> {
        let hash: Option<String> = conn.query_row(
            "SELECT file_hash FROM photos WHERE id=?1",
            rusqlite::params![photo.id],
            |r| r.get(0),
        )?;
        let Some(hash) = hash else {
            return Ok(StageOutcome::Err {
                error_class: "missing_hash".into(),
                message: "scan must run before thumb".into(),
            });
        };
        if hash.len() < 4 {
            return Ok(StageOutcome::Err {
                error_class: "missing_hash".into(),
                message: "file_hash too short for shard".into(),
            });
        }

        let img = match image::open(&photo.path) {
            Ok(img) => img,
            Err(e) => {
                return Ok(StageOutcome::Err {
                    error_class: "image_decode".into(),
                    message: format!("decode: {}", e),
                });
            }
        };

        let thumb = img.thumbnail(self.max_edge, self.max_edge);
        let out_path = self.thumb_path(&hash);
        std::fs::create_dir_all(out_path.parent().unwrap()).context("mkdir thumb shard")?;
        thumb
            .save_with_format(&out_path, image::ImageFormat::Png)
            .with_context(|| format!("save thumb {:?}", out_path))?;
        Ok(StageOutcome::Ok)
    }
}
