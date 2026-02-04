//! PostgreSQL backend implementation.

use anyhow::Result;
use postgres::NoTls;
use r2d2::Pool;
use r2d2_postgres::PostgresConnectionManager;
use std::path::Path;

use super::{PhotoMetadata, ExportedPhotoRow, exif_orientation_to_degrees, read_exif_rotation_from_file};
use super::embeddings::{SearchResult, EmbeddingRecord, embedding_to_bytes, bytes_to_embedding, cosine_similarity};
use super::faces::{
    BoundingBox, Face, FaceCluster, FaceWithPhoto, Person,
    embedding_to_bytes as face_embedding_to_bytes, bytes_to_embedding as face_bytes_to_embedding,
};
use super::similarity::{PhotoRecord, SimilarityGroup};
use super::trash::TrashedPhoto;
use super::schedule::{ScheduledTask, ScheduledTaskType, ScheduleStatus};
use super::albums::{UserTag, Album};
use super::postgres_schema::POSTGRES_SCHEMA;

pub struct PgDb {
    pool: Pool<PostgresConnectionManager<NoTls>>,
}

/// Compute hamming distance between two base64-encoded perceptual hashes.
/// Uses img_hash to decode and compare hashes, matching the scanner's approach.
fn hamming_distance(hash1: &str, hash2: &str) -> Result<u32> {
    use img_hash::ImageHash;

    let h1 = ImageHash::<Box<[u8]>>::from_base64(hash1)
        .map_err(|e| anyhow::anyhow!("Invalid hash1: {:?}", e))?;
    let h2 = ImageHash::<Box<[u8]>>::from_base64(hash2)
        .map_err(|e| anyhow::anyhow!("Invalid hash2: {:?}", e))?;

    Ok(h1.dist(&h2))
}

/// Helper to parse a postgres Row into a ScheduledTask.
fn row_to_scheduled_task(row: &postgres::Row) -> ScheduledTask {
    let task_type_str: String = row.get(1);
    let task_type = ScheduledTaskType::from_str(&task_type_str)
        .unwrap_or(ScheduledTaskType::Scan);
    let photo_ids_json: Option<String> = row.get(3);
    let photo_ids = photo_ids_json.and_then(|json| {
        serde_json::from_str::<Vec<i64>>(&json).ok()
    });
    let status_str: String = row.get(7);
    let status = ScheduleStatus::from_str(&status_str)
        .unwrap_or(ScheduleStatus::Pending);
    let hours_start: Option<i32> = row.get(5);
    let hours_end: Option<i32> = row.get(6);
    ScheduledTask {
        id: row.get(0),
        task_type,
        target_path: row.get(2),
        photo_ids,
        scheduled_at: row.get(4),
        hours_start: hours_start.map(|v| v as u8),
        hours_end: hours_end.map(|v| v as u8),
        status,
        created_at: row.get(8),
        started_at: row.get(9),
        completed_at: row.get(10),
        error_message: row.get(11),
    }
}

impl PgDb {
    pub fn open(url: &str, pool_size: u32) -> Result<Self> {
        let manager = PostgresConnectionManager::new(url.parse()?, NoTls);
        let pool = Pool::builder()
            .max_size(pool_size)
            .build(manager)?;
        Ok(Self { pool })
    }

    pub fn initialize(&self) -> Result<()> {
        let mut client = self.pool.get()?;
        client.batch_execute(POSTGRES_SCHEMA)?;
        Ok(())
    }

    // ========================================================================
    // Photo operations
    // ========================================================================

    pub fn save_description(&self, path: &Path, description: &str) -> Result<()> {
        self.ensure_photo_exists(path)?;
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET description = $1, llm_processed_at = CURRENT_TIMESTAMP WHERE path = $2",
            &[&description, &path_str.as_ref()],
        )?;
        Ok(())
    }

    pub fn get_description(&self, path: &Path) -> Result<Option<String>> {
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT description FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        match row {
            Some(row) => Ok(row.get(0)),
            None => Ok(None),
        }
    }

    pub fn update_photo_path(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        let old_path_str = old_path.to_string_lossy();
        let new_path_str = new_path.to_string_lossy();
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET path = $1 WHERE path = $2",
            &[&new_path_str.as_ref(), &old_path_str.as_ref()],
        )?;
        Ok(())
    }

    pub fn get_photos_mtime_in_dir(&self, directory: &str) -> Result<Vec<(String, Option<String>)>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT path, modified_at FROM photos WHERE directory = $1",
            &[&directory],
        )?;
        let results = rows
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect();
        Ok(results)
    }

    pub fn get_photo_metadata(&self, path: &Path) -> Result<Option<PhotoMetadata>> {
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            r#"
            SELECT id, path, filename, directory, size_bytes,
                   width, height, format,
                   camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                   gps_latitude, gps_longitude,
                   modified_at, scanned_at,
                   description, tags,
                   sha256_hash, perceptual_hash
            FROM photos
            WHERE path = $1
            "#,
            &[&path_str.as_ref()],
        )?;
        match row {
            Some(row) => {
                let photo_id: i64 = row.get(0);
                let width_i32: Option<i32> = row.get(5);
                let height_i32: Option<i32> = row.get(6);
                let iso_i32: Option<i32> = row.get(14);
                let mut metadata = PhotoMetadata {
                    id: photo_id,
                    path: row.get(1),
                    filename: row.get(2),
                    directory: row.get(3),
                    size_bytes: row.get(4),
                    width: width_i32.map(|v| v as i64),
                    height: height_i32.map(|v| v as i64),
                    format: row.get(7),
                    camera_make: row.get(8),
                    camera_model: row.get(9),
                    lens: row.get(10),
                    focal_length: row.get(11),
                    aperture: row.get(12),
                    shutter_speed: row.get(13),
                    iso: iso_i32.map(|v| v as i64),
                    taken_at: row.get(15),
                    gps_latitude: row.get(16),
                    gps_longitude: row.get(17),
                    modified_at: row.get(18),
                    scanned_at: row.get(19),
                    description: row.get(20),
                    tags: row.get(21),
                    sha256_hash: row.get(22),
                    perceptual_hash: row.get(23),
                    face_count: 0,
                    people_names: Vec::new(),
                };

                let face_count_row = client.query_one(
                    "SELECT COUNT(f.id) FROM faces f WHERE f.photo_id = $1",
                    &[&photo_id],
                )?;
                let count: i64 = face_count_row.get(0);
                metadata.face_count = count;

                let name_rows = client.query(
                    r#"
                    SELECT DISTINCT p.name
                    FROM faces f
                    JOIN people p ON f.person_id = p.id
                    WHERE f.photo_id = $1
                    ORDER BY p.name
                    "#,
                    &[&photo_id],
                )?;
                metadata.people_names = name_rows.iter().map(|r| r.get(0)).collect();

                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    pub fn semantic_search_by_text(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT id, path, filename, description FROM photos WHERE description IS NOT NULL",
            &[],
        )?;
        let mut results: Vec<SearchResult> = rows
            .iter()
            .filter_map(|row| {
                let id: i64 = row.get(0);
                let path: String = row.get(1);
                let filename: String = row.get(2);
                let description: String = row.get(3);
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
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT exif_orientation, user_rotation FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        match row {
            Some(row) => {
                let exif_orientation: i32 = row.get::<_, Option<i32>>(0).unwrap_or(1);
                let user_rotation: i32 = row.get::<_, Option<i32>>(1).unwrap_or(0);
                let exif_degrees = exif_orientation_to_degrees(exif_orientation);
                Ok((exif_degrees + user_rotation) % 360)
            }
            None => Ok(read_exif_rotation_from_file(path)),
        }
    }

    pub fn set_user_rotation(&self, path: &Path, rotation: i32) -> Result<()> {
        let path_str = path.to_string_lossy();
        let normalized = ((rotation % 360) + 360) % 360;
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET user_rotation = $1 WHERE path = $2",
            &[&normalized, &path_str.as_ref()],
        )?;
        Ok(())
    }

    pub fn rotate_photo_cw(&self, path: &Path) -> Result<i32> {
        self.ensure_photo_exists(path)?;
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT COALESCE(user_rotation, 0) FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        let current: i32 = row.map(|r| r.get(0)).unwrap_or(0);
        let new_rotation = (current + 90) % 360;
        client.execute(
            "UPDATE photos SET user_rotation = $1 WHERE path = $2",
            &[&new_rotation, &path_str.as_ref()],
        )?;
        drop(client);
        self.get_photo_rotation(path)
    }

    pub fn rotate_photo_ccw(&self, path: &Path) -> Result<i32> {
        self.ensure_photo_exists(path)?;
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT COALESCE(user_rotation, 0) FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        let current: i32 = row.map(|r| r.get(0)).unwrap_or(0);
        let new_rotation = (current + 270) % 360;
        client.execute(
            "UPDATE photos SET user_rotation = $1 WHERE path = $2",
            &[&new_rotation, &path_str.as_ref()],
        )?;
        drop(client);
        self.get_photo_rotation(path)
    }

    pub fn reset_photo_rotation(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET user_rotation = 0 WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        Ok(())
    }

    fn ensure_photo_exists(&self, path: &Path) -> Result<()> {
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT 1 FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        if row.is_none() {
            let filename = path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let directory = path.parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let size_bytes = std::fs::metadata(path)
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            client.execute(
                "INSERT INTO photos (path, filename, directory, size_bytes) VALUES ($1, $2, $3, $4)",
                &[&path_str.as_ref(), &filename.as_str(), &directory.as_str(), &size_bytes],
            )?;
        }
        Ok(())
    }

    // ========================================================================
    // Face operations
    // ========================================================================

    pub fn create_person(&self, name: &str) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "INSERT INTO people (name) VALUES ($1) RETURNING id",
            &[&name],
        )?;
        Ok(row.get(0))
    }

    pub fn find_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE LOWER(p.name) = LOWER($1)
            GROUP BY p.id
            "#,
            &[&name],
        )?;
        match row {
            Some(row) => Ok(Some(Person { id: row.get(0), name: row.get(1), face_count: row.get(2) })),
            None => Ok(None),
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
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE people SET name = $1, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
            &[&name, &person_id],
        )?;
        Ok(())
    }

    pub fn delete_person(&self, person_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute("DELETE FROM people WHERE id = $1", &[&person_id])?;
        Ok(())
    }

    pub fn get_all_people(&self) -> Result<Vec<Person>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            GROUP BY p.id
            ORDER BY p.name
            "#,
            &[],
        )?;
        let people = rows
            .iter()
            .map(|row| Person { id: row.get(0), name: row.get(1), face_count: row.get(2) })
            .collect();
        Ok(people)
    }

    pub fn get_person(&self, person_id: i64) -> Result<Option<Person>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE p.id = $1
            GROUP BY p.id
            "#,
            &[&person_id],
        )?;
        match row {
            Some(row) => Ok(Some(Person { id: row.get(0), name: row.get(1), face_count: row.get(2) })),
            None => Ok(None),
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
        let confidence_f64 = confidence.map(|c| c as f64);
        let mut client = self.pool.get()?;
        let row = client.query_one(
            r#"
            INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, embedding_dim, confidence)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id
            "#,
            &[&photo_id, &bbox.x, &bbox.y, &bbox.width, &bbox.height,
              &embedding_bytes.as_deref(), &embedding_dim, &confidence_f64],
        )?;
        Ok(row.get(0))
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<Face>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, person_id, confidence
            FROM faces
            WHERE photo_id = $1
            "#,
            &[&photo_id],
        )?;
        let faces = rows
            .iter()
            .map(|row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6);
                let confidence_f64: Option<f64> = row.get(8);
                Face {
                    id: row.get(0),
                    photo_id: row.get(1),
                    bbox: BoundingBox { x: row.get(2), y: row.get(3), width: row.get(4), height: row.get(5) },
                    embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                    person_id: row.get(7),
                    confidence: confidence_f64.map(|c| c as f32),
                }
            })
            .collect();
        Ok(faces)
    }

    pub fn get_faces_for_person(&self, person_id: i64) -> Result<Vec<FaceWithPhoto>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id = $1
            ORDER BY p.taken_at DESC
            "#,
            &[&person_id],
        )?;
        let faces = rows
            .iter()
            .map(|row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6);
                let confidence_f64: Option<f64> = row.get(8);
                FaceWithPhoto {
                    face: Face {
                        id: row.get(0),
                        photo_id: row.get(1),
                        bbox: BoundingBox { x: row.get(2), y: row.get(3), width: row.get(4), height: row.get(5) },
                        embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                        person_id: row.get(7),
                        confidence: confidence_f64.map(|c| c as f32),
                    },
                    photo_path: row.get(9),
                    photo_filename: row.get(10),
                }
            })
            .collect();
        Ok(faces)
    }

    pub fn assign_face_to_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE faces SET person_id = $1 WHERE id = $2",
            &[&person_id, &face_id],
        )?;
        Ok(())
    }

    pub fn unassign_face(&self, face_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        let null_id: Option<i64> = None;
        client.execute(
            "UPDATE faces SET person_id = $1 WHERE id = $2",
            &[&null_id, &face_id],
        )?;
        Ok(())
    }

    pub fn get_unassigned_faces(&self) -> Result<Vec<FaceWithPhoto>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id IS NULL
            ORDER BY p.taken_at DESC
            "#,
            &[],
        )?;
        let faces = rows
            .iter()
            .map(|row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6);
                let confidence_f64: Option<f64> = row.get(8);
                FaceWithPhoto {
                    face: Face {
                        id: row.get(0),
                        photo_id: row.get(1),
                        bbox: BoundingBox { x: row.get(2), y: row.get(3), width: row.get(4), height: row.get(5) },
                        embedding: embedding_bytes.map(|b| face_bytes_to_embedding(&b)),
                        person_id: row.get(7),
                        confidence: confidence_f64.map(|c| c as f32),
                    },
                    photo_path: row.get(9),
                    photo_filename: row.get(10),
                }
            })
            .collect();
        Ok(faces)
    }

    pub fn get_photos_without_faces_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
              AND p.path LIKE $1
            LIMIT $2
            "#,
            &[&dir_pattern, &limit_i64],
        )?;
        let results = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(results)
    }

    pub fn mark_photo_scanned(&self, photo_id: i64, faces_found: usize) -> Result<()> {
        let faces_found_i32 = faces_found as i32;
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            INSERT INTO face_scans (photo_id, faces_found, scanned_at)
            VALUES ($1, $2, CURRENT_TIMESTAMP)
            ON CONFLICT (photo_id) DO UPDATE SET faces_found = $2, scanned_at = CURRENT_TIMESTAMP
            "#,
            &[&photo_id, &faces_found_i32],
        )?;
        Ok(())
    }

    pub fn count_photos_needing_face_scan(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            r#"
            SELECT COUNT(*)
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
            "#,
            &[],
        )?;
        Ok(row.get(0))
    }

    pub fn count_faces(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one("SELECT COUNT(*) FROM faces", &[])?;
        Ok(row.get(0))
    }

    pub fn count_people(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one("SELECT COUNT(*) FROM people", &[])?;
        Ok(row.get(0))
    }

    pub fn get_all_face_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT id, embedding FROM faces WHERE embedding IS NOT NULL",
            &[],
        )?;
        let results = rows
            .iter()
            .map(|row| {
                let bytes: Vec<u8> = row.get(1);
                (row.get(0), face_bytes_to_embedding(&bytes))
            })
            .collect();
        Ok(results)
    }

    pub fn get_faces_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, i64, BoundingBox)>> {
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h
            FROM faces
            WHERE embedding IS NULL
            LIMIT $1
            "#,
            &[&limit_i64],
        )?;
        let results = rows
            .iter()
            .map(|row| {
                (row.get(0), row.get(1), BoundingBox { x: row.get(2), y: row.get(3), width: row.get(4), height: row.get(5) })
            })
            .collect();
        Ok(results)
    }

    pub fn get_photo_path(&self, photo_id: i64) -> Result<Option<String>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT path FROM photos WHERE id = $1",
            &[&photo_id],
        )?;
        Ok(row.map(|r| r.get(0)))
    }

    pub fn update_face_embedding(&self, face_id: i64, embedding: &[f32]) -> Result<()> {
        let embedding_bytes = face_embedding_to_bytes(embedding);
        let embedding_dim = embedding.len() as i32;
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE faces SET embedding = $1, embedding_dim = $2 WHERE id = $3",
            &[&embedding_bytes, &embedding_dim, &face_id],
        )?;
        Ok(())
    }

    pub fn count_faces_without_embeddings(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "SELECT COUNT(*) FROM faces WHERE embedding IS NULL",
            &[],
        )?;
        Ok(row.get(0))
    }

    pub fn create_face_cluster(&self, representative_face_id: Option<i64>, auto_name: &str) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "INSERT INTO face_clusters (representative_face_id, auto_name) VALUES ($1, $2) RETURNING id",
            &[&representative_face_id, &auto_name],
        )?;
        Ok(row.get(0))
    }

    pub fn add_face_to_cluster(&self, face_id: i64, cluster_id: i64, similarity_score: f32) -> Result<()> {
        let score_f64 = similarity_score as f64;
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            INSERT INTO face_cluster_members (face_id, cluster_id, similarity_score)
            VALUES ($1, $2, $3)
            ON CONFLICT (face_id, cluster_id) DO UPDATE SET similarity_score = $3
            "#,
            &[&face_id, &cluster_id, &score_f64],
        )?;
        Ok(())
    }

    pub fn get_all_face_clusters(&self) -> Result<Vec<FaceCluster>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT fc.id, fc.auto_name, fc.representative_face_id, COUNT(fcm.face_id) as face_count
            FROM face_clusters fc
            LEFT JOIN face_cluster_members fcm ON fc.id = fcm.cluster_id
            GROUP BY fc.id
            ORDER BY face_count DESC
            "#,
            &[],
        )?;
        let clusters = rows
            .iter()
            .map(|row| {
                FaceCluster {
                    id: row.get(0),
                    auto_name: row.get::<_, Option<String>>(1).unwrap_or_default(),
                    representative_face_id: row.get(2),
                    face_count: row.get(3),
                }
            })
            .collect();
        Ok(clusters)
    }

    pub fn clear_face_clusters(&self) -> Result<()> {
        let mut client = self.pool.get()?;
        let mut tx = client.transaction()?;
        tx.execute("DELETE FROM face_cluster_members", &[])?;
        tx.execute("DELETE FROM face_clusters", &[])?;
        tx.commit()?;
        Ok(())
    }

    pub fn cluster_to_person(&self, cluster_id: i64, person_name: &str) -> Result<i64> {
        let mut client = self.pool.get()?;
        let mut tx = client.transaction()?;
        let row = tx.query_one(
            "INSERT INTO people (name) VALUES ($1) RETURNING id",
            &[&person_name],
        )?;
        let person_id: i64 = row.get(0);
        tx.execute(
            r#"
            UPDATE faces SET person_id = $1
            WHERE id IN (SELECT face_id FROM face_cluster_members WHERE cluster_id = $2)
            "#,
            &[&person_id, &cluster_id],
        )?;
        tx.execute(
            "DELETE FROM face_cluster_members WHERE cluster_id = $1",
            &[&cluster_id],
        )?;
        tx.execute(
            "DELETE FROM face_clusters WHERE id = $1",
            &[&cluster_id],
        )?;
        tx.commit()?;
        Ok(person_id)
    }

    pub fn search_photos_by_person(&self, person_id: i64) -> Result<Vec<(i64, String, String)>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT DISTINCT p.id, p.path, p.filename
            FROM photos p
            JOIN faces f ON p.id = f.photo_id
            WHERE f.person_id = $1
            ORDER BY p.taken_at DESC
            "#,
            &[&person_id],
        )?;
        let results = rows.iter().map(|row| (row.get(0), row.get(1), row.get(2))).collect();
        Ok(results)
    }

    // ========================================================================
    // Embedding operations
    // ========================================================================

    pub fn store_embedding(&self, photo_id: i64, embedding: &[f32], model_name: &str) -> Result<()> {
        let bytes = embedding_to_bytes(embedding);
        let dim = embedding.len() as i32;
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            INSERT INTO embeddings (photo_id, embedding, embedding_dim, model_name, created_at)
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
            ON CONFLICT (photo_id) DO UPDATE SET embedding = $2, embedding_dim = $3, model_name = $4, created_at = CURRENT_TIMESTAMP
            "#,
            &[&photo_id, &bytes, &dim, &model_name],
        )?;
        Ok(())
    }

    pub fn get_embedding(&self, photo_id: i64) -> Result<Option<EmbeddingRecord>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT photo_id, embedding, model_name FROM embeddings WHERE photo_id = $1",
            &[&photo_id],
        )?;
        match row {
            Some(row) => {
                let bytes: Vec<u8> = row.get(1);
                Ok(Some(EmbeddingRecord {
                    photo_id: row.get(0),
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2),
                }))
            }
            None => Ok(None),
        }
    }

    pub fn get_all_embeddings(&self) -> Result<Vec<EmbeddingRecord>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT photo_id, embedding, model_name FROM embeddings",
            &[],
        )?;
        let records = rows
            .iter()
            .map(|row| {
                let bytes: Vec<u8> = row.get(1);
                EmbeddingRecord {
                    photo_id: row.get(0),
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2),
                }
            })
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
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT id, path, filename, description FROM photos WHERE id = $1",
            &[&photo_id],
        )?;
        match row {
            Some(row) => Ok(Some(SearchResult {
                photo_id: row.get(0),
                path: row.get(1),
                filename: row.get(2),
                similarity,
                description: row.get(3),
            })),
            None => Ok(None),
        }
    }

    pub fn get_photos_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
            LIMIT $1
            "#,
            &[&limit_i64],
        )?;
        let results = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(results)
    }

    pub fn get_photos_without_embeddings_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
              AND p.path LIKE $1
            LIMIT $2
            "#,
            &[&dir_pattern, &limit_i64],
        )?;
        let results = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(results)
    }

    pub fn count_embeddings(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one("SELECT COUNT(*) FROM embeddings", &[])?;
        Ok(row.get(0))
    }

    // ========================================================================
    // Similarity operations
    // ========================================================================

    pub fn find_exact_duplicates(&self) -> Result<Vec<SimilarityGroup>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT sha256_hash, COUNT(*) as cnt
            FROM photos
            WHERE sha256_hash IS NOT NULL
            GROUP BY sha256_hash
            HAVING COUNT(*) > 1
            "#,
            &[],
        )?;
        let duplicate_hashes: Vec<String> = rows.iter().map(|row| row.get(0)).collect();
        drop(client);

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
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE sha256_hash = $1
            ORDER BY taken_at, path
            "#,
            &[&sha256],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                let width_i32: Option<i32> = row.get(4);
                let height_i32: Option<i32> = row.get(5);
                let marked: bool = row.get(9);
                PhotoRecord {
                    id: row.get(0),
                    path: row.get(1),
                    filename: row.get(2),
                    size_bytes: row.get(3),
                    width: width_i32.map(|v| v as u32),
                    height: height_i32.map(|v| v as u32),
                    sha256_hash: row.get(6),
                    perceptual_hash: row.get(7),
                    taken_at: row.get(8),
                    marked_for_deletion: marked,
                }
            })
            .collect();
        Ok(photos)
    }

    fn get_all_photos_with_phash(&self) -> Result<Vec<PhotoRecord>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE perceptual_hash IS NOT NULL
            ORDER BY path
            "#,
            &[],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                let width_i32: Option<i32> = row.get(4);
                let height_i32: Option<i32> = row.get(5);
                let marked: bool = row.get(9);
                PhotoRecord {
                    id: row.get(0),
                    path: row.get(1),
                    filename: row.get(2),
                    size_bytes: row.get(3),
                    width: width_i32.map(|v| v as u32),
                    height: height_i32.map(|v| v as u32),
                    sha256_hash: row.get(6),
                    perceptual_hash: row.get(7),
                    taken_at: row.get(8),
                    marked_for_deletion: marked,
                }
            })
            .collect();
        Ok(photos)
    }

    pub fn mark_for_deletion(&self, photo_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET marked_for_deletion = true WHERE id = $1",
            &[&photo_id],
        )?;
        Ok(())
    }

    pub fn unmark_for_deletion(&self, photo_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET marked_for_deletion = false WHERE id = $1",
            &[&photo_id],
        )?;
        Ok(())
    }

    pub fn get_marked_for_deletion(&self) -> Result<Vec<PhotoRecord>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE marked_for_deletion = true
            ORDER BY path
            "#,
            &[],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                let width_i32: Option<i32> = row.get(4);
                let height_i32: Option<i32> = row.get(5);
                let marked: bool = row.get(9);
                PhotoRecord {
                    id: row.get(0),
                    path: row.get(1),
                    filename: row.get(2),
                    size_bytes: row.get(3),
                    width: width_i32.map(|v| v as u32),
                    height: height_i32.map(|v| v as u32),
                    sha256_hash: row.get(6),
                    perceptual_hash: row.get(7),
                    taken_at: row.get(8),
                    marked_for_deletion: marked,
                }
            })
            .collect();
        Ok(photos)
    }

    pub fn delete_marked_photos(&self) -> Result<usize> {
        let mut client = self.pool.get()?;
        let count = client.execute("DELETE FROM photos WHERE marked_for_deletion = true", &[])?;
        Ok(count as usize)
    }

    pub fn delete_photos_by_ids(&self, ids: &[i64]) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let sql = format!("DELETE FROM photos WHERE id IN ({})", placeholders.join(", "));
        let params: Vec<&(dyn postgres::types::ToSql + Sync)> = ids.iter().map(|id| id as &(dyn postgres::types::ToSql + Sync)).collect();
        let mut client = self.pool.get()?;
        let count = client.execute(&sql as &str, &params)?;
        Ok(count as usize)
    }

    pub fn get_photo_count(&self) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one("SELECT COUNT(*) FROM photos", &[])?;
        Ok(row.get(0))
    }

    // ========================================================================
    // Trash operations
    // ========================================================================

    pub fn mark_trashed(&self, photo_id: i64, trash_path: &Path) -> Result<()> {
        let mut client = self.pool.get()?;
        let orig_row = client.query_one(
            "SELECT path FROM photos WHERE id = $1",
            &[&photo_id],
        )?;
        let original_path: String = orig_row.get(0);
        let trash_path_str = trash_path.to_string_lossy().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        client.execute(
            r#"
            UPDATE photos
            SET path = $1,
                original_path = $2,
                trashed_at = $3,
                marked_for_deletion = false
            WHERE id = $4
            "#,
            &[&trash_path_str.as_str(), &original_path.as_str(), &now.as_str(), &photo_id],
        )?;
        Ok(())
    }

    pub fn get_trashed_photos(&self) -> Result<Vec<TrashedPhoto>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, original_path, filename, trashed_at, size_bytes
            FROM photos
            WHERE trashed_at IS NOT NULL
            ORDER BY trashed_at DESC
            "#,
            &[],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                TrashedPhoto {
                    id: row.get(0),
                    path: row.get(1),
                    original_path: row.get(2),
                    filename: row.get(3),
                    trashed_at: row.get(4),
                    size_bytes: row.get(5),
                }
            })
            .collect();
        Ok(photos)
    }

    pub fn restore_photo(&self, photo_id: i64) -> Result<String> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "SELECT original_path FROM photos WHERE id = $1",
            &[&photo_id],
        )?;
        let original_path: String = row.get(0);
        client.execute(
            r#"
            UPDATE photos
            SET path = original_path,
                original_path = NULL,
                trashed_at = NULL
            WHERE id = $1
            "#,
            &[&photo_id],
        )?;
        Ok(original_path)
    }

    pub fn delete_trashed_photo(&self, photo_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute("DELETE FROM photos WHERE id = $1", &[&photo_id])?;
        Ok(())
    }

    pub fn get_marked_not_trashed(&self) -> Result<Vec<PhotoRecord>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE marked_for_deletion = true AND trashed_at IS NULL
            ORDER BY path
            "#,
            &[],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                let width_i32: Option<i32> = row.get(4);
                let height_i32: Option<i32> = row.get(5);
                let marked: bool = row.get(9);
                PhotoRecord {
                    id: row.get(0),
                    path: row.get(1),
                    filename: row.get(2),
                    size_bytes: row.get(3),
                    width: width_i32.map(|v| v as u32),
                    height: height_i32.map(|v| v as u32),
                    sha256_hash: row.get(6),
                    perceptual_hash: row.get(7),
                    taken_at: row.get(8),
                    marked_for_deletion: marked,
                }
            })
            .collect();
        Ok(photos)
    }

    pub fn get_old_trashed_photos(&self, max_age_days: u32) -> Result<Vec<TrashedPhoto>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(max_age_days as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path, original_path, filename, trashed_at, size_bytes
            FROM photos
            WHERE trashed_at IS NOT NULL AND trashed_at < $1
            ORDER BY trashed_at
            "#,
            &[&cutoff_str],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                TrashedPhoto {
                    id: row.get(0),
                    path: row.get(1),
                    original_path: row.get(2),
                    filename: row.get(3),
                    trashed_at: row.get(4),
                    size_bytes: row.get(5),
                }
            })
            .collect();
        Ok(photos)
    }

    pub fn get_trash_total_size(&self) -> Result<u64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM photos WHERE trashed_at IS NOT NULL",
            &[],
        )?;
        let size: i64 = row.get(0);
        Ok(size as u64)
    }

    // ========================================================================
    // Schedule operations
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
        let hours_start_i32 = hours_start.map(|v| v as i32);
        let hours_end_i32 = hours_end.map(|v| v as i32);
        let mut client = self.pool.get()?;
        let row = client.query_one(
            r#"
            INSERT INTO scheduled_tasks (
                task_type, target_path, photo_ids, scheduled_at, hours_start, hours_end
            ) VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
            &[
                &task_type.as_str(),
                &target_path,
                &photo_ids_json,
                &scheduled_at,
                &hours_start_i32,
                &hours_end_i32,
            ],
        )?;
        Ok(row.get(0))
    }

    pub fn get_pending_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending'
            ORDER BY scheduled_at ASC
            "#,
            &[],
        )?;
        let tasks = rows.iter().map(|row| row_to_scheduled_task(row)).collect();
        Ok(tasks)
    }

    pub fn get_overdue_schedules(&self, now: &str) -> Result<Vec<ScheduledTask>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending' AND scheduled_at < $1
            ORDER BY scheduled_at ASC
            "#,
            &[&now],
        )?;
        let tasks = rows.iter().map(|row| row_to_scheduled_task(row)).collect();
        Ok(tasks)
    }

    pub fn get_all_schedules(&self) -> Result<Vec<ScheduledTask>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            ORDER BY scheduled_at DESC
            LIMIT 100
            "#,
            &[],
        )?;
        let tasks = rows.iter().map(|row| row_to_scheduled_task(row)).collect();
        Ok(tasks)
    }

    pub fn update_schedule_status(
        &self,
        id: i64,
        status: ScheduleStatus,
        error_message: Option<&str>,
    ) -> Result<()> {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        let mut client = self.pool.get()?;
        match status {
            ScheduleStatus::Running => {
                client.execute(
                    "UPDATE scheduled_tasks SET status = $1, started_at = $2 WHERE id = $3",
                    &[&status.as_str(), &now.as_str(), &id],
                )?;
            }
            ScheduleStatus::Completed | ScheduleStatus::Failed | ScheduleStatus::Cancelled => {
                client.execute(
                    "UPDATE scheduled_tasks SET status = $1, completed_at = $2, error_message = $3 WHERE id = $4",
                    &[&status.as_str(), &now.as_str(), &error_message, &id],
                )?;
            }
            ScheduleStatus::Pending => {
                client.execute(
                    "UPDATE scheduled_tasks SET status = $1 WHERE id = $2",
                    &[&status.as_str(), &id],
                )?;
            }
        }
        Ok(())
    }

    pub fn cancel_schedule(&self, id: i64) -> Result<()> {
        self.update_schedule_status(id, ScheduleStatus::Cancelled, None)
    }

    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute("DELETE FROM scheduled_tasks WHERE id = $1", &[&id])?;
        Ok(())
    }

    // ========================================================================
    // Daemon-specific schedule operations
    // ========================================================================

    pub fn get_due_pending_tasks(&self, limit: usize) -> Result<Vec<ScheduledTask>> {
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, task_type, target_path, photo_ids, scheduled_at,
                   hours_start, hours_end, status, created_at,
                   started_at, completed_at, error_message
            FROM scheduled_tasks
            WHERE status = 'pending'
              AND (scheduled_at IS NULL OR scheduled_at <= NOW())
            ORDER BY scheduled_at ASC
            LIMIT $1
            "#,
            &[&limit_i64],
        )?;
        let tasks = rows.iter().map(|row| row_to_scheduled_task(row)).collect();
        Ok(tasks)
    }

    pub fn mark_task_running(&self, task_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE scheduled_tasks SET status = 'running', started_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&task_id],
        )?;
        Ok(())
    }

    pub fn mark_task_completed(&self, task_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE scheduled_tasks SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = $1",
            &[&task_id],
        )?;
        Ok(())
    }

    pub fn mark_task_failed(&self, task_id: i64, error: &str) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE scheduled_tasks SET status = 'failed', error_message = $1, completed_at = CURRENT_TIMESTAMP WHERE id = $2",
            &[&error, &task_id],
        )?;
        Ok(())
    }

    // ========================================================================
    // Album operations
    // ========================================================================

    pub fn get_all_tags(&self) -> Result<Vec<UserTag>> {
        let mut client = self.pool.get()?;
        let rows = client.query("SELECT id, name, color FROM user_tags ORDER BY name", &[])?;
        let tags = rows
            .iter()
            .map(|row| UserTag { id: row.get(0), name: row.get(1), color: row.get(2) })
            .collect();
        Ok(tags)
    }

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<i64> {
        let color = color.unwrap_or("#808080");
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "INSERT INTO user_tags (name, color) VALUES ($1, $2) RETURNING id",
            &[&name, &color],
        )?;
        Ok(row.get(0))
    }

    pub fn get_or_create_tag(&self, name: &str) -> Result<UserTag> {
        let mut client = self.pool.get()?;
        let existing = client.query_opt(
            "SELECT id, name, color FROM user_tags WHERE LOWER(name) = LOWER($1)",
            &[&name],
        )?;
        match existing {
            Some(row) => Ok(UserTag { id: row.get(0), name: row.get(1), color: row.get(2) }),
            None => {
                drop(client);
                let id = self.create_tag(name, None)?;
                Ok(UserTag { id, name: name.to_string(), color: "#808080".to_string() })
            }
        }
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute("DELETE FROM user_tags WHERE id = $1", &[&tag_id])?;
        Ok(())
    }

    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE user_tags SET name = $1 WHERE id = $2",
            &[&new_name, &tag_id],
        )?;
        Ok(())
    }

    pub fn get_photo_tags(&self, photo_id: i64) -> Result<Vec<UserTag>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT t.id, t.name, t.color
            FROM user_tags t
            JOIN photo_user_tags pt ON pt.tag_id = t.id
            WHERE pt.photo_id = $1
            ORDER BY t.name
            "#,
            &[&photo_id],
        )?;
        let tags = rows
            .iter()
            .map(|row| UserTag { id: row.get(0), name: row.get(1), color: row.get(2) })
            .collect();
        Ok(tags)
    }

    pub fn add_tag_to_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "INSERT INTO photo_user_tags (photo_id, tag_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&photo_id, &tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag_from_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "DELETE FROM photo_user_tags WHERE photo_id = $1 AND tag_id = $2",
            &[&photo_id, &tag_id],
        )?;
        Ok(())
    }

    pub fn get_photos_with_tag(&self, tag_id: i64) -> Result<Vec<i64>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT photo_id FROM photo_user_tags WHERE tag_id = $1",
            &[&tag_id],
        )?;
        let ids = rows.iter().map(|row| row.get(0)).collect();
        Ok(ids)
    }

    pub fn search_tags(&self, prefix: &str) -> Result<Vec<UserTag>> {
        let pattern = format!("{}%", prefix);
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT id, name, color FROM user_tags WHERE name ILIKE $1 ORDER BY name LIMIT 10",
            &[&pattern],
        )?;
        let tags = rows
            .iter()
            .map(|row| UserTag { id: row.get(0), name: row.get(1), color: row.get(2) })
            .collect();
        Ok(tags)
    }

    pub fn get_all_albums(&self) -> Result<Vec<Album>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT a.id, a.name, a.description, a.cover_photo_id, a.is_smart, a.filter_tags,
                   (SELECT COUNT(*) FROM album_photos WHERE album_id = a.id) as photo_count
            FROM albums a
            ORDER BY a.name
            "#,
            &[],
        )?;
        let albums = rows
            .iter()
            .map(|row| {
                let filter_tags_json: Option<String> = row.get(5);
                let filter_tags: Vec<i64> = filter_tags_json
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default();
                let is_smart: bool = row.get(4);
                Album {
                    id: row.get(0),
                    name: row.get(1),
                    description: row.get(2),
                    cover_photo_id: row.get(3),
                    is_smart,
                    filter_tags,
                    photo_count: row.get(6),
                }
            })
            .collect();
        Ok(albums)
    }

    pub fn create_album(&self, name: &str, description: Option<&str>, is_smart: bool) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "INSERT INTO albums (name, description, is_smart) VALUES ($1, $2, $3) RETURNING id",
            &[&name, &description, &is_smart],
        )?;
        Ok(row.get(0))
    }

    pub fn delete_album(&self, album_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute("DELETE FROM albums WHERE id = $1", &[&album_id])?;
        Ok(())
    }

    pub fn add_photo_to_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "INSERT INTO album_photos (album_id, photo_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            &[&album_id, &photo_id],
        )?;
        Ok(())
    }

    pub fn remove_photo_from_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "DELETE FROM album_photos WHERE album_id = $1 AND photo_id = $2",
            &[&album_id, &photo_id],
        )?;
        Ok(())
    }

    pub fn get_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            "SELECT photo_id FROM album_photos WHERE album_id = $1 ORDER BY position, added_at",
            &[&album_id],
        )?;
        let ids = rows.iter().map(|row| row.get(0)).collect();
        Ok(ids)
    }

    pub fn get_album_photo_paths(&self, album_id: i64) -> Result<Vec<String>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT p.path
            FROM photos p
            JOIN album_photos ap ON ap.photo_id = p.id
            WHERE ap.album_id = $1
            ORDER BY ap.position, ap.added_at
            "#,
            &[&album_id],
        )?;
        let paths = rows.iter().map(|row| row.get(0)).collect();
        Ok(paths)
    }

    pub fn set_album_filter_tags(&self, album_id: i64, tag_ids: &[i64]) -> Result<()> {
        let json = serde_json::to_string(tag_ids)?;
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE albums SET filter_tags = $1, is_smart = true, updated_at = CURRENT_TIMESTAMP WHERE id = $2",
            &[&json, &album_id],
        )?;
        Ok(())
    }

    pub fn get_smart_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "SELECT filter_tags FROM albums WHERE id = $1",
            &[&album_id],
        )?;
        let filter_json: Option<String> = row.get(0);
        let tag_ids: Vec<i64> = filter_json
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default();
        if tag_ids.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> = (1..=tag_ids.len()).map(|i| format!("${}", i)).collect();
        let count_param = format!("${}", tag_ids.len() + 1);
        let query = format!(
            r#"
            SELECT photo_id
            FROM photo_user_tags
            WHERE tag_id IN ({})
            GROUP BY photo_id
            HAVING COUNT(DISTINCT tag_id) = {}
            "#,
            placeholders.join(","),
            count_param,
        );
        let tag_count = tag_ids.len() as i64;
        let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> = tag_ids
            .iter()
            .map(|id| id as &(dyn postgres::types::ToSql + Sync))
            .collect();
        params.push(&tag_count);
        let rows = client.query(&query as &str, &params)?;
        let ids: Vec<i64> = rows.iter().map(|row| row.get(0)).collect();
        Ok(ids)
    }

    // ========================================================================
    // LLM queue operations
    // ========================================================================

    pub fn save_llm_result(&self, photo_id: i64, description: &str, tags_json: &str) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            UPDATE photos
            SET description = $1, tags = $2, llm_processed_at = CURRENT_TIMESTAMP
            WHERE id = $3
            "#,
            &[&description, &tags_json, &photo_id],
        )?;
        Ok(())
    }

    pub fn get_photos_without_description(&self) -> Result<Vec<(i64, String)>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL
            ORDER BY scanned_at DESC
            "#,
            &[],
        )?;
        let tasks = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(tasks)
    }

    pub fn get_photos_without_description_in_dir(&self, directory: &Path) -> Result<Vec<(i64, String)>> {
        let dir_str = directory.to_string_lossy();
        let pattern = format!("{}%", dir_str);
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path FROM photos
            WHERE description IS NULL AND path LIKE $1
            ORDER BY path ASC
            "#,
            &[&pattern],
        )?;
        let tasks = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(tasks)
    }

    pub fn get_photo_description(&self, photo_id: i64) -> Result<Option<String>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT description FROM photos WHERE id = $1",
            &[&photo_id],
        )?;
        Ok(row.and_then(|r| r.get(0)))
    }

    // ========================================================================
    // Scanner operations
    // ========================================================================

    pub fn photo_exists(&self, path: &Path) -> Result<bool> {
        let path_str = path.to_string_lossy();
        let mut client = self.pool.get()?;
        let row = client.query_one(
            "SELECT COUNT(*) FROM photos WHERE path = $1",
            &[&path_str.as_ref()],
        )?;
        let count: i64 = row.get(0);
        Ok(count > 0)
    }

    #[allow(clippy::too_many_arguments)]
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
        let width_i32 = width.map(|w| w as i32);
        let height_i32 = height.map(|h| h as i32);
        let iso_i32 = iso.map(|i| i as i32);
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            INSERT INTO photos (
                path, filename, directory, size_bytes, modified_at,
                width, height, format,
                camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
                gps_latitude, gps_longitude, all_exif,
                md5_hash, sha256_hash, perceptual_hash,
                exif_orientation
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
            "#,
            &[
                &path, &filename, &directory, &size_bytes, &modified_at,
                &width_i32, &height_i32, &format,
                &camera_make, &camera_model, &lens, &focal_length, &aperture, &shutter_speed, &iso_i32, &taken_at,
                &gps_lat, &gps_lon, &all_exif,
                &md5_hash, &sha256_hash, &perceptual_hash,
                &exif_orientation,
            ],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
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
        let width_i32 = width.map(|w| w as i32);
        let height_i32 = height.map(|h| h as i32);
        let iso_i32 = iso.map(|i| i as i32);
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            UPDATE photos SET
                filename = $1, directory = $2, size_bytes = $3, modified_at = $4,
                width = $5, height = $6, format = $7,
                camera_make = $8, camera_model = $9, lens = $10, focal_length = $11, aperture = $12, shutter_speed = $13, iso = $14, taken_at = $15,
                gps_latitude = $16, gps_longitude = $17, all_exif = $18,
                md5_hash = $19, sha256_hash = $20, perceptual_hash = $21,
                exif_orientation = $22,
                scanned_at = CURRENT_TIMESTAMP
            WHERE path = $23
            "#,
            &[
                &filename, &directory, &size_bytes, &modified_at,
                &width_i32, &height_i32, &format,
                &camera_make, &camera_model, &lens, &focal_length, &aperture, &shutter_speed, &iso_i32, &taken_at,
                &gps_lat, &gps_lon, &all_exif,
                &md5_hash, &sha256_hash, &perceptual_hash,
                &exif_orientation,
                &path,
            ],
        )?;
        Ok(())
    }

    // ========================================================================
    // Export operations
    // ========================================================================

    pub fn get_photos_for_export(&self) -> Result<Vec<ExportedPhotoRow>> {
        let mut client = self.pool.get()?;
        let rows = client.query(
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
            &[],
        )?;
        let photos = rows
            .iter()
            .map(|row| {
                let path: String = row.get(0);
                let filename = std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let width_i32: Option<i32> = row.get(1);
                let height_i32: Option<i32> = row.get(2);
                let size_bytes: Option<i64> = row.get(3);
                ExportedPhotoRow {
                    path,
                    filename,
                    width: width_i32.map(|v| v as u32),
                    height: height_i32.map(|v| v as u32),
                    file_size: size_bytes.map(|v| v as u64),
                    sha256: row.get(4),
                    perceptual_hash: row.get(5),
                    camera_make: row.get(6),
                    camera_model: row.get(7),
                    date_taken: row.get(8),
                    description: row.get(9),
                    scanned_at: row.get(10),
                }
            })
            .collect();
        Ok(photos)
    }

    // ========================================================================
    // Daemon scan operations
    // ========================================================================

    pub fn photo_exists_by_path(&self, path: &str) -> bool {
        let mut client = match self.pool.get() {
            Ok(c) => c,
            Err(_) => return false,
        };
        let row = client.query_opt(
            "SELECT 1 FROM photos WHERE path = $1",
            &[&path],
        );
        match row {
            Ok(Some(_)) => true,
            _ => false,
        }
    }

    pub fn insert_basic_photo(&self, path: &str, filename: &str, directory: &str, size: i64) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            r#"
            INSERT INTO photos (path, filename, directory, size_bytes, scanned_at)
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
            ON CONFLICT DO NOTHING
            "#,
            &[&path, &filename, &directory, &size],
        )?;
        Ok(())
    }

    pub fn get_photos_without_description_in_directory(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let limit_i64 = limit as i64;
        let mut client = self.pool.get()?;
        let rows = client.query(
            r#"
            SELECT id, path
            FROM photos
            WHERE directory = $1 AND description IS NULL
            LIMIT $2
            "#,
            &[&directory, &limit_i64],
        )?;
        let results = rows.iter().map(|row| (row.get(0), row.get(1))).collect();
        Ok(results)
    }

    pub fn save_photo_description_by_id(&self, photo_id: i64, description: &str) -> Result<()> {
        let mut client = self.pool.get()?;
        client.execute(
            "UPDATE photos SET description = $1, llm_processed_at = CURRENT_TIMESTAMP WHERE id = $2",
            &[&description, &photo_id],
        )?;
        Ok(())
    }

    // ========================================================================
    // Directory prompt operations
    // ========================================================================

    pub fn get_directory_prompt(&self, directory: &str) -> Result<Option<String>> {
        let mut client = self.pool.get()?;
        let row = client.query_opt(
            "SELECT custom_prompt FROM directory_prompts WHERE directory = $1",
            &[&directory],
        )?;
        Ok(row.map(|r| r.get(0)))
    }

    pub fn set_directory_prompt(&self, directory: &str, prompt: &str) -> Result<()> {
        let mut client = self.pool.get()?;
        if prompt.is_empty() {
            client.execute(
                "DELETE FROM directory_prompts WHERE directory = $1",
                &[&directory],
            )?;
        } else {
            client.execute(
                r#"
                INSERT INTO directory_prompts (directory, custom_prompt, updated_at)
                VALUES ($1, $2, NOW())
                ON CONFLICT (directory) DO UPDATE SET custom_prompt = $2, updated_at = NOW()
                "#,
                &[&directory, &prompt],
            )?;
        }
        Ok(())
    }

    pub fn count_photos_without_faces_in_dir(&self, directory: &str) -> Result<i64> {
        let mut client = self.pool.get()?;
        let row = client.query_one(
            r#"
            SELECT COUNT(*)
            FROM photos p
            WHERE p.directory = $1
              AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.photo_id = p.id)
            "#,
            &[&directory],
        )?;
        Ok(row.get(0))
    }
}
