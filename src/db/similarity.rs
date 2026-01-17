use anyhow::Result;
use rusqlite::params;

use super::Database;
use crate::scanner::hashing::hamming_distance;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct PhotoRecord {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub size_bytes: i64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sha256_hash: Option<String>,
    pub perceptual_hash: Option<String>,
    pub taken_at: Option<String>,
    pub marked_for_deletion: bool,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SimilarityGroup {
    pub id: i64,
    pub group_type: String,
    pub photos: Vec<PhotoRecord>,
}

impl Database {
    pub fn find_exact_duplicates(&self) -> Result<Vec<SimilarityGroup>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT sha256_hash, COUNT(*) as cnt
            FROM photos
            WHERE sha256_hash IS NOT NULL
            GROUP BY sha256_hash
            HAVING cnt > 1
            "#,
        )?;

        let duplicate_hashes: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut groups = Vec::new();

        for hash in duplicate_hashes {
            let photos = self.get_photos_by_sha256(&hash)?;
            if photos.len() > 1 {
                groups.push(SimilarityGroup {
                    id: 0, // Will be assigned when saved
                    group_type: "exact".to_string(),
                    photos,
                });
            }
        }

        Ok(groups)
    }

    pub fn find_perceptual_duplicates(&self, threshold: u32) -> Result<Vec<SimilarityGroup>> {
        // Get all photos with perceptual hashes
        let photos = self.get_all_photos_with_phash()?;

        // Simple O(n²) comparison - for large collections, consider using LSH
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
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE sha256_hash = ?
            ORDER BY taken_at, path
            "#,
        )?;

        let photos = stmt
            .query_map([sha256], |row| {
                Ok(PhotoRecord {
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

    fn get_all_photos_with_phash(&self) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE perceptual_hash IS NOT NULL
            ORDER BY path
            "#,
        )?;

        let photos = stmt
            .query_map([], |row| {
                Ok(PhotoRecord {
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

    pub fn mark_for_deletion(&self, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET marked_for_deletion = 1 WHERE id = ?",
            params![photo_id],
        )?;
        Ok(())
    }

    pub fn unmark_for_deletion(&self, photo_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE photos SET marked_for_deletion = 0 WHERE id = ?",
            params![photo_id],
        )?;
        Ok(())
    }

    pub fn get_marked_for_deletion(&self) -> Result<Vec<PhotoRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, filename, size_bytes, width, height,
                   sha256_hash, perceptual_hash, taken_at, marked_for_deletion
            FROM photos
            WHERE marked_for_deletion = 1
            ORDER BY path
            "#,
        )?;

        let photos = stmt
            .query_map([], |row| {
                Ok(PhotoRecord {
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

    pub fn delete_marked_photos(&self) -> Result<usize> {
        let count = self.conn.execute(
            "DELETE FROM photos WHERE marked_for_deletion = 1",
            [],
        )?;
        Ok(count)
    }

    #[allow(dead_code)]
    pub fn get_photo_count(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM photos",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

pub fn calculate_quality_score(photo: &PhotoRecord) -> i32 {
    let mut score = 0;

    // Prefer larger dimensions
    if let (Some(w), Some(h)) = (photo.width, photo.height) {
        score += (w * h / 10000) as i32; // Points per 10000 pixels
    }

    // Prefer larger file sizes (usually means less compression)
    score += (photo.size_bytes / 100000) as i32; // Points per 100KB

    // Prefer photos with EXIF date (means original, not copied)
    if photo.taken_at.is_some() {
        score += 10;
    }

    score
}
