mod schema;
pub mod similarity;

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

pub use schema::SCHEMA;
pub use similarity::{PhotoRecord, SimilarityGroup, calculate_quality_score};

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
}
