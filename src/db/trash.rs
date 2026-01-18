use anyhow::Result;
use rusqlite::params;
use std::path::Path;

use super::Database;

#[derive(Debug, Clone)]
pub struct TrashedPhoto {
    pub id: i64,
    pub path: String,            // Current path in trash
    pub original_path: String,   // Path before trashing
    pub filename: String,
    pub trashed_at: String,
    pub size_bytes: i64,
}

impl Database {
    /// Mark photo as trashed with new path
    pub fn mark_trashed(&self, photo_id: i64, trash_path: &Path) -> Result<()> {
        // First get the current path to store as original_path
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
            params![trash_path_str, original_path, now, photo_id],
        )?;

        Ok(())
    }

    /// Get all trashed photos
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

    /// Restore photo (update path back to original)
    pub fn restore_photo(&self, photo_id: i64) -> Result<String> {
        // Get original path
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

    /// Permanently remove trashed photo record from database
    pub fn delete_trashed_photo(&self, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM photos WHERE id = ?",
            [photo_id],
        )?;
        Ok(())
    }

    /// Get photos that are both marked for deletion and not yet trashed
    pub fn get_marked_not_trashed(&self) -> Result<Vec<super::similarity::PhotoRecord>> {
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
                Ok(super::similarity::PhotoRecord {
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

    /// Clean up old trashed photos based on age (days)
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

    /// Get total size of trashed photos
    pub fn get_trash_total_size(&self) -> Result<u64> {
        let size: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(size_bytes), 0) FROM photos WHERE trashed_at IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(size as u64)
    }
}
