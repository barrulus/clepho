//! SQLite backend implementation.

use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use super::{PhotoMetadata, exif_orientation_to_degrees, read_exif_rotation_from_file};
use super::schema::{SCHEMA, MIGRATIONS};
use super::embeddings::{SearchResult, EmbeddingRecord, embedding_to_bytes, bytes_to_embedding, cosine_similarity};
use super::faces::{
    BoundingBox, Face, FaceCluster, FaceWithPhoto, Person,
    embedding_to_bytes as face_embedding_to_bytes, bytes_to_embedding as face_bytes_to_embedding,
};
use super::similarity::PhotoRecord;
use super::similarity::SimilarityGroup;
use super::trash::TrashedPhoto;
use super::schedule::{ScheduledTask, ScheduledTaskType, ScheduleStatus};
use super::albums::{UserTag, Album};
use super::similarity::hamming_distance;

pub struct SqliteDb {
    pub(crate) conn: Connection,
}

impl SqliteDb {
    pub fn open(path: &PathBuf) -> Result<Self> {
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

    fn run_migrations(&self) -> Result<()> {
        for migration in MIGRATIONS {
            let _ = self.conn.execute(migration, []);
        }
        Ok(())
    }

    // ========================================================================
    // Photo operations (from mod.rs)
    // ========================================================================

    pub fn save_description(&self, path: &Path, description: &str) -> Result<()> {
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

    pub fn get_description(&self, path: &Path) -> Result<Option<String>> {
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

    pub fn update_photo_path(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        let old_path_str = old_path.to_string_lossy();
        let new_path_str = new_path.to_string_lossy();
        self.conn.execute(
            "UPDATE photos SET path = ? WHERE path = ?",
            rusqlite::params![new_path_str, old_path_str],
        )?;
        Ok(())
    }

    pub fn get_photos_mtime_in_dir(&self, directory: &str) -> Result<Vec<(String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, modified_at FROM photos WHERE directory = ?",
        )?;
        let results = stmt
            .query_map([directory], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_photo_metadata(&self, path: &Path) -> Result<Option<PhotoMetadata>> {
        let path_str = path.to_string_lossy();
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
                let face_info = self.conn.query_row(
                    "SELECT COUNT(f.id) FROM faces f WHERE f.photo_id = ?",
                    [metadata.id],
                    |row| row.get::<_, i64>(0),
                );
                if let Ok(count) = face_info {
                    metadata.face_count = count;
                }
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
                let mut score = 0.0f32;
                for word in &query_words {
                    if desc_lower.contains(word) {
                        score += 1.0;
                    }
                }
                if score > 0.0 {
                    let similarity = score / query_words.len() as f32;
                    Some(SearchResult { photo_id: id, path, filename, similarity, description: Some(description) })
                } else {
                    None
                }
            })
            .collect();
        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        Ok(results)
    }

    pub fn get_photo_rotation(&self, path: &Path) -> Result<i32> {
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
                let exif_degrees = exif_orientation_to_degrees(exif_orientation);
                Ok((exif_degrees + user_rotation) % 360)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                Ok(read_exif_rotation_from_file(path))
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn set_user_rotation(&self, path: &Path, rotation: i32) -> Result<()> {
        let path_str = path.to_string_lossy();
        let normalized = ((rotation % 360) + 360) % 360;
        self.conn.execute(
            "UPDATE photos SET user_rotation = ? WHERE path = ?",
            rusqlite::params![normalized, path_str],
        )?;
        Ok(())
    }

    fn ensure_photo_exists(&self, path: &Path) -> Result<()> {
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

    pub fn rotate_photo_cw(&self, path: &Path) -> Result<i32> {
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
        self.get_photo_rotation(path)
    }

    pub fn rotate_photo_ccw(&self, path: &Path) -> Result<i32> {
        self.ensure_photo_exists(path)?;
        let path_str = path.to_string_lossy();
        let current: i32 = self.conn.query_row(
            "SELECT COALESCE(user_rotation, 0) FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| row.get(0),
        ).unwrap_or(0);
        let new_rotation = (current + 270) % 360;
        self.conn.execute(
            "UPDATE photos SET user_rotation = ? WHERE path = ?",
            rusqlite::params![new_rotation, path_str],
        )?;
        self.get_photo_rotation(path)
    }

    pub fn reset_photo_rotation(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        self.conn.execute(
            "UPDATE photos SET user_rotation = 0 WHERE path = ?",
            [path_str],
        )?;
        Ok(())
    }

    // ========================================================================
    // Face operations (from faces.rs)
    // ========================================================================

    pub fn create_person(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO people (name) VALUES (?)",
            rusqlite::params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn find_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        let result = self.conn.query_row(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE LOWER(p.name) = LOWER(?)
            GROUP BY p.id
            "#,
            [name],
            |row| Ok(Person { id: row.get(0)?, name: row.get(1)?, face_count: row.get(2)? }),
        );
        match result {
            Ok(person) => Ok(Some(person)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn find_or_create_person(&self, name: &str) -> Result<i64> {
        if let Some(person) = self.find_person_by_name(name)? {
            Ok(person.id)
        } else {
            self.create_person(name)
        }
    }

    pub fn update_person_name(&self, person_id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE people SET name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            rusqlite::params![name, person_id],
        )?;
        Ok(())
    }

    pub fn delete_person(&self, person_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM people WHERE id = ?", rusqlite::params![person_id])?;
        Ok(())
    }

    pub fn get_all_people(&self) -> Result<Vec<Person>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            GROUP BY p.id
            ORDER BY p.name
            "#,
        )?;
        let people = stmt
            .query_map([], |row| Ok(Person { id: row.get(0)?, name: row.get(1)?, face_count: row.get(2)? }))
            ?
            .filter_map(|r| r.ok())
            .collect();
        Ok(people)
    }

    pub fn get_person(&self, person_id: i64) -> Result<Option<Person>> {
        let result = self.conn.query_row(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE p.id = ?
            GROUP BY p.id
            "#,
            [person_id],
            |row| Ok(Person { id: row.get(0)?, name: row.get(1)?, face_count: row.get(2)? }),
        );
        match result {
            Ok(person) => Ok(Some(person)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn store_face(
        &self,
        photo_id: i64,
        bbox: &BoundingBox,
        embedding: Option<&[f32]>,
        confidence: Option<f32>,
    ) -> Result<i64> {
        let embedding_bytes = embedding.map(face_embedding_to_bytes);
        let embedding_dim = embedding.map(|e| e.len() as i32);
        self.conn.execute(
            r#"
            INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, embedding_dim, confidence)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![photo_id, bbox.x, bbox.y, bbox.width, bbox.height, embedding_bytes, embedding_dim, confidence],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<Face>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, person_id, confidence
            FROM faces
            WHERE photo_id = ?
            "#,
        )?;
        let faces = stmt
            .query_map([photo_id], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(Face {
                    id: row.get(0)?,
                    photo_id: row.get(1)?,
                    bbox: BoundingBox { x: row.get(2)?, y: row.get(3)?, width: row.get(4)?, height: row.get(5)? },
                    embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                    person_id: row.get(7)?,
                    confidence: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(faces)
    }

    pub fn get_faces_for_person(&self, person_id: i64) -> Result<Vec<FaceWithPhoto>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id = ?
            ORDER BY p.taken_at DESC
            "#,
        )?;
        let faces = stmt
            .query_map([person_id], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(FaceWithPhoto {
                    face: Face {
                        id: row.get(0)?,
                        photo_id: row.get(1)?,
                        bbox: BoundingBox { x: row.get(2)?, y: row.get(3)?, width: row.get(4)?, height: row.get(5)? },
                        embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                        person_id: row.get(7)?,
                        confidence: row.get(8)?,
                    },
                    photo_path: row.get(9)?,
                    photo_filename: row.get(10)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(faces)
    }

    pub fn assign_face_to_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE faces SET person_id = ? WHERE id = ?",
            rusqlite::params![person_id, face_id],
        )?;
        Ok(())
    }

    pub fn unassign_face(&self, face_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE faces SET person_id = NULL WHERE id = ?",
            rusqlite::params![face_id],
        )?;
        Ok(())
    }

    pub fn get_unassigned_faces(&self) -> Result<Vec<FaceWithPhoto>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id IS NULL
            ORDER BY p.taken_at DESC
            "#,
        )?;
        let faces = stmt
            .query_map([], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(FaceWithPhoto {
                    face: Face {
                        id: row.get(0)?,
                        photo_id: row.get(1)?,
                        bbox: BoundingBox { x: row.get(2)?, y: row.get(3)?, width: row.get(4)?, height: row.get(5)? },
                        embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                        person_id: row.get(7)?,
                        confidence: row.get(8)?,
                    },
                    photo_path: row.get(9)?,
                    photo_filename: row.get(10)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(faces)
    }

    pub fn get_photos_without_faces_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
              AND p.path LIKE ?
            LIMIT ?
            "#,
        )?;
        let results = stmt
            .query_map(rusqlite::params![dir_pattern, limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn mark_photo_scanned(&self, photo_id: i64, faces_found: usize) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO face_scans (photo_id, faces_found, scanned_at) VALUES (?, ?, CURRENT_TIMESTAMP)",
            rusqlite::params![photo_id, faces_found as i64],
        )?;
        Ok(())
    }

    pub fn count_photos_needing_face_scan(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
            "#,
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn count_faces(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM faces", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn count_people(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM people", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_all_face_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, embedding FROM faces WHERE embedding IS NOT NULL",
        )?;
        let results = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok((row.get(0)?, face_bytes_to_embedding(&bytes)))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_faces_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, i64, BoundingBox)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h
            FROM faces
            WHERE embedding IS NULL
            LIMIT ?
            "#,
        )?;
        let results = stmt
            .query_map([limit as i64], |row| {
                Ok((row.get(0)?, row.get(1)?, BoundingBox { x: row.get(2)?, y: row.get(3)?, width: row.get(4)?, height: row.get(5)? }))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_photo_path(&self, photo_id: i64) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT path FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        );
        match result {
            Ok(path) => Ok(Some(path)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update_face_embedding(&self, face_id: i64, embedding: &[f32]) -> Result<()> {
        let embedding_bytes = face_embedding_to_bytes(embedding);
        let embedding_dim = embedding.len() as i32;
        self.conn.execute(
            "UPDATE faces SET embedding = ?, embedding_dim = ? WHERE id = ?",
            rusqlite::params![embedding_bytes, embedding_dim, face_id],
        )?;
        Ok(())
    }

    pub fn count_faces_without_embeddings(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM faces WHERE embedding IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn create_face_cluster(&self, representative_face_id: Option<i64>, auto_name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO face_clusters (representative_face_id, auto_name) VALUES (?, ?)",
            rusqlite::params![representative_face_id, auto_name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_face_to_cluster(&self, face_id: i64, cluster_id: i64, similarity_score: f32) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO face_cluster_members (face_id, cluster_id, similarity_score)
            VALUES (?, ?, ?)
            "#,
            rusqlite::params![face_id, cluster_id, similarity_score],
        )?;
        Ok(())
    }

    pub fn get_all_face_clusters(&self) -> Result<Vec<FaceCluster>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT fc.id, fc.auto_name, fc.representative_face_id, COUNT(fcm.face_id) as face_count
            FROM face_clusters fc
            LEFT JOIN face_cluster_members fcm ON fc.id = fcm.cluster_id
            GROUP BY fc.id
            ORDER BY face_count DESC
            "#,
        )?;
        let clusters = stmt
            .query_map([], |row| {
                Ok(FaceCluster {
                    id: row.get(0)?,
                    auto_name: row.get(1)?,
                    representative_face_id: row.get(2)?,
                    face_count: row.get(3)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(clusters)
    }

    pub fn clear_face_clusters(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM face_cluster_members;
            DELETE FROM face_clusters;
            "#,
        )?;
        Ok(())
    }

    pub fn cluster_to_person(&self, cluster_id: i64, person_name: &str) -> Result<i64> {
        let person_id = self.create_person(person_name)?;
        self.conn.execute(
            r#"
            UPDATE faces SET person_id = ?
            WHERE id IN (SELECT face_id FROM face_cluster_members WHERE cluster_id = ?)
            "#,
            rusqlite::params![person_id, cluster_id],
        )?;
        self.conn.execute(
            "DELETE FROM face_cluster_members WHERE cluster_id = ?",
            rusqlite::params![cluster_id],
        )?;
        self.conn.execute(
            "DELETE FROM face_clusters WHERE id = ?",
            rusqlite::params![cluster_id],
        )?;
        Ok(person_id)
    }

    pub fn search_photos_by_person(&self, person_id: i64) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT p.id, p.path, p.filename
            FROM photos p
            JOIN faces f ON p.id = f.photo_id
            WHERE f.person_id = ?
            ORDER BY p.taken_at DESC
            "#,
        )?;
        let results = stmt
            .query_map([person_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    // ========================================================================
    // Embedding operations (from embeddings.rs)
    // ========================================================================

    pub fn store_embedding(&self, photo_id: i64, embedding: &[f32], model_name: &str) -> Result<()> {
        let bytes = embedding_to_bytes(embedding);
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO embeddings (photo_id, embedding, embedding_dim, model_name, created_at)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            rusqlite::params![photo_id, bytes, embedding.len() as i32, model_name],
        )?;
        Ok(())
    }

    pub fn get_embedding(&self, photo_id: i64) -> Result<Option<EmbeddingRecord>> {
        let result = self.conn.query_row(
            "SELECT photo_id, embedding, model_name FROM embeddings WHERE photo_id = ?",
            [photo_id],
            |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok(EmbeddingRecord {
                    photo_id: row.get(0)?,
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_all_embeddings(&self) -> Result<Vec<EmbeddingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, embedding, model_name FROM embeddings",
        )?;
        let records = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok(EmbeddingRecord {
                    photo_id: row.get(0)?,
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(records)
    }

    pub fn semantic_search(&self, query_embedding: &[f32], limit: usize, min_similarity: f32) -> Result<Vec<SearchResult>> {
        let embeddings = self.get_all_embeddings()?;
        let mut results: Vec<(i64, f32)> = embeddings
            .iter()
            .map(|record| {
                let similarity = cosine_similarity(query_embedding, &record.embedding);
                (record.photo_id, similarity)
            })
            .filter(|(_, sim)| *sim >= min_similarity)
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_results: Vec<(i64, f32)> = results.into_iter().take(limit).collect();
        let mut search_results = Vec::new();
        for (photo_id, similarity) in top_results {
            if let Ok(Some(result)) = self.get_photo_for_search(photo_id, similarity) {
                search_results.push(result);
            }
        }
        Ok(search_results)
    }

    fn get_photo_for_search(&self, photo_id: i64, similarity: f32) -> Result<Option<SearchResult>> {
        let result = self.conn.query_row(
            "SELECT id, path, filename, description FROM photos WHERE id = ?",
            [photo_id],
            |row| {
                Ok(SearchResult {
                    photo_id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    similarity,
                    description: row.get(3)?,
                })
            },
        );
        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_photos_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
            LIMIT ?
            "#,
        )?;
        let results = stmt
            .query_map([limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn get_photos_without_embeddings_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
              AND p.path LIKE ?
            LIMIT ?
            "#,
        )?;
        let results = stmt
            .query_map(rusqlite::params![dir_pattern, limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn count_embeddings(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))?;
        Ok(count)
    }

    // ========================================================================
    // Similarity operations (from similarity.rs)
    // ========================================================================

    pub fn find_exact_duplicates(&self) -> Result<Vec<SimilarityGroup>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sha256_hash, COUNT(*) as cnt
            FROM photos
            WHERE sha256_hash IS NOT NULL
            GROUP BY sha256_hash
            HAVING cnt > 1
            "#,
        )?;
        let duplicate_hashes: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();
        let mut groups = Vec::new();
        for hash in duplicate_hashes {
            let photos = self.get_photos_by_sha256(&hash)?;
            if photos.len() > 1 {
                groups.push(SimilarityGroup {
                    id: 0,
                    group_type: "exact".to_string(),
                    photos,
                });
            }
        }
        Ok(groups)
    }

    pub fn find_perceptual_duplicates(&self, threshold: u32) -> Result<Vec<SimilarityGroup>> {
        let photos = self.get_all_photos_with_phash()?;
        let mut groups: Vec<SimilarityGroup> = Vec::new();
        let mut processed: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for (i, photo) in photos.iter().enumerate() {
            if processed.contains(&photo.id) {
                continue;
            }
            let hash1 = match &photo.perceptual_hash {
                Some(h) => h,
                None => continue,
            };
            let mut similar_photos = vec![photo.clone()];
            for other in photos.iter().skip(i + 1) {
                if processed.contains(&other.id) {
                    continue;
                }
                let hash2 = match &other.perceptual_hash {
                    Some(h) => h,
                    None => continue,
                };
                if let Ok(distance) = hamming_distance(hash1, hash2) {
                    if distance <= threshold {
                        similar_photos.push(other.clone());
                        processed.insert(other.id);
                    }
                }
            }
            if similar_photos.len() > 1 {
                processed.insert(photo.id);
                groups.push(SimilarityGroup {
                    id: 0,
                    group_type: "perceptual".to_string(),
                    photos: similar_photos,
                });
            }
        }
        Ok(groups)
    }

    fn get_photos_by_sha256(&self, sha256: &str) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE sha256_hash = ?
            ORDER BY taken_at, path
            "#,
        )?;
        let photos = stmt
            .query_map([sha256], |row| {
                Ok(PhotoRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    size_bytes: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    sha256_hash: row.get(6)?,
                    perceptual_hash: row.get(7)?,
                    taken_at: row.get(8)?,
                    marked_for_deletion: row.get::<_, i32>(9)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    fn get_all_photos_with_phash(&self) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE perceptual_hash IS NOT NULL
            ORDER BY path
            "#,
        )?;
        let photos = stmt
            .query_map([], |row| {
                Ok(PhotoRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    size_bytes: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    sha256_hash: row.get(6)?,
                    perceptual_hash: row.get(7)?,
                    taken_at: row.get(8)?,
                    marked_for_deletion: row.get::<_, i32>(9)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    pub fn mark_for_deletion(&self, photo_id: i64) -> Result<()> {
        self.conn.execute("UPDATE photos SET marked_for_deletion = 1 WHERE id = ?", rusqlite::params![photo_id])?;
        Ok(())
    }

    pub fn unmark_for_deletion(&self, photo_id: i64) -> Result<()> {
        self.conn.execute("UPDATE photos SET marked_for_deletion = 0 WHERE id = ?", rusqlite::params![photo_id])?;
        Ok(())
    }

    pub fn get_marked_for_deletion(&self) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE marked_for_deletion = 1
            ORDER BY path
            "#,
        )?;
        let photos = stmt
            .query_map([], |row| {
                Ok(PhotoRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    size_bytes: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    sha256_hash: row.get(6)?,
                    perceptual_hash: row.get(7)?,
                    taken_at: row.get(8)?,
                    marked_for_deletion: row.get::<_, i32>(9)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    pub fn delete_marked_photos(&self) -> Result<usize> {
        let count = self.conn.execute("DELETE FROM photos WHERE marked_for_deletion = 1", [])?;
        Ok(count)
    }

    pub fn delete_photos_by_ids(&self, ids: &[i64]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
        let sql = format!("DELETE FROM photos WHERE id IN ({})", placeholders.join(", "));
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
        let count = self.conn.execute(&sql, params.as_slice())?;
        Ok(count)
    }

    pub fn get_photo_count(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))?;
        Ok(count)
    }

    // ========================================================================
    // Trash operations (from trash.rs)
    // ========================================================================

    pub fn mark_trashed(&self, photo_id: i64, trash_path: &Path) -> Result<()> {
        let original_path: String = self.conn.query_row(
            "SELECT path FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        )?;
        let trash_path_str = trash_path.to_string_lossy();
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            r#"
            UPDATE photos
            SET path = ?,
                original_path = ?,
                trashed_at = ?,
                marked_for_deletion = 0
            WHERE id = ?
            "#,
            rusqlite::params![trash_path_str, original_path, now, photo_id],
        )?;
        Ok(())
    }

    pub fn get_trashed_photos(&self) -> Result<Vec<TrashedPhoto>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, original_path, filename, trashed_at, size_bytes
            FROM photos
            WHERE trashed_at IS NOT NULL
            ORDER BY trashed_at DESC
            "#,
        )?;
        let photos = stmt
            .query_map([], |row| {
                Ok(TrashedPhoto {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    original_path: row.get(2)?,
                    filename: row.get(3)?,
                    trashed_at: row.get(4)?,
                    size_bytes: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    pub fn restore_photo(&self, photo_id: i64) -> Result<String> {
        let original_path: String = self.conn.query_row(
            "SELECT original_path FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        )?;
        self.conn.execute(
            r#"
            UPDATE photos
            SET path = original_path,
                original_path = NULL,
                trashed_at = NULL
            WHERE id = ?
            "#,
            [photo_id],
        )?;
        Ok(original_path)
    }

    pub fn delete_trashed_photo(&self, photo_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM photos WHERE id = ?", [photo_id])?;
        Ok(())
    }

    pub fn get_marked_not_trashed(&self) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE marked_for_deletion = 1 AND trashed_at IS NULL
            ORDER BY path
            "#,
        )?;
        let photos = stmt
            .query_map([], |row| {
                Ok(PhotoRecord {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    size_bytes: row.get(3)?,
                    width: row.get(4)?,
                    height: row.get(5)?,
                    sha256_hash: row.get(6)?,
                    perceptual_hash: row.get(7)?,
                    taken_at: row.get(8)?,
                    marked_for_deletion: row.get::<_, i32>(9)? != 0,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    pub fn get_old_trashed_photos(&self, max_age_days: u32) -> Result<Vec<TrashedPhoto>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, original_path, filename, trashed_at, size_bytes
            FROM photos
            WHERE trashed_at IS NOT NULL AND trashed_at < ?
            ORDER BY trashed_at
            "#,
        )?;
        let photos = stmt
            .query_map([cutoff_str], |row| {
                Ok(TrashedPhoto {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    original_path: row.get(2)?,
                    filename: row.get(3)?,
                    trashed_at: row.get(4)?,
                    size_bytes: row.get(5)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    pub fn get_trash_total_size(&self) -> Result<u64> {
        let size: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM photos WHERE trashed_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(size as u64)
    }

    // ========================================================================
    // Schedule operations (from schedule.rs)
    // ========================================================================

    pub fn create_scheduled_task(
        &self,
        task_type: ScheduledTaskType,
        target_path: &str,
        photo_ids: Option<&[i64]>,
        scheduled_at: &str,
        hours_start: Option<u8>,
        hours_end: Option<u8>,
    ) -> Result<i64> {
        let photo_ids_json = photo_ids.map(|ids| {
            serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string())
        });
        self.conn.execute(
            r#"
            INSERT INTO scheduled_tasks (
                task_type, target_path, photo_ids, scheduled_at, hours_start, hours_end
            ) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            rusqlite::params![
                task_type.as_str(),
                target_path,
                photo_ids_json,
                scheduled_at,
                hours_start,
                hours_end,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_pending_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending'
            ORDER BY scheduled_at ASC
            "#,
        )?;
        let tasks = stmt
            .query_map([], row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn get_overdue_schedules(&self, now: &str) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending' AND scheduled_at < ?
            ORDER BY scheduled_at ASC
            "#,
        )?;
        let tasks = stmt
            .query_map([now], row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn get_all_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            ORDER BY scheduled_at DESC
            LIMIT 100
            "#,
        )?;
        let tasks = stmt
            .query_map([], row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn update_schedule_status(
        &self,
        id: i64,
        status: ScheduleStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        match status {
            ScheduleStatus::Running => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ?, started_at = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), now, id],
                )?;
            }
            ScheduleStatus::Completed | ScheduleStatus::Failed | ScheduleStatus::Cancelled => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ?, completed_at = ?, error_message = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), now, error_message, id],
                )?;
            }
            ScheduleStatus::Pending => {
                self.conn.execute(
                    "UPDATE scheduled_tasks SET status = ? WHERE id = ?",
                    rusqlite::params![status.as_str(), id],
                )?;
            }
        }
        Ok(())
    }

    pub fn cancel_schedule(&self, id: i64) -> Result<()> {
        self.update_schedule_status(id, ScheduleStatus::Cancelled, None)
    }

    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM scheduled_tasks WHERE id = ?", [id])?;
        Ok(())
    }

    // ========================================================================
    // Daemon-specific schedule operations
    // ========================================================================

    pub fn get_due_pending_tasks(&self, limit: usize) -> Result<Vec<ScheduledTask>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending'
              AND (scheduled_at IS NULL OR datetime(scheduled_at) <= datetime('now'))
            ORDER BY scheduled_at ASC
            LIMIT ?
            "#,
        )?;
        let tasks = stmt
            .query_map([limit as i64], row_to_scheduled_task)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn mark_task_running(&self, task_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scheduled_tasks SET status = 'running', started_at = CURRENT_TIMESTAMP WHERE id = ?",
            [task_id],
        )?;
        Ok(())
    }

    pub fn mark_task_completed(&self, task_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE scheduled_tasks SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = ?",
            [task_id],
        )?;
        Ok(())
    }

    pub fn mark_task_failed(&self, task_id: i64, error: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE scheduled_tasks SET status = 'failed', error_message = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?",
            rusqlite::params![error, task_id],
        )?;
        Ok(())
    }

    // ========================================================================
    // Album operations (from albums.rs)
    // ========================================================================

    pub fn get_all_tags(&self) -> Result<Vec<UserTag>> {
        let mut stmt = self.conn.prepare("SELECT id, name, color FROM user_tags ORDER BY name")?;
        let tags = stmt
            .query_map([], |row| Ok(UserTag { id: row.get(0)?, name: row.get(1)?, color: row.get(2)? }))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<i64> {
        let color = color.unwrap_or("#808080");
        self.conn.execute(
            "INSERT INTO user_tags (name, color) VALUES (?, ?)",
            rusqlite::params![name, color],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_or_create_tag(&self, name: &str) -> Result<UserTag> {
        let existing = self.conn.query_row(
            "SELECT id, name, color FROM user_tags WHERE name = ? COLLATE NOCASE",
            [name],
            |row| Ok(UserTag { id: row.get(0)?, name: row.get(1)?, color: row.get(2)? }),
        );
        match existing {
            Ok(tag) => Ok(tag),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let id = self.create_tag(name, None)?;
                Ok(UserTag { id, name: name.to_string(), color: "#808080".to_string() })
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM user_tags WHERE id = ?", [tag_id])?;
        Ok(())
    }

    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE user_tags SET name = ? WHERE id = ?",
            rusqlite::params![new_name, tag_id],
        )?;
        Ok(())
    }

    pub fn get_photo_tags(&self, photo_id: i64) -> Result<Vec<UserTag>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT t.id, t.name, t.color
            FROM user_tags t
            JOIN photo_user_tags pt ON pt.tag_id = t.id
            WHERE pt.photo_id = ?
            ORDER BY t.name
            "#,
        )?;
        let tags = stmt
            .query_map([photo_id], |row| Ok(UserTag { id: row.get(0)?, name: row.get(1)?, color: row.get(2)? }))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    pub fn add_tag_to_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO photo_user_tags (photo_id, tag_id) VALUES (?, ?)",
            rusqlite::params![photo_id, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag_from_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM photo_user_tags WHERE photo_id = ? AND tag_id = ?",
            rusqlite::params![photo_id, tag_id],
        )?;
        Ok(())
    }

    pub fn get_photos_with_tag(&self, tag_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT photo_id FROM photo_user_tags WHERE tag_id = ?")?;
        let ids = stmt
            .query_map([tag_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    pub fn search_tags(&self, prefix: &str) -> Result<Vec<UserTag>> {
        let pattern = format!("{}%", prefix);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color FROM user_tags WHERE name LIKE ? COLLATE NOCASE ORDER BY name LIMIT 10",
        )?;
        let tags = stmt
            .query_map([pattern], |row| Ok(UserTag { id: row.get(0)?, name: row.get(1)?, color: row.get(2)? }))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tags)
    }

    pub fn get_all_albums(&self) -> Result<Vec<Album>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name, a.description, a.cover_photo_id, a.is_smart, a.filter_tags,
                   (SELECT COUNT(*) FROM album_photos WHERE album_id = a.id) as photo_count
            FROM albums a
            ORDER BY a.name
            "#,
        )?;
        let albums = stmt
            .query_map([], |row| {
                let filter_tags_json: Option<String> = row.get(5)?;
                let filter_tags: Vec<i64> = filter_tags_json
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default();
                Ok(Album {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    cover_photo_id: row.get(3)?,
                    is_smart: row.get::<_, i64>(4)? == 1,
                    filter_tags,
                    photo_count: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(albums)
    }

    pub fn create_album(&self, name: &str, description: Option<&str>, is_smart: bool) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO albums (name, description, is_smart) VALUES (?, ?, ?)",
            rusqlite::params![name, description, if is_smart { 1 } else { 0 }],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn delete_album(&self, album_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM albums WHERE id = ?", [album_id])?;
        Ok(())
    }

    pub fn add_photo_to_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO album_photos (album_id, photo_id) VALUES (?, ?)",
            rusqlite::params![album_id, photo_id],
        )?;
        Ok(())
    }

    pub fn remove_photo_from_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM album_photos WHERE album_id = ? AND photo_id = ?",
            rusqlite::params![album_id, photo_id],
        )?;
        Ok(())
    }

    pub fn get_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id FROM album_photos WHERE album_id = ? ORDER BY position, added_at",
        )?;
        let ids = stmt
            .query_map([album_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    pub fn get_album_photo_paths(&self, album_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.path
            FROM photos p
            JOIN album_photos ap ON ap.photo_id = p.id
            WHERE ap.album_id = ?
            ORDER BY ap.position, ap.added_at
            "#,
        )?;
        let paths = stmt
            .query_map([album_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(paths)
    }

    pub fn set_album_filter_tags(&self, album_id: i64, tag_ids: &[i64]) -> Result<()> {
        let json = serde_json::to_string(tag_ids)?;
        self.conn.execute(
            "UPDATE albums SET filter_tags = ?, is_smart = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            rusqlite::params![json, album_id],
        )?;
        Ok(())
    }

    pub fn get_smart_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        let filter_json: Option<String> = self.conn.query_row(
            "SELECT filter_tags FROM albums WHERE id = ?",
            [album_id],
            |row| row.get(0),
        )?;
        let tag_ids: Vec<i64> = filter_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        if tag_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> = tag_ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            r#"
            SELECT photo_id
            FROM photo_user_tags
            WHERE tag_id IN ({})
            GROUP BY photo_id
            HAVING COUNT(DISTINCT tag_id) = ?
            "#,
            placeholders.join(",")
        );
        let mut stmt = self.conn.prepare(&query)?;
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = tag_ids
            .iter()
            .map(|id| Box::new(*id) as Box<dyn rusqlite::ToSql>)
            .collect();
        params_vec.push(Box::new(tag_ids.len() as i64));
        let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let ids: Vec<i64> = stmt
            .query_map(params_refs.as_slice(), |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }

    // ========================================================================
    // LLM queue operations (from llm/queue.rs)
    // ========================================================================

    pub fn save_llm_result(&self, photo_id: i64, description: &str, tags_json: &str) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE photos
            SET description = ?, tags = ?, llm_processed_at = CURRENT_TIMESTAMP
            WHERE id = ?
            "#,
            rusqlite::params![description, tags_json, photo_id],
        )?;
        Ok(())
    }

    pub fn get_photos_without_description(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL
            ORDER BY scanned_at DESC
            "#,
        )?;
        let tasks = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn get_photos_without_description_in_dir(&self, directory: &Path) -> Result<Vec<(i64, String)>> {
        let dir_str = directory.to_string_lossy();
        let pattern = format!("{}%", dir_str);
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL AND path LIKE ?
            ORDER BY path ASC
            "#,
        )?;
        let tasks = stmt
            .query_map([pattern], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(tasks)
    }

    pub fn get_photo_description(&self, photo_id: i64) -> Result<Option<String>> {
        let result: Option<String> = self.conn.query_row(
            "SELECT description FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        ).ok();
        Ok(result)
    }

    // ========================================================================
    // Scanner operations (from scanner/mod.rs)
    // ========================================================================

    pub fn photo_exists(&self, path: &Path) -> Result<bool> {
        let path_str = path.to_string_lossy();
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE path = ?",
            [path_str.as_ref()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn insert_scanned_photo(
        &self,
        path: &str,
        filename: &str,
        directory: &str,
        size_bytes: i64,
        modified_at: Option<&str>,
        width: Option<u32>,
        height: Option<u32>,
        format: Option<&str>,
        camera_make: Option<&str>,
        camera_model: Option<&str>,
        lens: Option<&str>,
        focal_length: Option<f64>,
        aperture: Option<f64>,
        shutter_speed: Option<&str>,
        iso: Option<i64>,
        taken_at: Option<&str>,
        gps_lat: Option<f64>,
        gps_lon: Option<f64>,
        all_exif: Option<&str>,
        md5_hash: Option<&str>,
        sha256_hash: Option<&str>,
        perceptual_hash: Option<&str>,
        exif_orientation: i32,
    ) -> Result<()> {
        self.conn.execute(
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
                path, filename, directory, size_bytes, modified_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                gps_lat, gps_lon, all_exif,
                md5_hash, sha256_hash, perceptual_hash,
                exif_orientation,
            ],
        )?;
        Ok(())
    }

    pub fn update_scanned_photo(
        &self,
        path: &str,
        filename: &str,
        directory: &str,
        size_bytes: i64,
        modified_at: Option<&str>,
        width: Option<u32>,
        height: Option<u32>,
        format: Option<&str>,
        camera_make: Option<&str>,
        camera_model: Option<&str>,
        lens: Option<&str>,
        focal_length: Option<f64>,
        aperture: Option<f64>,
        shutter_speed: Option<&str>,
        iso: Option<i64>,
        taken_at: Option<&str>,
        gps_lat: Option<f64>,
        gps_lon: Option<f64>,
        all_exif: Option<&str>,
        md5_hash: Option<&str>,
        sha256_hash: Option<&str>,
        perceptual_hash: Option<&str>,
        exif_orientation: i32,
    ) -> Result<()> {
        self.conn.execute(
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
                filename, directory, size_bytes, modified_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                gps_lat, gps_lon, all_exif,
                md5_hash, sha256_hash, perceptual_hash,
                exif_orientation,
                path,
            ],
        )?;
        Ok(())
    }

    // ========================================================================
    // Export operations (from export/mod.rs)
    // ========================================================================

    pub fn get_photos_for_export(&self) -> Result<Vec<super::ExportedPhotoRow>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                path,
                width,
                height,
                size_bytes,
                sha256_hash,
                perceptual_hash,
                camera_make,
                camera_model,
                taken_at,
                description,
                scanned_at
            FROM photos
            ORDER BY path
            "#,
        )?;
        let photos = stmt
            .query_map([], |row| {
                let path: String = row.get(0)?;
                let filename = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                Ok(super::ExportedPhotoRow {
                    path,
                    filename,
                    width: row.get(1)?,
                    height: row.get(2)?,
                    file_size: row.get(3)?,
                    sha256: row.get(4)?,
                    perceptual_hash: row.get(5)?,
                    camera_make: row.get(6)?,
                    camera_model: row.get(7)?,
                    date_taken: row.get(8)?,
                    description: row.get(9)?,
                    scanned_at: row.get(10)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(photos)
    }

    // ========================================================================
    // Daemon scan operations
    // ========================================================================

    pub fn photo_exists_by_path(&self, path: &str) -> bool {
        self.conn.query_row(
            "SELECT 1 FROM photos WHERE path = ?",
            [path],
            |_| Ok(true),
        ).unwrap_or(false)
    }

    pub fn insert_basic_photo(&self, path: &str, filename: &str, directory: &str, size: i64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR IGNORE INTO photos (path, filename, directory, size_bytes, scanned_at)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            rusqlite::params![path, filename, directory, size],
        )?;
        Ok(())
    }

    pub fn get_photos_without_description_in_directory(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path
            FROM photos
            WHERE directory = ? AND description IS NULL
            LIMIT ?
            "#,
        )?;
        let results = stmt
            .query_map(rusqlite::params![directory, limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(results)
    }

    pub fn save_photo_description_by_id(&self, photo_id: i64, description: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET description = ?, llm_processed_at = CURRENT_TIMESTAMP WHERE id = ?",
            rusqlite::params![description, photo_id],
        )?;
        Ok(())
    }

    pub fn count_photos_without_faces_in_dir(&self, directory: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM photos p
            WHERE p.directory = ?
              AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.photo_id = p.id)
            "#,
            [directory],
            |row| row.get(0),
        ).unwrap_or(0);
        Ok(count)
    }
}

/// Helper to convert a row to ScheduledTask.
fn row_to_scheduled_task(row: &rusqlite::Row) -> rusqlite::Result<ScheduledTask> {
    let task_type_str: String = row.get(1)?;
    let task_type = ScheduledTaskType::from_str(&task_type_str)
        .unwrap_or(ScheduledTaskType::Scan);
    let photo_ids_json: Option<String> = row.get(3)?;
    let photo_ids = photo_ids_json.and_then(|json| {
        serde_json::from_str::<Vec<i64>>(&json).ok()
    });
    let status_str: String = row.get(7)?;
    let status = ScheduleStatus::from_str(&status_str)
        .unwrap_or(ScheduleStatus::Pending);
    Ok(ScheduledTask {
        id: row.get(0)?,
        task_type,
        target_path: row.get(2)?,
        photo_ids,
        scheduled_at: row.get(4)?,
        hours_start: row.get(5)?,
        hours_end: row.get(6)?,
        status,
        created_at: row.get(8)?,
        started_at: row.get(9)?,
        completed_at: row.get(10)?,
        error_message: row.get(11)?,
    })
}
