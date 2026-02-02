mod schema;
pub mod albums;
pub mod backend;
pub mod embeddings;
pub mod faces;
pub mod schedule;
pub mod similarity;
pub mod trash;

use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

pub use schema::{SCHEMA, MIGRATIONS};

/// Convert EXIF orientation value (1-8) to rotation degrees (0, 90, 180, 270)
fn exif_orientation_to_degrees(orientation: i32) -> i32 {
    match orientation {
        6 => 90,   // Rotate 90 CW
        3 => 180,  // Rotate 180
        8 => 270,  // Rotate 90 CCW
        _ => 0,    // Normal (1) or other values
    }
}

/// Read EXIF orientation directly from an image file and return rotation degrees
fn read_exif_rotation_from_file(path: &Path) -> i32 {
    use std::fs::File;
    use std::io::BufReader;

    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return 0,
    };

    let mut reader = BufReader::new(file);
    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        if let exif::Value::Short(ref v) = field.value {
            if let Some(&orientation) = v.first() {
                return exif_orientation_to_degrees(orientation as i32);
            }
        }
    }

    0
}
pub use similarity::{PhotoRecord, SimilarityGroup, calculate_quality_score};
pub use embeddings::SearchResult;
pub use faces::{BoundingBox, FaceWithPhoto, Person};
pub use schedule::{ScheduledTask, ScheduledTaskType, ScheduleStatus};
pub use albums::UserTag;

/// Full metadata for a photo from the database
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PhotoMetadata {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub directory: String,
    pub size_bytes: i64,

    // Image dimensions
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub format: Option<String>,

    // EXIF data
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub focal_length: Option<f64>,
    pub aperture: Option<f64>,
    pub shutter_speed: Option<String>,
    pub iso: Option<i64>,
    pub taken_at: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,

    // Timestamps
    pub modified_at: Option<String>,
    pub scanned_at: Option<String>,

    // LLM-generated content
    pub description: Option<String>,
    pub tags: Option<String>,

    // Hashes
    pub sha256_hash: Option<String>,
    pub perceptual_hash: Option<String>,

    // Face and people counts (computed)
    pub face_count: i64,
    pub people_names: Vec<String>,
}

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn open(path: &PathBuf) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn initialize(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        self.run_migrations()?;
        Ok(())
    }

    /// Run database migrations for existing databases.
    /// These add columns that may not exist in older versions.
    fn run_migrations(&self) -> Result<()> {
        for migration in MIGRATIONS {
            // Try to run each migration - they may fail if column already exists
            // which is expected behavior for idempotent migrations
            let _ = self.conn.execute(migration, []);
        }
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Save LLM description for a photo by path.
    /// Creates a minimal photo record if one doesn't exist yet.
    pub fn save_description(&self, path: &std::path::Path, description: &str) -> Result<()> {
        // Ensure photo record exists before updating
        self.ensure_photo_exists(path)?;

        let path_str = path.to_string_lossy();
        self.conn.execute(
            r#"
            UPDATE photos
            SET description = ?, llm_processed_at = CURRENT_TIMESTAMP
            WHERE path = ?
            "#,
            rusqlite::params![description, path_str],
        )?;
        Ok(())
    }

    /// Get LLM description for a photo by path
    pub fn get_description(&self, path: &std::path::Path) -> Result<Option<String>> {
        let path_str = path.to_string_lossy();
        let result = self.conn.query_row(
            "SELECT description FROM photos WHERE path = ?",
            [path_str],
            |row| row.get::<_, Option<String>>(0),
        );

        match result {
            Ok(desc) => Ok(desc),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update photo path after moving a file
    pub fn update_photo_path(&self, old_path: &std::path::Path, new_path: &std::path::Path) -> Result<()> {
        let old_path_str = old_path.to_string_lossy();
        let new_path_str = new_path.to_string_lossy();

        self.conn.execute(
            "UPDATE photos SET path = ? WHERE path = ?",
            rusqlite::params![new_path_str, old_path_str],
        )?;

        Ok(())
    }

    /// Get photos with their modified_at timestamps for a specific directory.
    /// Used for change detection.
    pub fn get_photos_mtime_in_dir(&self, directory: &str) -> Result<Vec<(String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, modified_at FROM photos WHERE directory = ?",
        )?;

        let results = stmt
            .query_map([directory], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get full photo metadata by path.
    /// Returns all stored data including EXIF, dimensions, description, and face/people info.
    pub fn get_photo_metadata(&self, path: &std::path::Path) -> Result<Option<PhotoMetadata>> {
        let path_str = path.to_string_lossy();

        // First get the basic photo data
        let result = self.conn.query_row(
            r#"
            SELECT id, path, filename, directory, size_bytes,
                   width, height, format,
                   camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                   gps_latitude, gps_longitude,
                   modified_at, scanned_at,
                   description, tags,
                   sha256_hash, perceptual_hash
            FROM photos
            WHERE path = ?
            "#,
            [path_str.as_ref()],
            |row| {
                Ok(PhotoMetadata {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    directory: row.get(3)?,
                    size_bytes: row.get(4)?,
                    width: row.get(5)?,
                    height: row.get(6)?,
                    format: row.get(7)?,
                    camera_make: row.get(8)?,
                    camera_model: row.get(9)?,
                    lens: row.get(10)?,
                    focal_length: row.get(11)?,
                    aperture: row.get(12)?,
                    shutter_speed: row.get(13)?,
                    iso: row.get(14)?,
                    taken_at: row.get(15)?,
                    gps_latitude: row.get(16)?,
                    gps_longitude: row.get(17)?,
                    modified_at: row.get(18)?,
                    scanned_at: row.get(19)?,
                    description: row.get(20)?,
                    tags: row.get(21)?,
                    sha256_hash: row.get(22)?,
                    perceptual_hash: row.get(23)?,
                    face_count: 0,
                    people_names: Vec::new(),
                })
            },
        );

        match result {
            Ok(mut metadata) => {
                // Get face count and people names for this photo
                let face_info = self.conn.query_row(
                    r#"
                    SELECT COUNT(f.id)
                    FROM faces f
                    WHERE f.photo_id = ?
                    "#,
                    [metadata.id],
                    |row| row.get::<_, i64>(0),
                );
                if let Ok(count) = face_info {
                    metadata.face_count = count;
                }

                // Get unique people names in this photo
                let mut stmt = self.conn.prepare(
                    r#"
                    SELECT DISTINCT p.name
                    FROM faces f
                    JOIN people p ON f.person_id = p.id
                    WHERE f.photo_id = ?
                    ORDER BY p.name
                    "#,
                )?;
                let names: Vec<String> = stmt
                    .query_map([metadata.id], |row| row.get(0))?
                    .filter_map(|r| r.ok())
                    .collect();
                metadata.people_names = names;

                Ok(Some(metadata))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Simple text-based search on descriptions (fallback when no embeddings)
    pub fn semantic_search_by_text(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, description
            FROM photos
            WHERE description IS NOT NULL
            "#,
        )?;

        let mut results: Vec<SearchResult> = stmt
            .query_map([], |row| {
                let description: String = row.get(3)?;
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    description,
                ))
            })?
            .filter_map(|r| r.ok())
            .filter_map(|(id, path, filename, description)| {
                let desc_lower = description.to_lowercase();

                // Calculate simple relevance score based on word matches
                let mut score = 0.0f32;
                for word in &query_words {
                    if desc_lower.contains(word) {
                        score += 1.0;
                    }
                }

                if score > 0.0 {
                    // Normalize score
                    let similarity = score / query_words.len() as f32;
                    Some(SearchResult {
                        photo_id: id,
                        path,
                        filename,
                        similarity,
                        description: Some(description),
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        results.truncate(limit);

        Ok(results)
    }

    /// Get the effective rotation for a photo (combines EXIF orientation and user rotation).
    /// Returns rotation in degrees (0, 90, 180, 270).
    /// If the photo is not in the database, reads EXIF orientation directly from file.
    pub fn get_photo_rotation(&self, path: &std::path::Path) -> Result<i32> {
        let path_str = path.to_string_lossy();
        let result = self.conn.query_row(
            "SELECT exif_orientation, user_rotation FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| {
                let exif_orientation: i32 = row.get::<_, Option<i32>>(0)?.unwrap_or(1);
                let user_rotation: i32 = row.get::<_, Option<i32>>(1)?.unwrap_or(0);
                Ok((exif_orientation, user_rotation))
            },
        );

        match result {
            Ok((exif_orientation, user_rotation)) => {
                // Convert EXIF orientation to degrees
                let exif_degrees = exif_orientation_to_degrees(exif_orientation);
                // Combine with user rotation
                Ok((exif_degrees + user_rotation) % 360)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Photo not in database - read EXIF directly from file
                Ok(read_exif_rotation_from_file(path))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Set user rotation for a photo.
    /// rotation should be 0, 90, 180, or 270 degrees.
    #[allow(dead_code)]
    pub fn set_user_rotation(&self, path: &std::path::Path, rotation: i32) -> Result<()> {
        let path_str = path.to_string_lossy();
        // Normalize rotation to 0, 90, 180, 270
        let normalized = ((rotation % 360) + 360) % 360;
        self.conn.execute(
            "UPDATE photos SET user_rotation = ? WHERE path = ?",
            rusqlite::params![normalized, path_str],
        )?;
        Ok(())
    }

    /// Ensure a minimal photo record exists in the database.
    /// Creates a stub record if the photo doesn't exist yet.
    fn ensure_photo_exists(&self, path: &std::path::Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        let exists: bool = self.conn.query_row(
            "SELECT 1 FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |_| Ok(true),
        ).unwrap_or(false);

        if !exists {
            let filename = path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let directory = path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let size_bytes = std::fs::metadata(path)
                .map(|m| m.len() as i64)
                .unwrap_or(0);

            self.conn.execute(
                "INSERT INTO photos (path, filename, directory, size_bytes) VALUES (?, ?, ?, ?)",
                rusqlite::params![path_str.as_ref(), filename, directory, size_bytes],
            )?;
        }
        Ok(())
    }

    /// Rotate a photo clockwise by 90 degrees (adds to user rotation).
    pub fn rotate_photo_cw(&self, path: &std::path::Path) -> Result<i32> {
        // Ensure photo record exists before updating
        self.ensure_photo_exists(path)?;

        let path_str = path.to_string_lossy();
        let current: i32 = self.conn.query_row(
            "SELECT COALESCE(user_rotation, 0) FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| row.get(0),
        ).unwrap_or(0);

        let new_rotation = (current + 90) % 360;
        self.conn.execute(
            "UPDATE photos SET user_rotation = ? WHERE path = ?",
            rusqlite::params![new_rotation, path_str],
        )?;

        // Return effective rotation
        self.get_photo_rotation(path)
    }

    /// Rotate a photo counter-clockwise by 90 degrees (subtracts from user rotation).
    pub fn rotate_photo_ccw(&self, path: &std::path::Path) -> Result<i32> {
        // Ensure photo record exists before updating
        self.ensure_photo_exists(path)?;

        let path_str = path.to_string_lossy();
        let current: i32 = self.conn.query_row(
            "SELECT COALESCE(user_rotation, 0) FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| row.get(0),
        ).unwrap_or(0);

        let new_rotation = (current + 270) % 360; // +270 is same as -90
        self.conn.execute(
            "UPDATE photos SET user_rotation = ? WHERE path = ?",
            rusqlite::params![new_rotation, path_str],
        )?;

        // Return effective rotation
        self.get_photo_rotation(path)
    }

    /// Reset user rotation to 0 (rely on EXIF only).
    #[allow(dead_code)]
    pub fn reset_photo_rotation(&self, path: &std::path::Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        self.conn.execute(
            "UPDATE photos SET user_rotation = 0 WHERE path = ?",
            [path_str],
        )?;
        Ok(())
    }
}
