pub mod change_detection;
pub mod discovery;
pub mod hashing;
pub mod metadata;
pub mod thumbnails;

use anyhow::Result;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::config::Config;
use crate::db::Database;
use crate::tasks::{TaskUpdate, TaskProgress};

pub use change_detection::{detect_changes, ChangeDetectionResult};
pub use discovery::discover_images;
pub use hashing::HashResult;
pub use metadata::ImageMetadata;
pub use thumbnails::ThumbnailManager;

#[derive(Debug, Clone)]
pub struct ScannedPhoto {
    pub path: PathBuf,
    pub filename: String,
    pub directory: String,
    pub size_bytes: u64,
    pub modified_at: Option<String>,
    pub metadata: Option<ImageMetadata>,
    pub hashes: Option<HashResult>,
}

pub struct Scanner {
    config: Config,
    thumbnail_manager: ThumbnailManager,
}

impl Scanner {
    pub fn new(config: Config) -> Self {
        let thumbnail_manager = ThumbnailManager::new(&config.thumbnails);
        Self { config, thumbnail_manager }
    }

    /// Scan directory with cancellation support via TaskUpdate protocol.
    /// Uses parallel processing for faster scanning.
    pub fn scan_directory_cancellable(
        &self,
        directory: &PathBuf,
        db: &Database,
        tx: mpsc::Sender<TaskUpdate>,
        cancel_flag: Arc<AtomicBool>,
    ) {
        // Discover all image files
        let image_paths = match discover_images(directory, &self.config.scanner.image_extensions) {
            Ok(paths) => paths,
            Err(e) => {
                let _ = tx.send(TaskUpdate::Failed {
                    error: format!("Failed to discover images: {}", e),
                });
                return;
            }
        };

        let total = image_paths.len();
        let _ = tx.send(TaskUpdate::Started { total });

        if total == 0 {
            let _ = tx.send(TaskUpdate::Completed {
                message: "No images found".to_string(),
            });
            return;
        }

        // Progress counter for parallel processing
        let progress_counter = Arc::new(AtomicUsize::new(0));

        // Process images in parallel
        let tx_clone = tx.clone();
        let cancel_clone = cancel_flag.clone();
        let progress_clone = progress_counter.clone();

        let scanned_photos: Vec<(PathBuf, Result<ScannedPhoto>)> = image_paths
            .par_iter()
            .map(|path| {
                // Check for cancellation
                if cancel_clone.load(Ordering::SeqCst) {
                    return (path.clone(), Err(anyhow::anyhow!("Cancelled")));
                }

                // Update progress
                let current = progress_clone.fetch_add(1, Ordering::SeqCst) + 1;
                let filename = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let _ = tx_clone.send(TaskUpdate::Progress(
                    TaskProgress::new(current, total).with_item(&filename)
                ));

                // Scan the file (expensive operation - done in parallel)
                let result = self.scan_single_file(path);
                (path.clone(), result)
            })
            .collect();

        // Check if cancelled during parallel processing
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = tx.send(TaskUpdate::Cancelled);
            return;
        }

        // Insert/update database sequentially (SQLite prefers this)
        let mut scanned = 0;
        let mut new_count = 0;
        let mut updated_count = 0;

        for (path, result) in scanned_photos {
            match result {
                Ok(photo) => {
                    match self.photo_exists(db, &path) {
                        Ok(exists) => {
                            if exists {
                                if let Err(e) = self.update_photo(db, &photo) {
                                    tracing::error!(path = %path.display(), error = %e, "Error updating photo");
                                } else {
                                    updated_count += 1;
                                }
                            } else {
                                if let Err(e) = self.insert_photo(db, &photo) {
                                    tracing::error!(path = %path.display(), error = %e, "Error inserting photo");
                                } else {
                                    new_count += 1;
                                }
                            }
                            scanned += 1;
                        }
                        Err(e) => {
                            tracing::error!(path = %path.display(), error = %e, "Error checking photo existence");
                        }
                    }
                }
                Err(e) => {
                    if !e.to_string().contains("Cancelled") {
                        tracing::error!(path = %path.display(), error = %e, "Error scanning photo");
                    }
                }
            }
        }

        let _ = tx.send(TaskUpdate::Completed {
            message: format!("{} scanned, {} new, {} updated", scanned, new_count, updated_count),
        });
    }

    fn scan_single_file(&self, path: &PathBuf) -> Result<ScannedPhoto> {
        let file_metadata = std::fs::metadata(path)?;
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let directory = path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Get file modification time as ISO timestamp
        let modified_at = file_metadata
            .modified()
            .ok()
            .and_then(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                Some(datetime.format("%Y-%m-%dT%H:%M:%S").to_string())
            });

        // Extract image metadata (EXIF, dimensions)
        let metadata = metadata::extract_metadata(path).ok();

        // Calculate hashes
        let hashes = hashing::calculate_hashes(path).ok();

        // Generate thumbnail (cached)
        let _ = self.thumbnail_manager.generate(path);

        Ok(ScannedPhoto {
            path: path.clone(),
            filename,
            directory,
            size_bytes: file_metadata.len(),
            modified_at,
            metadata,
            hashes,
        })
    }

    fn photo_exists(&self, db: &Database, path: &PathBuf) -> Result<bool> {
        let path_str = path.to_string_lossy();
        let count: i64 = db.conn().query_row(
            "SELECT COUNT(*) FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    fn insert_photo(&self, db: &Database, photo: &ScannedPhoto) -> Result<()> {
        let path_str = photo.path.to_string_lossy();

        let (width, height, format, camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at, gps_lat, gps_lon, all_exif, orientation) =
            if let Some(ref meta) = photo.metadata {
                (
                    meta.width,
                    meta.height,
                    meta.format.clone(),
                    meta.camera_make.clone(),
                    meta.camera_model.clone(),
                    meta.lens.clone(),
                    meta.focal_length,
                    meta.aperture,
                    meta.shutter_speed.clone(),
                    meta.iso,
                    meta.taken_at.clone(),
                    meta.gps_latitude,
                    meta.gps_longitude,
                    meta.all_exif.clone(),
                    meta.orientation,
                )
            } else {
                (None, None, None, None, None, None, None, None, None, None, None, None, None, None, None)
            };

        let (md5_hash, sha256_hash, perceptual_hash) = if let Some(ref hashes) = photo.hashes {
            (
                Some(hashes.md5.clone()),
                Some(hashes.sha256.clone()),
                hashes.perceptual.clone(),
            )
        } else {
            (None, None, None)
        };

        db.conn().execute(
            r#"
            INSERT INTO photos (
                path, filename, directory, size_bytes, modified_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                gps_latitude, gps_longitude, all_exif,
                md5_hash, sha256_hash, perceptual_hash,
                exif_orientation
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![
                path_str.as_ref(),
                photo.filename,
                photo.directory,
                photo.size_bytes as i64,
                photo.modified_at,
                width,
                height,
                format,
                camera_make,
                camera_model,
                lens,
                focal_length,
                aperture,
                shutter_speed,
                iso,
                taken_at,
                gps_lat,
                gps_lon,
                all_exif,
                md5_hash,
                sha256_hash,
                perceptual_hash,
                orientation.unwrap_or(1) as i32,
            ],
        )?;

        Ok(())
    }

    fn update_photo(&self, db: &Database, photo: &ScannedPhoto) -> Result<()> {
        let path_str = photo.path.to_string_lossy();

        let (width, height, format, camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at, gps_lat, gps_lon, all_exif, orientation) =
            if let Some(ref meta) = photo.metadata {
                (
                    meta.width,
                    meta.height,
                    meta.format.clone(),
                    meta.camera_make.clone(),
                    meta.camera_model.clone(),
                    meta.lens.clone(),
                    meta.focal_length,
                    meta.aperture,
                    meta.shutter_speed.clone(),
                    meta.iso,
                    meta.taken_at.clone(),
                    meta.gps_latitude,
                    meta.gps_longitude,
                    meta.all_exif.clone(),
                    meta.orientation,
                )
            } else {
                (None, None, None, None, None, None, None, None, None, None, None, None, None, None, None)
            };

        let (md5_hash, sha256_hash, perceptual_hash) = if let Some(ref hashes) = photo.hashes {
            (
                Some(hashes.md5.clone()),
                Some(hashes.sha256.clone()),
                hashes.perceptual.clone(),
            )
        } else {
            (None, None, None)
        };

        db.conn().execute(
            r#"
            UPDATE photos SET
                filename = ?, directory = ?, size_bytes = ?, modified_at = ?,
                width = ?, height = ?, format = ?,
                camera_make = ?, camera_model = ?, lens = ?, focal_length = ?, aperture = ?, shutter_speed = ?, iso = ?, taken_at = ?,
                gps_latitude = ?, gps_longitude = ?, all_exif = ?,
                md5_hash = ?, sha256_hash = ?, perceptual_hash = ?,
                exif_orientation = ?,
                scanned_at = CURRENT_TIMESTAMP
            WHERE path = ?
            "#,
            rusqlite::params![
                photo.filename,
                photo.directory,
                photo.size_bytes as i64,
                photo.modified_at,
                width,
                height,
                format,
                camera_make,
                camera_model,
                lens,
                focal_length,
                aperture,
                shutter_speed,
                iso,
                taken_at,
                gps_lat,
                gps_lon,
                all_exif,
                md5_hash,
                sha256_hash,
                perceptual_hash,
                orientation.unwrap_or(1) as i32,
                path_str.as_ref(),
            ],
        )?;

        Ok(())
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScanResult {
    pub total_found: usize,
    pub scanned: usize,
    pub new: usize,
    pub updated: usize,
}
