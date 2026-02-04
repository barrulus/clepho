mod schema;
pub mod albums;
pub mod embeddings;
pub mod faces;
pub mod schedule;
pub mod similarity;
pub mod sqlite;
pub mod trash;

#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "postgres")]
pub mod postgres_schema;
#[cfg(feature = "postgres")]
pub mod migrate;

use anyhow::Result;
use std::path::Path;

pub use schema::{SCHEMA, MIGRATIONS};
pub use similarity::{PhotoRecord, SimilarityGroup, calculate_quality_score};
pub use embeddings::SearchResult;
pub use faces::{BoundingBox, Face, FaceCluster, FaceWithPhoto, Person};
pub use schedule::{ScheduledTask, ScheduledTaskType, ScheduleStatus};
pub use albums::UserTag;

use crate::config::DatabaseConfig;
#[cfg(feature = "postgres")]
use crate::config::DatabaseType;

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

/// Full metadata for a photo from the database
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PhotoMetadata {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub directory: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub format: Option<String>,
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
    pub modified_at: Option<String>,
    pub scanned_at: Option<String>,
    pub description: Option<String>,
    pub tags: Option<String>,
    pub sha256_hash: Option<String>,
    pub perceptual_hash: Option<String>,
    pub face_count: i64,
    pub people_names: Vec<String>,
}

/// Photo data for export (database-layer struct to avoid circular dependency with export module)
#[derive(Debug, Clone)]
pub struct ExportedPhotoRow {
    pub path: String,
    pub filename: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub file_size: Option<u64>,
    pub sha256: Option<String>,
    pub perceptual_hash: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_taken: Option<String>,
    pub description: Option<String>,
    pub scanned_at: Option<String>,
}

/// Macro to dispatch a method call to the active backend variant.
macro_rules! dispatch {
    // No arguments beyond self
    ($self:expr, $method:ident()) => {
        match &$self.inner {
            DatabaseInner::Sqlite(db) => db.$method(),
            #[cfg(feature = "postgres")]
            DatabaseInner::Postgres(db) => db.$method(),
        }
    };
    // With arguments
    ($self:expr, $method:ident($($arg:expr),+ $(,)?)) => {
        match &$self.inner {
            DatabaseInner::Sqlite(db) => db.$method($($arg),+),
            #[cfg(feature = "postgres")]
            DatabaseInner::Postgres(db) => db.$method($($arg),+),
        }
    };
}

enum DatabaseInner {
    Sqlite(sqlite::SqliteDb),
    #[cfg(feature = "postgres")]
    Postgres(postgres::PgDb),
}

pub struct Database {
    inner: DatabaseInner,
}

impl Database {
    /// Open a database connection based on the provided configuration.
    pub fn open(config: &DatabaseConfig) -> Result<Self> {
        #[cfg(feature = "postgres")]
        {
            if config.backend == DatabaseType::Postgresql {
                let url = config.postgresql_url.as_deref()
                    .ok_or_else(|| anyhow::anyhow!("PostgreSQL URL not configured"))?;
                let pool_size = config.pool_size.unwrap_or(10);
                let pg = postgres::PgDb::open(url, pool_size)?;
                return Ok(Self { inner: DatabaseInner::Postgres(pg) });
            }
        }

        let db = sqlite::SqliteDb::open(&config.sqlite_path)?;
        Ok(Self { inner: DatabaseInner::Sqlite(db) })
    }

    pub fn initialize(&self) -> Result<()> {
        dispatch!(self, initialize())
    }

    // ========================================================================
    // Photo operations
    // ========================================================================

    pub fn save_description(&self, path: &Path, description: &str) -> Result<()> {
        dispatch!(self, save_description(path, description))
    }

    pub fn get_description(&self, path: &Path) -> Result<Option<String>> {
        dispatch!(self, get_description(path))
    }

    pub fn update_photo_path(&self, old_path: &Path, new_path: &Path) -> Result<()> {
        dispatch!(self, update_photo_path(old_path, new_path))
    }

    pub fn get_photos_mtime_in_dir(&self, directory: &str) -> Result<Vec<(String, Option<String>)>> {
        dispatch!(self, get_photos_mtime_in_dir(directory))
    }

    pub fn get_photo_metadata(&self, path: &Path) -> Result<Option<PhotoMetadata>> {
        dispatch!(self, get_photo_metadata(path))
    }

    pub fn semantic_search_by_text(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        dispatch!(self, semantic_search_by_text(query, limit))
    }

    pub fn get_photo_rotation(&self, path: &Path) -> Result<i32> {
        dispatch!(self, get_photo_rotation(path))
    }

    #[allow(dead_code)]
    pub fn set_user_rotation(&self, path: &Path, rotation: i32) -> Result<()> {
        dispatch!(self, set_user_rotation(path, rotation))
    }

    pub fn rotate_photo_cw(&self, path: &Path) -> Result<i32> {
        dispatch!(self, rotate_photo_cw(path))
    }

    pub fn rotate_photo_ccw(&self, path: &Path) -> Result<i32> {
        dispatch!(self, rotate_photo_ccw(path))
    }

    #[allow(dead_code)]
    pub fn reset_photo_rotation(&self, path: &Path) -> Result<()> {
        dispatch!(self, reset_photo_rotation(path))
    }

    // ========================================================================
    // Face operations
    // ========================================================================

    pub fn create_person(&self, name: &str) -> Result<i64> {
        dispatch!(self, create_person(name))
    }

    pub fn find_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        dispatch!(self, find_person_by_name(name))
    }

    pub fn find_or_create_person(&self, name: &str) -> Result<i64> {
        dispatch!(self, find_or_create_person(name))
    }

    pub fn update_person_name(&self, person_id: i64, name: &str) -> Result<()> {
        dispatch!(self, update_person_name(person_id, name))
    }

    pub fn delete_person(&self, person_id: i64) -> Result<()> {
        dispatch!(self, delete_person(person_id))
    }

    pub fn get_all_people(&self) -> Result<Vec<Person>> {
        dispatch!(self, get_all_people())
    }

    pub fn get_person(&self, person_id: i64) -> Result<Option<Person>> {
        dispatch!(self, get_person(person_id))
    }

    pub fn store_face(
        &self,
        photo_id: i64,
        bbox: &BoundingBox,
        embedding: Option<&[f32]>,
        confidence: Option<f32>,
    ) -> Result<i64> {
        dispatch!(self, store_face(photo_id, bbox, embedding, confidence))
    }

    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<Face>> {
        dispatch!(self, get_faces_for_photo(photo_id))
    }

    pub fn get_faces_for_person(&self, person_id: i64) -> Result<Vec<FaceWithPhoto>> {
        dispatch!(self, get_faces_for_person(person_id))
    }

    pub fn assign_face_to_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        dispatch!(self, assign_face_to_person(face_id, person_id))
    }

    pub fn unassign_face(&self, face_id: i64) -> Result<()> {
        dispatch!(self, unassign_face(face_id))
    }

    pub fn get_unassigned_faces(&self) -> Result<Vec<FaceWithPhoto>> {
        dispatch!(self, get_unassigned_faces())
    }

    pub fn get_photos_without_faces_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_faces_in_dir(directory, limit))
    }

    pub fn mark_photo_scanned(&self, photo_id: i64, faces_found: usize) -> Result<()> {
        dispatch!(self, mark_photo_scanned(photo_id, faces_found))
    }

    pub fn count_photos_needing_face_scan(&self) -> Result<i64> {
        dispatch!(self, count_photos_needing_face_scan())
    }

    pub fn count_faces(&self) -> Result<i64> {
        dispatch!(self, count_faces())
    }

    pub fn count_people(&self) -> Result<i64> {
        dispatch!(self, count_people())
    }

    pub fn get_all_face_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        dispatch!(self, get_all_face_embeddings())
    }

    pub fn get_faces_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, i64, BoundingBox)>> {
        dispatch!(self, get_faces_without_embeddings(limit))
    }

    pub fn get_photo_path(&self, photo_id: i64) -> Result<Option<String>> {
        dispatch!(self, get_photo_path(photo_id))
    }

    pub fn update_face_embedding(&self, face_id: i64, embedding: &[f32]) -> Result<()> {
        dispatch!(self, update_face_embedding(face_id, embedding))
    }

    pub fn count_faces_without_embeddings(&self) -> Result<i64> {
        dispatch!(self, count_faces_without_embeddings())
    }

    pub fn create_face_cluster(&self, representative_face_id: Option<i64>, auto_name: &str) -> Result<i64> {
        dispatch!(self, create_face_cluster(representative_face_id, auto_name))
    }

    pub fn add_face_to_cluster(&self, face_id: i64, cluster_id: i64, similarity_score: f32) -> Result<()> {
        dispatch!(self, add_face_to_cluster(face_id, cluster_id, similarity_score))
    }

    pub fn get_all_face_clusters(&self) -> Result<Vec<FaceCluster>> {
        dispatch!(self, get_all_face_clusters())
    }

    pub fn clear_face_clusters(&self) -> Result<()> {
        dispatch!(self, clear_face_clusters())
    }

    pub fn cluster_to_person(&self, cluster_id: i64, person_name: &str) -> Result<i64> {
        dispatch!(self, cluster_to_person(cluster_id, person_name))
    }

    pub fn search_photos_by_person(&self, person_id: i64) -> Result<Vec<(i64, String, String)>> {
        dispatch!(self, search_photos_by_person(person_id))
    }

    // ========================================================================
    // Embedding operations
    // ========================================================================

    pub fn store_embedding(&self, photo_id: i64, embedding: &[f32], model_name: &str) -> Result<()> {
        dispatch!(self, store_embedding(photo_id, embedding, model_name))
    }

    #[allow(dead_code)]
    pub fn get_embedding(&self, photo_id: i64) -> Result<Option<embeddings::EmbeddingRecord>> {
        dispatch!(self, get_embedding(photo_id))
    }

    pub fn get_all_embeddings(&self) -> Result<Vec<embeddings::EmbeddingRecord>> {
        dispatch!(self, get_all_embeddings())
    }

    pub fn semantic_search(&self, query_embedding: &[f32], limit: usize, min_similarity: f32) -> Result<Vec<SearchResult>> {
        dispatch!(self, semantic_search(query_embedding, limit, min_similarity))
    }

    #[allow(dead_code)]
    pub fn get_photos_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_embeddings(limit))
    }

    pub fn get_photos_without_embeddings_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_embeddings_in_dir(directory, limit))
    }

    pub fn count_embeddings(&self) -> Result<i64> {
        dispatch!(self, count_embeddings())
    }

    // ========================================================================
    // Similarity operations
    // ========================================================================

    pub fn find_exact_duplicates(&self) -> Result<Vec<SimilarityGroup>> {
        dispatch!(self, find_exact_duplicates())
    }

    pub fn find_perceptual_duplicates(&self, threshold: u32) -> Result<Vec<SimilarityGroup>> {
        dispatch!(self, find_perceptual_duplicates(threshold))
    }

    pub fn mark_for_deletion(&self, photo_id: i64) -> Result<()> {
        dispatch!(self, mark_for_deletion(photo_id))
    }

    pub fn unmark_for_deletion(&self, photo_id: i64) -> Result<()> {
        dispatch!(self, unmark_for_deletion(photo_id))
    }

    pub fn get_marked_for_deletion(&self) -> Result<Vec<PhotoRecord>> {
        dispatch!(self, get_marked_for_deletion())
    }

    #[allow(dead_code)]
    pub fn delete_marked_photos(&self) -> Result<usize> {
        dispatch!(self, delete_marked_photos())
    }

    pub fn delete_photos_by_ids(&self, ids: &[i64]) -> Result<usize> {
        dispatch!(self, delete_photos_by_ids(ids))
    }

    #[allow(dead_code)]
    pub fn get_photo_count(&self) -> Result<i64> {
        dispatch!(self, get_photo_count())
    }

    // ========================================================================
    // Trash operations
    // ========================================================================

    pub fn mark_trashed(&self, photo_id: i64, trash_path: &Path) -> Result<()> {
        dispatch!(self, mark_trashed(photo_id, trash_path))
    }

    pub fn get_trashed_photos(&self) -> Result<Vec<trash::TrashedPhoto>> {
        dispatch!(self, get_trashed_photos())
    }

    pub fn restore_photo(&self, photo_id: i64) -> Result<String> {
        dispatch!(self, restore_photo(photo_id))
    }

    pub fn delete_trashed_photo(&self, photo_id: i64) -> Result<()> {
        dispatch!(self, delete_trashed_photo(photo_id))
    }

    pub fn get_marked_not_trashed(&self) -> Result<Vec<PhotoRecord>> {
        dispatch!(self, get_marked_not_trashed())
    }

    pub fn get_old_trashed_photos(&self, max_age_days: u32) -> Result<Vec<trash::TrashedPhoto>> {
        dispatch!(self, get_old_trashed_photos(max_age_days))
    }

    pub fn get_trash_total_size(&self) -> Result<u64> {
        dispatch!(self, get_trash_total_size())
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
        dispatch!(self, create_scheduled_task(task_type, target_path, photo_ids, scheduled_at, hours_start, hours_end))
    }

    pub fn get_pending_schedules(&self) -> Result<Vec<ScheduledTask>> {
        dispatch!(self, get_pending_schedules())
    }

    pub fn get_overdue_schedules(&self, now: &str) -> Result<Vec<ScheduledTask>> {
        dispatch!(self, get_overdue_schedules(now))
    }

    #[allow(dead_code)]
    pub fn get_all_schedules(&self) -> Result<Vec<ScheduledTask>> {
        dispatch!(self, get_all_schedules())
    }

    pub fn update_schedule_status(&self, id: i64, status: ScheduleStatus, error_message: Option<&str>) -> Result<()> {
        dispatch!(self, update_schedule_status(id, status, error_message))
    }

    pub fn cancel_schedule(&self, id: i64) -> Result<()> {
        dispatch!(self, cancel_schedule(id))
    }

    #[allow(dead_code)]
    pub fn delete_schedule(&self, id: i64) -> Result<()> {
        dispatch!(self, delete_schedule(id))
    }

    pub fn get_due_pending_tasks(&self, limit: usize) -> Result<Vec<ScheduledTask>> {
        dispatch!(self, get_due_pending_tasks(limit))
    }

    pub fn mark_task_running(&self, task_id: i64) -> Result<()> {
        dispatch!(self, mark_task_running(task_id))
    }

    pub fn mark_task_completed(&self, task_id: i64) -> Result<()> {
        dispatch!(self, mark_task_completed(task_id))
    }

    pub fn mark_task_failed(&self, task_id: i64, error: &str) -> Result<()> {
        dispatch!(self, mark_task_failed(task_id, error))
    }

    // ========================================================================
    // Album operations
    // ========================================================================

    pub fn get_all_tags(&self) -> Result<Vec<UserTag>> {
        dispatch!(self, get_all_tags())
    }

    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<i64> {
        dispatch!(self, create_tag(name, color))
    }

    pub fn get_or_create_tag(&self, name: &str) -> Result<UserTag> {
        dispatch!(self, get_or_create_tag(name))
    }

    #[allow(dead_code)]
    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        dispatch!(self, delete_tag(tag_id))
    }

    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<()> {
        dispatch!(self, rename_tag(tag_id, new_name))
    }

    pub fn get_photo_tags(&self, photo_id: i64) -> Result<Vec<UserTag>> {
        dispatch!(self, get_photo_tags(photo_id))
    }

    pub fn add_tag_to_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        dispatch!(self, add_tag_to_photo(photo_id, tag_id))
    }

    pub fn remove_tag_from_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        dispatch!(self, remove_tag_from_photo(photo_id, tag_id))
    }

    pub fn get_photos_with_tag(&self, tag_id: i64) -> Result<Vec<i64>> {
        dispatch!(self, get_photos_with_tag(tag_id))
    }

    pub fn search_tags(&self, prefix: &str) -> Result<Vec<UserTag>> {
        dispatch!(self, search_tags(prefix))
    }

    pub fn get_all_albums(&self) -> Result<Vec<albums::Album>> {
        dispatch!(self, get_all_albums())
    }

    pub fn create_album(&self, name: &str, description: Option<&str>, is_smart: bool) -> Result<i64> {
        dispatch!(self, create_album(name, description, is_smart))
    }

    pub fn delete_album(&self, album_id: i64) -> Result<()> {
        dispatch!(self, delete_album(album_id))
    }

    pub fn add_photo_to_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        dispatch!(self, add_photo_to_album(album_id, photo_id))
    }

    pub fn remove_photo_from_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        dispatch!(self, remove_photo_from_album(album_id, photo_id))
    }

    pub fn get_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        dispatch!(self, get_album_photos(album_id))
    }

    pub fn get_album_photo_paths(&self, album_id: i64) -> Result<Vec<String>> {
        dispatch!(self, get_album_photo_paths(album_id))
    }

    pub fn set_album_filter_tags(&self, album_id: i64, tag_ids: &[i64]) -> Result<()> {
        dispatch!(self, set_album_filter_tags(album_id, tag_ids))
    }

    pub fn get_smart_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        dispatch!(self, get_smart_album_photos(album_id))
    }

    // ========================================================================
    // LLM queue operations
    // ========================================================================

    pub fn save_llm_result(&self, photo_id: i64, description: &str, tags_json: &str) -> Result<()> {
        dispatch!(self, save_llm_result(photo_id, description, tags_json))
    }

    #[allow(dead_code)]
    pub fn get_photos_without_description(&self) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_description())
    }

    pub fn get_photos_without_description_in_dir(&self, directory: &Path) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_description_in_dir(directory))
    }

    #[allow(dead_code)]
    pub fn get_photo_description(&self, photo_id: i64) -> Result<Option<String>> {
        dispatch!(self, get_photo_description(photo_id))
    }

    // ========================================================================
    // Scanner operations
    // ========================================================================

    pub fn photo_exists(&self, path: &Path) -> Result<bool> {
        dispatch!(self, photo_exists(path))
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
        dispatch!(self, insert_scanned_photo(
            path, filename, directory, size_bytes, modified_at,
            width, height, format,
            camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
            gps_lat, gps_lon, all_exif,
            md5_hash, sha256_hash, perceptual_hash,
            exif_orientation
        ))
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
        dispatch!(self, update_scanned_photo(
            path, filename, directory, size_bytes, modified_at,
            width, height, format,
            camera_make, camera_model, lens, focal_length, aperture, shutter_speed, iso, taken_at,
            gps_lat, gps_lon, all_exif,
            md5_hash, sha256_hash, perceptual_hash,
            exif_orientation
        ))
    }

    // ========================================================================
    // Export operations
    // ========================================================================

    pub fn get_photos_for_export(&self) -> Result<Vec<ExportedPhotoRow>> {
        dispatch!(self, get_photos_for_export())
    }

    // ========================================================================
    // Daemon operations
    // ========================================================================

    pub fn photo_exists_by_path(&self, path: &str) -> bool {
        match &self.inner {
            DatabaseInner::Sqlite(db) => db.photo_exists_by_path(path),
            #[cfg(feature = "postgres")]
            DatabaseInner::Postgres(db) => db.photo_exists_by_path(path),
        }
    }

    pub fn insert_basic_photo(&self, path: &str, filename: &str, directory: &str, size: i64) -> Result<()> {
        dispatch!(self, insert_basic_photo(path, filename, directory, size))
    }

    pub fn get_photos_without_description_in_directory(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        dispatch!(self, get_photos_without_description_in_directory(directory, limit))
    }

    pub fn save_photo_description_by_id(&self, photo_id: i64, description: &str) -> Result<()> {
        dispatch!(self, save_photo_description_by_id(photo_id, description))
    }

    pub fn count_photos_without_faces_in_dir(&self, directory: &str) -> Result<i64> {
        dispatch!(self, count_photos_without_faces_in_dir(directory))
    }

    // ========================================================================
    // Directory prompt operations
    // ========================================================================

    pub fn get_directory_prompt(&self, directory: &str) -> Result<Option<String>> {
        dispatch!(self, get_directory_prompt(directory))
    }

    pub fn set_directory_prompt(&self, directory: &str, prompt: &str) -> Result<()> {
        dispatch!(self, set_directory_prompt(directory, prompt))
    }
}
