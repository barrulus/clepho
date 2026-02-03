//! Types for trash management.

#[derive(Debug, Clone)]
pub struct TrashedPhoto {
    pub id: i64,
    pub path: String,            // Current path in trash
    pub original_path: String,   // Path before trashing
    pub filename: String,
    pub trashed_at: String,
    pub size_bytes: i64,
}
