//! Change detection for files in a directory.
//!
//! Detects new and modified files by comparing filesystem state against
//! the database records.

use anyhow::Result;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::db::Database;

/// Tolerance in seconds for timestamp comparison.
/// This accounts for filesystem timestamp granularity (some filesystems
/// only have 1-2 second precision) and potential rounding during storage.
const TIMESTAMP_TOLERANCE_SECS: i64 = 2;

/// Result of detecting changes in a directory.
#[derive(Debug, Clone, Default)]
pub struct ChangeDetectionResult {
    /// Files that exist on disk but not in the database.
    pub new_files: Vec<PathBuf>,
    /// Files that have a newer mtime on disk than in the database.
    pub modified_files: Vec<PathBuf>,
}

impl ChangeDetectionResult {
    /// Check if there are any changes detected.
    pub fn has_changes(&self) -> bool {
        !self.new_files.is_empty() || !self.modified_files.is_empty()
    }

    /// Total number of changes (new + modified).
    pub fn total_count(&self) -> usize {
        self.new_files.len() + self.modified_files.len()
    }
}

/// Detect changes in a directory by comparing filesystem with database.
///
/// This performs a non-recursive check of the specified directory only.
pub fn detect_changes(
    directory: &PathBuf,
    db: &Database,
    extensions: &[String],
) -> Result<ChangeDetectionResult> {
    let mut result = ChangeDetectionResult::default();

    // Get database records for this directory
    let dir_str = directory.to_string_lossy().to_string();
    let db_records = db.get_photos_mtime_in_dir(&dir_str)?;

    // Build a map of path -> mtime from database
    let db_map: HashMap<String, Option<String>> = db_records.into_iter().collect();

    // Read directory entries
    let entries = match std::fs::read_dir(directory) {
        Ok(e) => e,
        Err(_) => return Ok(result), // Directory doesn't exist or not readable
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Check if file has a valid image extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !extensions.iter().any(|e| e.to_lowercase() == ext) {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();

        if let Some(db_mtime) = db_map.get(&path_str) {
            // File exists in database, check if modified
            if let Ok(metadata) = std::fs::metadata(&path) {
                if let Ok(fs_mtime) = metadata.modified() {
                    let fs_datetime: DateTime<Utc> = fs_mtime.into();

                    // Compare filesystem mtime with database mtime using tolerance
                    if let Some(ref db_mtime_str) = db_mtime {
                        // Parse the database timestamp
                        if let Ok(db_datetime) = parse_db_timestamp(db_mtime_str) {
                            // Check if filesystem time is significantly newer than DB time
                            let diff = fs_datetime.timestamp() - db_datetime.timestamp();
                            if diff > TIMESTAMP_TOLERANCE_SECS {
                                result.modified_files.push(path);
                            }
                        } else {
                            // Can't parse DB timestamp, consider it modified
                            result.modified_files.push(path);
                        }
                    } else {
                        // DB has no mtime recorded, consider it modified
                        result.modified_files.push(path);
                    }
                }
            }
        } else {
            // File doesn't exist in database - it's new
            result.new_files.push(path);
        }
    }

    // Sort results for consistent display
    result.new_files.sort();
    result.modified_files.sort();

    Ok(result)
}

/// Parse a database timestamp string into a DateTime.
/// Handles the ISO 8601 format used by the scanner: "YYYY-MM-DDTHH:MM:SS"
fn parse_db_timestamp(timestamp: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    // Try parsing with the format we store (no timezone suffix)
    chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S")
        .map(|dt| dt.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_change_detection_result() {
        let result = ChangeDetectionResult {
            new_files: vec![PathBuf::from("/test/a.jpg")],
            modified_files: vec![PathBuf::from("/test/b.jpg"), PathBuf::from("/test/c.jpg")],
        };

        assert!(result.has_changes());
        assert_eq!(result.total_count(), 3);
    }

    #[test]
    fn test_empty_result() {
        let result = ChangeDetectionResult::default();
        assert!(!result.has_changes());
        assert_eq!(result.total_count(), 0);
    }
}
