//! Database functions for user tags and albums.
//! Most album functionality is reserved for future implementation.

#![allow(dead_code)]

use anyhow::Result;
use rusqlite::params;

use super::Database;

/// A user-defined tag
#[derive(Debug, Clone)]
pub struct UserTag {
    pub id: i64,
    pub name: String,
    #[allow(dead_code)]
    pub color: String,
}

/// An album (collection of photos)
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub cover_photo_id: Option<i64>,
    pub is_smart: bool,
    pub filter_tags: Vec<i64>,
    pub photo_count: i64,
}

impl Database {
    // ========== Tag Management ==========

    /// Get all user tags
    pub fn get_all_tags(&self) -> Result<Vec<UserTag>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color FROM user_tags ORDER BY name"
        )?;

        let tags = stmt
            .query_map([], |row| {
                Ok(UserTag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    /// Create a new tag
    pub fn create_tag(&self, name: &str, color: Option<&str>) -> Result<i64> {
        let color = color.unwrap_or("#808080");
        self.conn.execute(
            "INSERT INTO user_tags (name, color) VALUES (?, ?)",
            params![name, color],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Get or create a tag by name
    pub fn get_or_create_tag(&self, name: &str) -> Result<UserTag> {
        // Try to find existing
        let existing = self.conn.query_row(
            "SELECT id, name, color FROM user_tags WHERE name = ? COLLATE NOCASE",
            [name],
            |row| {
                Ok(UserTag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            },
        );

        match existing {
            Ok(tag) => Ok(tag),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let id = self.create_tag(name, None)?;
                Ok(UserTag {
                    id,
                    name: name.to_string(),
                    color: "#808080".to_string(),
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Delete a tag
    #[allow(dead_code)]
    pub fn delete_tag(&self, tag_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM user_tags WHERE id = ?", [tag_id])?;
        Ok(())
    }

    /// Rename a tag
    pub fn rename_tag(&self, tag_id: i64, new_name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE user_tags SET name = ? WHERE id = ?",
            params![new_name, tag_id],
        )?;
        Ok(())
    }

    /// Get tags for a photo
    pub fn get_photo_tags(&self, photo_id: i64) -> Result<Vec<UserTag>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT t.id, t.name, t.color
            FROM user_tags t
            JOIN photo_user_tags pt ON pt.tag_id = t.id
            WHERE pt.photo_id = ?
            ORDER BY t.name
            "#
        )?;

        let tags = stmt
            .query_map([photo_id], |row| {
                Ok(UserTag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    /// Add a tag to a photo
    pub fn add_tag_to_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO photo_user_tags (photo_id, tag_id) VALUES (?, ?)",
            params![photo_id, tag_id],
        )?;
        Ok(())
    }

    /// Remove a tag from a photo
    pub fn remove_tag_from_photo(&self, photo_id: i64, tag_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM photo_user_tags WHERE photo_id = ? AND tag_id = ?",
            params![photo_id, tag_id],
        )?;
        Ok(())
    }

    /// Get photos with a specific tag
    pub fn get_photos_with_tag(&self, tag_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id FROM photo_user_tags WHERE tag_id = ?"
        )?;

        let ids = stmt
            .query_map([tag_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    /// Search tags by prefix (for autocomplete)
    pub fn search_tags(&self, prefix: &str) -> Result<Vec<UserTag>> {
        let pattern = format!("{}%", prefix);
        let mut stmt = self.conn.prepare(
            "SELECT id, name, color FROM user_tags WHERE name LIKE ? COLLATE NOCASE ORDER BY name LIMIT 10"
        )?;

        let tags = stmt
            .query_map([pattern], |row| {
                Ok(UserTag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(tags)
    }

    // ========== Album Management ==========

    /// Get all albums
    pub fn get_all_albums(&self) -> Result<Vec<Album>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT a.id, a.name, a.description, a.cover_photo_id, a.is_smart, a.filter_tags,
                   (SELECT COUNT(*) FROM album_photos WHERE album_id = a.id) as photo_count
            FROM albums a
            ORDER BY a.name
            "#
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

    /// Create a new album
    pub fn create_album(&self, name: &str, description: Option<&str>, is_smart: bool) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO albums (name, description, is_smart) VALUES (?, ?, ?)",
            params![name, description, if is_smart { 1 } else { 0 }],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete an album
    pub fn delete_album(&self, album_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM albums WHERE id = ?", [album_id])?;
        Ok(())
    }

    /// Add photo to album
    pub fn add_photo_to_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO album_photos (album_id, photo_id) VALUES (?, ?)",
            params![album_id, photo_id],
        )?;
        Ok(())
    }

    /// Remove photo from album
    pub fn remove_photo_from_album(&self, album_id: i64, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM album_photos WHERE album_id = ? AND photo_id = ?",
            params![album_id, photo_id],
        )?;
        Ok(())
    }

    /// Get photos in an album
    pub fn get_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id FROM album_photos WHERE album_id = ? ORDER BY position, added_at"
        )?;

        let ids = stmt
            .query_map([album_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    /// Get album photo paths for display
    pub fn get_album_photo_paths(&self, album_id: i64) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.path
            FROM photos p
            JOIN album_photos ap ON ap.photo_id = p.id
            WHERE ap.album_id = ?
            ORDER BY ap.position, ap.added_at
            "#
        )?;

        let paths = stmt
            .query_map([album_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(paths)
    }

    /// Set album filter tags (for smart albums)
    pub fn set_album_filter_tags(&self, album_id: i64, tag_ids: &[i64]) -> Result<()> {
        let json = serde_json::to_string(tag_ids)?;
        self.conn.execute(
            "UPDATE albums SET filter_tags = ?, is_smart = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            params![json, album_id],
        )?;
        Ok(())
    }

    /// Get smart album photos (based on tag filters)
    pub fn get_smart_album_photos(&self, album_id: i64) -> Result<Vec<i64>> {
        // Get the filter tags
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

        // Find photos that have ALL the specified tags
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

        // Build params dynamically
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
}
