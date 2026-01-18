mod schema;
pub mod embeddings;
pub mod faces;
pub mod schedule;
pub mod similarity;
pub mod trash;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub use schema::SCHEMA;
pub use similarity::{PhotoRecord, SimilarityGroup, calculate_quality_score};
pub use embeddings::SearchResult;
pub use faces::{BoundingBox, FaceWithPhoto, Person};
pub use schedule::{ScheduledTask, ScheduledTaskType, ScheduleStatus};

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
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Save LLM description for a photo by path
    pub fn save_description(&self, path: &std::path::Path, description: &str) -> Result<()> {
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
}
