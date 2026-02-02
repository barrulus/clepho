//! Database backend abstraction for multiple database support.
//!
//! This module provides a trait-based abstraction layer that allows
//! the application to work with different database backends (SQLite, PostgreSQL).
//!
//! Currently reserved for future multi-database support.

#![allow(dead_code)]

use anyhow::Result;
use std::path::Path;

use super::{
    BoundingBox, FaceWithPhoto, Person, PhotoMetadata, SearchResult, ScheduledTask,
    ScheduleStatus, ScheduledTaskType,
};
use super::albums::{Album, UserTag};
use super::similarity::{PhotoRecord, SimilarityGroup};

/// Trait for database backend implementations.
/// This provides a common interface for SQLite and PostgreSQL backends.
pub trait DatabaseBackend: Send + Sync {
    // === Connection Management ===

    /// Initialize the database schema and run migrations
    fn initialize(&self) -> Result<()>;

    // === Photo Operations ===

    /// Get full photo metadata by path
    fn get_photo_metadata(&self, path: &Path) -> Result<Option<PhotoMetadata>>;

    /// Save LLM description for a photo
    fn save_description(&self, path: &Path, description: &str) -> Result<()>;

    /// Get LLM description for a photo
    fn get_description(&self, path: &Path) -> Result<Option<String>>;

    /// Update photo path after moving a file
    fn update_photo_path(&self, old_path: &Path, new_path: &Path) -> Result<()>;

    /// Get photos with their modified_at timestamps for a specific directory
    fn get_photos_mtime_in_dir(&self, directory: &str) -> Result<Vec<(String, Option<String>)>>;

    /// Get effective rotation for a photo (EXIF + user rotation)
    fn get_photo_rotation(&self, path: &Path) -> Result<i32>;

    /// Set user rotation for a photo
    fn set_user_rotation(&self, path: &Path, rotation: i32) -> Result<()>;

    /// Rotate photo clockwise by 90 degrees
    fn rotate_photo_cw(&self, path: &Path) -> Result<i32>;

    /// Rotate photo counter-clockwise by 90 degrees
    fn rotate_photo_ccw(&self, path: &Path) -> Result<i32>;

    /// Reset user rotation to 0
    fn reset_photo_rotation(&self, path: &Path) -> Result<()>;

    /// Simple text-based search on descriptions
    fn semantic_search_by_text(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>>;

    // === Face Operations ===

    /// Create a new person
    fn create_person(&self, name: &str) -> Result<i64>;

    /// Find person by name
    fn find_person_by_name(&self, name: &str) -> Result<Option<Person>>;

    /// Find or create person by name
    fn find_or_create_person(&self, name: &str) -> Result<i64>;

    /// Get all people
    fn get_all_people(&self) -> Result<Vec<Person>>;

    /// Get person by ID
    fn get_person(&self, person_id: i64) -> Result<Option<Person>>;

    /// Update person name
    fn update_person_name(&self, person_id: i64, new_name: &str) -> Result<()>;

    /// Delete person
    fn delete_person(&self, person_id: i64) -> Result<()>;

    /// Store a detected face
    fn store_face(
        &self,
        photo_id: i64,
        bbox: &BoundingBox,
        embedding: Option<&[f32]>,
        confidence: f32,
    ) -> Result<i64>;

    /// Get faces for a photo
    fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<FaceWithPhoto>>;

    /// Get faces for a person
    fn get_faces_for_person(&self, person_id: i64) -> Result<Vec<FaceWithPhoto>>;

    /// Assign a face to a person
    fn assign_face_to_person(&self, face_id: i64, person_id: i64) -> Result<()>;

    /// Unassign a face from a person
    fn unassign_face(&self, face_id: i64) -> Result<()>;

    /// Get unassigned faces
    fn get_unassigned_faces(&self, limit: usize) -> Result<Vec<FaceWithPhoto>>;

    /// Get all face embeddings
    fn get_all_face_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>>;

    /// Count photos needing face scan in a directory
    fn count_photos_needing_face_scan(&self, directory: &str) -> Result<usize>;

    /// Get photo path by ID
    fn get_photo_path(&self, photo_id: i64) -> Result<Option<String>>;

    /// Search photos by person name
    fn search_photos_by_person(&self, person_name: &str) -> Result<Vec<String>>;

    // === Embedding Operations ===

    /// Store CLIP embedding for a photo
    fn store_embedding(&self, photo_id: i64, embedding: &[f32], model_name: &str) -> Result<()>;

    /// Semantic search using embeddings
    fn semantic_search(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;

    /// Get photos without embeddings in a directory
    fn get_photos_without_embeddings_in_dir(&self, directory: &str) -> Result<Vec<(i64, String)>>;

    /// Count embeddings
    fn count_embeddings(&self) -> Result<usize>;

    // === Similarity/Duplicate Operations ===

    /// Find exact duplicates by SHA256 hash
    fn find_exact_duplicates(&self) -> Result<Vec<SimilarityGroup>>;

    /// Find perceptual duplicates within threshold
    fn find_perceptual_duplicates(&self, threshold: u32) -> Result<Vec<SimilarityGroup>>;

    /// Mark photo for deletion
    fn mark_for_deletion(&self, photo_id: i64) -> Result<()>;

    /// Unmark photo for deletion
    fn unmark_for_deletion(&self, photo_id: i64) -> Result<()>;

    /// Get photos marked for deletion
    fn get_marked_for_deletion(&self) -> Result<Vec<PhotoRecord>>;

    /// Delete marked photos from database
    fn delete_marked_photos(&self) -> Result<usize>;

    /// Get total photo count
    fn get_photo_count(&self) -> Result<usize>;

    // === Tag Operations ===

    /// Get all user tags
    fn get_all_tags(&self) -> Result<Vec<UserTag>>;

    /// Create a new tag
    fn create_tag(&self, name: &str, color: &str) -> Result<i64>;

    /// Delete a tag
    fn delete_tag(&self, tag_id: i64) -> Result<()>;

    /// Get tags for a photo
    fn get_photo_tags(&self, photo_id: i64) -> Result<Vec<UserTag>>;

    /// Add tag to photo
    fn add_tag_to_photo(&self, photo_id: i64, tag_id: i64) -> Result<()>;

    /// Remove tag from photo
    fn remove_tag_from_photo(&self, photo_id: i64, tag_id: i64) -> Result<()>;

    // === Album Operations ===

    /// Get all albums
    fn get_all_albums(&self) -> Result<Vec<Album>>;

    /// Create an album
    fn create_album(&self, name: &str, description: Option<&str>) -> Result<i64>;

    /// Delete an album
    fn delete_album(&self, album_id: i64) -> Result<()>;

    /// Add photo to album
    fn add_photo_to_album(&self, album_id: i64, photo_id: i64) -> Result<()>;

    /// Get album photo paths
    fn get_album_photo_paths(&self, album_id: i64) -> Result<Vec<String>>;

    // === Trash Operations ===

    /// Mark a photo as trashed
    fn mark_trashed(&self, path: &Path, original_path: &Path) -> Result<()>;

    /// Get all trashed photos
    fn get_trashed_photos(&self) -> Result<Vec<PhotoRecord>>;

    /// Restore a photo from trash
    fn restore_photo(&self, photo_id: i64) -> Result<Option<String>>;

    /// Permanently delete a trashed photo
    fn delete_trashed_photo(&self, photo_id: i64) -> Result<()>;

    /// Get old trashed photos for cleanup
    fn get_old_trashed_photos(&self, max_age_days: u32) -> Result<Vec<PhotoRecord>>;

    /// Get total size of trash
    fn get_trash_total_size(&self) -> Result<u64>;

    // === Schedule Operations ===

    /// Create a scheduled task
    fn create_scheduled_task(
        &self,
        task_type: ScheduledTaskType,
        target_path: &str,
        photo_ids: Option<&[i64]>,
        scheduled_at: &str,
        hours_start: Option<u8>,
        hours_end: Option<u8>,
    ) -> Result<i64>;

    /// Get pending schedules
    fn get_pending_schedules(&self) -> Result<Vec<ScheduledTask>>;

    /// Get overdue schedules
    fn get_overdue_schedules(&self) -> Result<Vec<ScheduledTask>>;

    /// Update schedule status
    fn update_schedule_status(&self, task_id: i64, status: ScheduleStatus) -> Result<()>;

    /// Cancel a schedule
    fn cancel_schedule(&self, task_id: i64) -> Result<()>;
}
