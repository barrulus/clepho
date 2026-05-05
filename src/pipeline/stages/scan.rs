//! Scan stage: discover files, compute hash, populate basic metadata.
//! Idempotent: on rerun, re-uses existing row if path exists.

use crate::db::clock::Clock;
use crate::pipeline::stages::{pending_default, PendingPhoto, Stage, StageId, StageOutcome};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[allow(dead_code)]
pub struct ScanStage {
    pub root_paths_for_walk: Arc<Vec<PathBuf>>,
    pub allowed_extensions: Arc<Vec<String>>,
}

#[allow(dead_code)]
impl ScanStage {
    pub fn new(roots: Vec<PathBuf>, exts: Vec<String>) -> Self {
        Self {
            root_paths_for_walk: Arc::new(roots),
            allowed_extensions: Arc::new(exts.into_iter().map(|e| e.to_lowercase()).collect()),
        }
    }

    /// Discover candidate files under `folder` and INSERT photo rows for any
    /// not yet present. Returns the number of files seen (whether inserted or
    /// already present).
    pub fn discover(&self, conn: &Connection, folder: &Path) -> Result<usize> {
        let mut count = 0usize;
        let walker = walkdir::WalkDir::new(folder).follow_links(false);
        for entry in walker.into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !self.allowed_extensions.iter().any(|e| e == &ext) {
                continue;
            }

            let path_str = path.to_string_lossy();
            conn.execute(
                "INSERT OR IGNORE INTO photos(path) VALUES (?1)",
                rusqlite::params![path_str],
            )?;
            count += 1;
        }
        Ok(count)
    }
}

impl Stage for ScanStage {
    fn id(&self) -> StageId {
        StageId::Scan
    }

    fn pending(
        &self,
        conn: &Connection,
        folder: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PendingPhoto>> {
        pending_default(conn, "scan_done_at", None, folder, limit)
    }

    fn process_one(
        &self,
        conn: &Connection,
        photo: &PendingPhoto,
        _clock: &dyn Clock,
    ) -> Result<StageOutcome> {
        let path = &photo.path;
        let metadata = match std::fs::metadata(path) {
            Ok(m) => m,
            Err(e) => {
                return Ok(StageOutcome::Err {
                    error_class: "fs_missing".into(),
                    message: format!("stat failed: {}", e),
                });
            }
        };
        if !metadata.is_file() {
            return Ok(StageOutcome::Err {
                error_class: "fs_not_file".into(),
                message: "path is not a regular file".into(),
            });
        }
        let size = metadata.len() as i64;

        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                return Ok(StageOutcome::Err {
                    error_class: "image_decode".into(),
                    message: format!("read failed: {}", e),
                });
            }
        };

        use sha2::{Digest, Sha256};
        let hash = format!("{:x}", Sha256::digest(&bytes));

        let (width, height, mime) = match image::load_from_memory(&bytes) {
            Ok(img) => {
                let (w, h) = (img.width() as i64, img.height() as i64);
                let mime = mime_from_path(path).unwrap_or("application/octet-stream");
                (Some(w), Some(h), Some(mime.to_string()))
            }
            Err(_) => (None, None, None),
        };

        conn.execute(
            "UPDATE photos
             SET file_hash=?1, file_size=?2, width=?3, height=?4, mime=?5,
                 updated_at=CURRENT_TIMESTAMP
             WHERE id=?6",
            rusqlite::params![hash, size, width, height, mime, photo.id],
        )
        .context("update photo scan results")?;

        Ok(StageOutcome::Ok)
    }
}

fn mime_from_path(p: &Path) -> Option<&'static str> {
    let ext = p.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "heic" => "image/heic",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "gif" => "image/gif",
        _ => return None,
    })
}
