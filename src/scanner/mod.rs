pub mod discovery;
pub mod hashing;
pub mod metadata;

use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::config::Config;
use crate::db::Database;
use crate::tasks::{TaskUpdate, TaskProgress};

pub use discovery::discover_images;
pub use hashing::HashResult;
pub use metadata::ImageMetadata;

#[derive(Debug, Clone)]
pub struct ScannedPhoto {
    pub path: PathBuf,
    pub filename: String,
    pub directory: String,
    pub size_bytes: u64,
    pub metadata: Option<ImageMetadata>,
    pub hashes: Option<HashResult>,
}

pub struct Scanner {
    config: Config,
}

impl Scanner {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Scan directory with cancellation support via TaskUpdate protocol.
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

        let mut scanned = 0;
        let mut new_count = 0;
        let mut updated_count = 0;

        for (index, path) in image_paths.iter().enumerate() {
            // Check for cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            let path_str = path.to_string_lossy().to_string();
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path_str.clone());

            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(index + 1, total).with_item(&filename)
            ));

            match self.scan_single_file(path) {
                Ok(photo) => {
                    // Check if photo already exists in database
                    match self.photo_exists(db, path) {
                        Ok(exists) => {
                            if exists {
                                if let Err(e) = self.update_photo(db, &photo) {
                                    // Log error but continue
                                    eprintln!("Error updating {}: {}", path.display(), e);
                                } else {
                                    updated_count += 1;
                                }
                            } else {
                                if let Err(e) = self.insert_photo(db, &photo) {
                                    eprintln!("Error inserting {}: {}", path.display(), e);
                                } else {
                                    new_count += 1;
                                }
                            }
                            scanned += 1;
                        }
                        Err(e) => {
                            eprintln!("Error checking existence of {}: {}", path.display(), e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error scanning {}: {}", path.display(), e);
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

        // Extract image metadata (EXIF, dimensions)
        let metadata = metadata::extract_metadata(path).ok();

        // Calculate hashes
        let hashes = hashing::calculate_hashes(path).ok();

        Ok(ScannedPhoto {
            path: path.clone(),
            filename,
            directory,
            size_bytes: file_metadata.len(),
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

        let (width, height, format, camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at, gps_lat, gps_lon, all_exif) =
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
                )
            } else {
                (None, None, None, None, None, None, None, None, None, None, None, None, None, None)
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
                path, filename, directory, size_bytes,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                gps_latitude, gps_longitude, all_exif,
                md5_hash, sha256_hash, perceptual_hash
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![
                path_str.as_ref(),
                photo.filename,
                photo.directory,
                photo.size_bytes as i64,
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
            ],
        )?;

        Ok(())
    }

    fn update_photo(&self, db: &Database, photo: &ScannedPhoto) -> Result<()> {
        let path_str = photo.path.to_string_lossy();

        let (width, height, format, camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at, gps_lat, gps_lon, all_exif) =
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
                )
            } else {
                (None, None, None, None, None, None, None, None, None, None, None, None, None, None)
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
                filename = ?, directory = ?, size_bytes = ?,
                width = ?, height = ?, format = ?,
                camera_make = ?, camera_model = ?, lens = ?, focal_length = ?, aperture = ?, shutter_speed = ?, iso = ?, taken_at = ?,
                gps_latitude = ?, gps_longitude = ?, all_exif = ?,
                md5_hash = ?, sha256_hash = ?, perceptual_hash = ?,
                scanned_at = CURRENT_TIMESTAMP
            WHERE path = ?
            "#,
            rusqlite::params![
                photo.filename,
                photo.directory,
                photo.size_bytes as i64,
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
