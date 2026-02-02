//! File centralization - organize photos into a managed library location.
//!
//! Organizes files into a Year/Month hierarchy with descriptive filenames:
//! ```text
//! /Library/
//! ├── 2024/
//! │   └── 03/
//! │       └── 20240315-0930_birthday_emma-tom_cake-cutting_001.jpg
//! └── unknown/
//!     └── {NO_CAT}_old-photo-scan_001.jpg
//! ```

use anyhow::{Context, Result};
use chrono::{Datelike, NaiveDateTime, Timelike};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::CentraliseOperation;
use crate::db::{Database, PhotoMetadata};

/// Marker for uncategorized content
const NO_CAT: &str = "{NO_CAT}";

/// Result of a centralise operation
#[derive(Debug, Clone)]
pub struct CentraliseResult {
    /// Files successfully processed
    pub succeeded: Vec<FileOperation>,
    /// Files that failed
    pub failed: Vec<(PathBuf, String)>,
    /// Files skipped (already in library, etc.)
    pub skipped: Vec<(PathBuf, String)>,
}

/// A single file operation
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileOperation {
    /// Original file path
    pub source: PathBuf,
    /// Destination path in library
    pub destination: PathBuf,
    /// Whether this was a copy (true) or move (false)
    pub was_copy: bool,
}

/// Preview of what a centralise operation would do
#[derive(Debug, Clone)]
pub struct CentralisePreview {
    /// Planned file operations
    pub operations: Vec<PlannedOperation>,
    /// Files that would be skipped
    pub skipped: Vec<(PathBuf, String)>,
    /// Total bytes to be processed
    pub total_bytes: u64,
}

/// A planned file operation (for dry-run preview)
#[derive(Debug, Clone)]
pub struct PlannedOperation {
    /// Original file path
    pub source: PathBuf,
    /// Proposed destination path
    pub destination: PathBuf,
    /// File size in bytes (reserved for UI display)
    #[allow(dead_code)]
    pub size_bytes: u64,
    /// Generated filename components for display (reserved for UI display)
    #[allow(dead_code)]
    pub filename_parts: FilenameParts,
}

/// Components that make up a generated filename
#[derive(Debug, Clone, Default)]
pub struct FilenameParts {
    pub date: Option<String>,
    pub time: Option<String>,
    pub event: Option<String>,
    pub people: Option<String>,
    pub description: Option<String>,
    pub original_name: String,
    pub count: u32,
    pub extension: String,
}

impl FilenameParts {
    /// Generate the full filename from parts
    pub fn to_filename(&self, max_length: usize) -> String {
        let mut parts = Vec::new();

        // Date-time prefix
        if let (Some(date), Some(time)) = (&self.date, &self.time) {
            parts.push(format!("{}-{}", date, time));
        } else if let Some(date) = &self.date {
            parts.push(date.clone());
        }

        // Event
        if let Some(event) = &self.event {
            if !event.is_empty() {
                parts.push(event.clone());
            }
        }

        // People
        if let Some(people) = &self.people {
            if !people.is_empty() {
                parts.push(people.clone());
            }
        }

        // Description
        if let Some(desc) = &self.description {
            if !desc.is_empty() {
                parts.push(desc.clone());
            }
        }

        // If no metadata, use NO_CAT marker with original name
        if parts.is_empty() || (parts.len() == 1 && self.date.is_none()) {
            parts.clear();
            parts.push(format!("{}", NO_CAT));
            parts.push(sanitize_filename(&self.original_name));
        }

        // Add count
        parts.push(format!("{:03}", self.count));

        let mut filename = parts.join("_");

        // Truncate if too long (leaving room for extension)
        let ext_len = self.extension.len() + 1; // +1 for the dot
        if filename.len() > max_length.saturating_sub(ext_len) {
            filename = filename[..max_length.saturating_sub(ext_len)].to_string();
            // Clean up any trailing underscore or hyphen
            filename = filename.trim_end_matches(|c| c == '_' || c == '-').to_string();
        }

        format!("{}.{}", filename, self.extension)
    }

    /// Check if this file has the NO_CAT marker
    #[allow(dead_code)]
    pub fn is_uncategorized(&self) -> bool {
        self.date.is_none() && self.event.is_none() && self.description.is_none()
    }
}

/// Sanitize a string for use in filenames
fn sanitize_filename(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            ' ' | '_' | '-' => '-',
            _ => '-',
        })
        .collect::<String>()
        // Collapse multiple hyphens
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Extract event/category from description or tags
fn extract_event(metadata: &PhotoMetadata) -> Option<String> {
    // Try to extract from tags first
    if let Some(ref tags) = metadata.tags {
        let tags_lower = tags.to_lowercase();
        // Look for common event keywords
        for keyword in ["birthday", "wedding", "vacation", "holiday", "christmas", "easter",
                        "graduation", "party", "concert", "trip", "travel", "family"] {
            if tags_lower.contains(keyword) {
                return Some(keyword.to_string());
            }
        }
    }

    // Try to extract from description
    if let Some(ref desc) = metadata.description {
        let desc_lower = desc.to_lowercase();
        for keyword in ["birthday", "wedding", "vacation", "holiday", "christmas", "easter",
                        "graduation", "party", "concert", "trip", "travel", "family"] {
            if desc_lower.contains(keyword) {
                return Some(keyword.to_string());
            }
        }
    }

    None
}

/// Extract a brief description from the full description
fn extract_brief_description(metadata: &PhotoMetadata, max_words: usize) -> Option<String> {
    let desc = metadata.description.as_ref()?;

    // Take first sentence or first few words
    let first_sentence: String = desc
        .split('.')
        .next()
        .unwrap_or(desc)
        .trim()
        .to_string();

    let words: Vec<&str> = first_sentence.split_whitespace().take(max_words).collect();
    if words.is_empty() {
        return None;
    }

    Some(sanitize_filename(&words.join(" ")))
}

/// Generate filename parts from photo metadata
pub fn generate_filename_parts(metadata: &PhotoMetadata, existing_count: u32) -> FilenameParts {
    let mut parts = FilenameParts::default();

    // Original filename (without extension)
    let original_path = Path::new(&metadata.filename);
    parts.original_name = original_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| metadata.filename.clone());

    parts.extension = original_path
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_else(|| "jpg".to_string());

    // Parse date/time from taken_at
    if let Some(ref taken_at) = metadata.taken_at {
        if let Ok(dt) = NaiveDateTime::parse_from_str(taken_at, "%Y-%m-%d %H:%M:%S") {
            parts.date = Some(format!("{:04}{:02}{:02}", dt.year(), dt.month(), dt.day()));
            parts.time = Some(format!("{:02}{:02}", dt.hour(), dt.minute()));
        } else if let Ok(dt) = NaiveDateTime::parse_from_str(taken_at, "%Y:%m:%d %H:%M:%S") {
            // EXIF format
            parts.date = Some(format!("{:04}{:02}{:02}", dt.year(), dt.month(), dt.day()));
            parts.time = Some(format!("{:02}{:02}", dt.hour(), dt.minute()));
        }
    }

    // Extract event
    parts.event = extract_event(metadata);

    // People from face recognition
    if !metadata.people_names.is_empty() {
        parts.people = Some(
            metadata.people_names
                .iter()
                .map(|n| sanitize_filename(n))
                .collect::<Vec<_>>()
                .join("-")
        );
    }

    // Brief description
    parts.description = extract_brief_description(metadata, 4);

    // Count for uniqueness
    parts.count = existing_count + 1;

    parts
}

/// Determine the destination folder path for a photo
pub fn get_destination_folder(library_root: &Path, metadata: &PhotoMetadata) -> PathBuf {
    // Parse date from taken_at
    if let Some(ref taken_at) = metadata.taken_at {
        if let Ok(dt) = NaiveDateTime::parse_from_str(taken_at, "%Y-%m-%d %H:%M:%S") {
            return library_root
                .join(format!("{:04}", dt.year()))
                .join(format!("{:02}", dt.month()));
        } else if let Ok(dt) = NaiveDateTime::parse_from_str(taken_at, "%Y:%m:%d %H:%M:%S") {
            return library_root
                .join(format!("{:04}", dt.year()))
                .join(format!("{:02}", dt.month()));
        }
    }

    // No date - use "unknown" folder
    library_root.join("unknown")
}

/// Preview what a centralise operation would do (dry-run)
pub fn preview_centralise(
    db: &Database,
    library_root: &Path,
    source_paths: &[PathBuf],
    max_filename_length: usize,
) -> Result<CentralisePreview> {
    let mut operations = Vec::new();
    let mut skipped = Vec::new();
    let mut total_bytes = 0u64;

    // Track destination filenames to handle conflicts
    let mut dest_counts: HashMap<PathBuf, u32> = HashMap::new();

    for source in source_paths {
        // Check if file exists
        if !source.exists() {
            skipped.push((source.clone(), "File not found".to_string()));
            continue;
        }

        // Check if already in library
        if source.starts_with(library_root) {
            skipped.push((source.clone(), "Already in library".to_string()));
            continue;
        }

        // Get metadata from database
        let metadata = match db.get_photo_metadata(source)? {
            Some(m) => m,
            None => {
                skipped.push((source.clone(), "Not in database - scan first".to_string()));
                continue;
            }
        };

        // Determine destination folder
        let dest_folder = get_destination_folder(library_root, &metadata);

        // Generate filename
        let base_dest = dest_folder.clone();
        let count = *dest_counts.get(&base_dest).unwrap_or(&0);
        let filename_parts = generate_filename_parts(&metadata, count);
        let filename = filename_parts.to_filename(max_filename_length);

        let mut destination = dest_folder.join(&filename);

        // Handle conflicts by incrementing count
        let mut conflict_count = count;
        while dest_counts.values().any(|_| destination.exists()) ||
              operations.iter().any(|op: &PlannedOperation| op.destination == destination) {
            conflict_count += 1;
            let mut new_parts = filename_parts.clone();
            new_parts.count = conflict_count + 1;
            let new_filename = new_parts.to_filename(max_filename_length);
            destination = dest_folder.join(&new_filename);
        }

        dest_counts.insert(base_dest, conflict_count);

        // Get file size
        let size_bytes = std::fs::metadata(source)
            .map(|m| m.len())
            .unwrap_or(0);
        total_bytes += size_bytes;

        operations.push(PlannedOperation {
            source: source.clone(),
            destination,
            size_bytes,
            filename_parts,
        });
    }

    Ok(CentralisePreview {
        operations,
        skipped,
        total_bytes,
    })
}

/// Execute the centralise operation
pub fn execute_centralise(
    db: &Database,
    preview: &CentralisePreview,
    operation: CentraliseOperation,
) -> Result<CentraliseResult> {
    let mut result = CentraliseResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
        skipped: preview.skipped.clone(),
    };

    for planned in &preview.operations {
        // Ensure destination directory exists
        if let Some(parent) = planned.destination.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                result.failed.push((
                    planned.source.clone(),
                    format!("Failed to create directory: {}", e),
                ));
                continue;
            }
        }

        // Perform the operation
        let op_result = match operation {
            CentraliseOperation::Copy => {
                std::fs::copy(&planned.source, &planned.destination)
                    .map(|_| ())
                    .context("Copy failed")
            }
            CentraliseOperation::Move => {
                // Try rename first (same filesystem)
                std::fs::rename(&planned.source, &planned.destination)
                    .or_else(|_| {
                        // Fall back to copy + delete for cross-filesystem
                        std::fs::copy(&planned.source, &planned.destination)?;
                        std::fs::remove_file(&planned.source)
                    })
                    .context("Move failed")
            }
        };

        match op_result {
            Ok(()) => {
                // Update database path if moved
                if operation == CentraliseOperation::Move {
                    if let Err(e) = db.update_photo_path(&planned.source, &planned.destination) {
                        // Log but don't fail - file was moved successfully
                        eprintln!("Warning: Failed to update database path: {}", e);
                    }
                }

                result.succeeded.push(FileOperation {
                    source: planned.source.clone(),
                    destination: planned.destination.clone(),
                    was_copy: operation == CentraliseOperation::Copy,
                });
            }
            Err(e) => {
                result.failed.push((planned.source.clone(), e.to_string()));
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("Hello World"), "hello-world");
        assert_eq!(sanitize_filename("Test  Multiple   Spaces"), "test-multiple-spaces");
        assert_eq!(sanitize_filename("Special@#$Characters"), "special-characters");
    }

    #[test]
    fn test_filename_parts_to_filename() {
        let parts = FilenameParts {
            date: Some("20241120".to_string()),
            time: Some("1435".to_string()),
            event: Some("vacation".to_string()),
            people: Some("john-jane".to_string()),
            description: Some("beach-sunset".to_string()),
            original_name: "IMG_1234".to_string(),
            count: 1,
            extension: "jpg".to_string(),
        };

        let filename = parts.to_filename(100);
        assert_eq!(filename, "20241120-1435_vacation_john-jane_beach-sunset_001.jpg");
    }

    #[test]
    fn test_filename_parts_no_metadata() {
        let parts = FilenameParts {
            date: None,
            time: None,
            event: None,
            people: None,
            description: None,
            original_name: "old_photo".to_string(),
            count: 1,
            extension: "jpg".to_string(),
        };

        let filename = parts.to_filename(100);
        assert!(filename.contains(NO_CAT));
        assert!(filename.contains("old-photo"));
    }
}
