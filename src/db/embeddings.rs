//! Embedding storage and similarity search functionality.

use anyhow::Result;
use rusqlite::params;

use super::Database;

/// Embedding record from the database
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EmbeddingRecord {
    pub photo_id: i64,
    pub embedding: Vec<f32>,
    pub model_name: String,
}

/// Search result with similarity score
#[derive(Debug, Clone)]
pub struct SearchResult {
    #[allow(dead_code)]
    pub photo_id: i64,
    pub path: String,
    pub filename: String,
    pub similarity: f32,
    pub description: Option<String>,
}

impl Database {
    /// Store an embedding for a photo
    pub fn store_embedding(
        &self,
        photo_id: i64,
        embedding: &[f32],
        model_name: &str,
    ) -> Result<()> {
        // Convert f32 array to bytes
        let bytes = embedding_to_bytes(embedding);

        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO embeddings (photo_id, embedding, embedding_dim, model_name, created_at)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            params![photo_id, bytes, embedding.len() as i32, model_name],
        )?;

        Ok(())
    }

    /// Get embedding for a photo
    #[allow(dead_code)]
    pub fn get_embedding(&self, photo_id: i64) -> Result<Option<EmbeddingRecord>> {
        let result = self.conn.query_row(
            "SELECT photo_id, embedding, model_name FROM embeddings WHERE photo_id = ?",
            [photo_id],
            |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok(EmbeddingRecord {
                    photo_id: row.get(0)?,
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2)?,
                })
            },
        );

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get all embeddings (for search)
    pub fn get_all_embeddings(&self) -> Result<Vec<EmbeddingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT photo_id, embedding, model_name FROM embeddings",
        )?;

        let records = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok(EmbeddingRecord {
                    photo_id: row.get(0)?,
                    embedding: bytes_to_embedding(&bytes),
                    model_name: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Search for photos by semantic similarity to a query embedding
    pub fn semantic_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<SearchResult>> {
        // Get all embeddings
        let embeddings = self.get_all_embeddings()?;

        // Calculate similarities
        let mut results: Vec<(i64, f32)> = embeddings
            .iter()
            .map(|record| {
                let similarity = cosine_similarity(query_embedding, &record.embedding);
                (record.photo_id, similarity)
            })
            .filter(|(_, sim)| *sim >= min_similarity)
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top results
        let top_results: Vec<(i64, f32)> = results.into_iter().take(limit).collect();

        // Fetch photo details for results
        let mut search_results = Vec::new();
        for (photo_id, similarity) in top_results {
            if let Ok(Some(result)) = self.get_photo_for_search(photo_id, similarity) {
                search_results.push(result);
            }
        }

        Ok(search_results)
    }

    /// Get photo details for search result
    fn get_photo_for_search(&self, photo_id: i64, similarity: f32) -> Result<Option<SearchResult>> {
        let result = self.conn.query_row(
            "SELECT id, path, filename, description FROM photos WHERE id = ?",
            [photo_id],
            |row| {
                Ok(SearchResult {
                    photo_id: row.get(0)?,
                    path: row.get(1)?,
                    filename: row.get(2)?,
                    similarity,
                    description: row.get(3)?,
                })
            },
        );

        match result {
            Ok(r) => Ok(Some(r)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get photos without embeddings for batch processing
    #[allow(dead_code)]
    pub fn get_photos_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
            LIMIT ?
            "#,
        )?;

        let results = stmt
            .query_map([limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get photos without embeddings in a specific directory (and subdirectories)
    pub fn get_photos_without_embeddings_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN embeddings e ON p.id = e.photo_id
            WHERE e.photo_id IS NULL
              AND p.path LIKE ?
            LIMIT ?
            "#,
        )?;

        let results = stmt
            .query_map(params![dir_pattern, limit as i64], |row| Ok((row.get(0)?, row.get(1)?)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Count photos with embeddings
    pub fn count_embeddings(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM embeddings",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

/// Convert f32 slice to bytes for storage
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Convert bytes back to f32 vector
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Calculate cosine similarity between two vectors
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.0001);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 0.0001);

        let d = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &d) - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_embedding_conversion() {
        let original = vec![1.5, -2.3, 0.0, 100.0];
        let bytes = embedding_to_bytes(&original);
        let recovered = bytes_to_embedding(&bytes);
        assert_eq!(original, recovered);
    }
}
