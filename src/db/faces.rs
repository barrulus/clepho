//! Database functions for face detection and people management.
//! Face detection feature is in development; many items are reserved for future use.

#![allow(dead_code)]

use anyhow::Result;
use rusqlite::params;

use super::Database;

/// Bounding box for a detected face
#[derive(Debug, Clone)]
pub struct BoundingBox {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// A detected face in a photo
#[derive(Debug, Clone)]
pub struct Face {
    pub id: i64,
    pub photo_id: i64,
    pub bbox: BoundingBox,
    pub embedding: Option<Vec<f32>>,
    pub person_id: Option<i64>,
    pub confidence: Option<f32>,
}

/// A person (named face cluster)
#[derive(Debug, Clone)]
pub struct Person {
    pub id: i64,
    pub name: String,
    pub face_count: i64,
}

/// A face cluster (ungrouped faces)
#[derive(Debug, Clone)]
pub struct FaceCluster {
    pub id: i64,
    pub auto_name: String,
    pub representative_face_id: Option<i64>,
    pub face_count: i64,
}

/// Face with associated photo path for display
#[derive(Debug, Clone)]
pub struct FaceWithPhoto {
    pub face: Face,
    pub photo_path: String,
    pub photo_filename: String,
}

impl Database {
    // ========================================================================
    // People management
    // ========================================================================

    /// Create a new person
    pub fn create_person(&self, name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO people (name) VALUES (?)",
            params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Find a person by name (case-insensitive)
    pub fn find_person_by_name(&self, name: &str) -> Result<Option<Person>> {
        let result = self.conn.query_row(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE LOWER(p.name) = LOWER(?)
            GROUP BY p.id
            "#,
            [name],
            |row| {
                Ok(Person {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    face_count: row.get(2)?,
                })
            },
        );

        match result {
            Ok(person) => Ok(Some(person)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Find an existing person by name, or create a new one
    pub fn find_or_create_person(&self, name: &str) -> Result<i64> {
        if let Some(person) = self.find_person_by_name(name)? {
            Ok(person.id)
        } else {
            self.create_person(name)
        }
    }

    /// Update a person's name
    pub fn update_person_name(&self, person_id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE people SET name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
            params![name, person_id],
        )?;
        Ok(())
    }

    /// Delete a person (faces will have person_id set to NULL)
    pub fn delete_person(&self, person_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM people WHERE id = ?",
            params![person_id],
        )?;
        Ok(())
    }

    /// Get all people with face counts
    pub fn get_all_people(&self) -> Result<Vec<Person>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            GROUP BY p.id
            ORDER BY p.name
            "#,
        )?;

        let people = stmt
            .query_map([], |row| {
                Ok(Person {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    face_count: row.get(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(people)
    }

    /// Get a person by ID
    pub fn get_person(&self, person_id: i64) -> Result<Option<Person>> {
        let result = self.conn.query_row(
            r#"
            SELECT p.id, p.name, COUNT(f.id) as face_count
            FROM people p
            LEFT JOIN faces f ON f.person_id = p.id
            WHERE p.id = ?
            GROUP BY p.id
            "#,
            [person_id],
            |row| {
                Ok(Person {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    face_count: row.get(2)?,
                })
            },
        );

        match result {
            Ok(person) => Ok(Some(person)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // ========================================================================
    // Face management
    // ========================================================================

    /// Store a detected face
    pub fn store_face(
        &self,
        photo_id: i64,
        bbox: &BoundingBox,
        embedding: Option<&[f32]>,
        confidence: Option<f32>,
    ) -> Result<i64> {
        let embedding_bytes = embedding.map(embedding_to_bytes);
        let embedding_dim = embedding.map(|e| e.len() as i32);

        self.conn.execute(
            r#"
            INSERT INTO faces (photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, embedding_dim, confidence)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                photo_id,
                bbox.x,
                bbox.y,
                bbox.width,
                bbox.height,
                embedding_bytes,
                embedding_dim,
                confidence,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get all faces for a photo
    pub fn get_faces_for_photo(&self, photo_id: i64) -> Result<Vec<Face>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h, embedding, person_id, confidence
            FROM faces
            WHERE photo_id = ?
            "#,
        )?;

        let faces = stmt
            .query_map([photo_id], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(Face {
                    id: row.get(0)?,
                    photo_id: row.get(1)?,
                    bbox: BoundingBox {
                        x: row.get(2)?,
                        y: row.get(3)?,
                        width: row.get(4)?,
                        height: row.get(5)?,
                    },
                    embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
                    person_id: row.get(7)?,
                    confidence: row.get(8)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(faces)
    }

    /// Get all faces for a person
    pub fn get_faces_for_person(&self, person_id: i64) -> Result<Vec<FaceWithPhoto>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id = ?
            ORDER BY p.taken_at DESC
            "#,
        )?;

        let faces = stmt
            .query_map([person_id], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(FaceWithPhoto {
                    face: Face {
                        id: row.get(0)?,
                        photo_id: row.get(1)?,
                        bbox: BoundingBox {
                            x: row.get(2)?,
                            y: row.get(3)?,
                            width: row.get(4)?,
                            height: row.get(5)?,
                        },
                        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
                        person_id: row.get(7)?,
                        confidence: row.get(8)?,
                    },
                    photo_path: row.get(9)?,
                    photo_filename: row.get(10)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(faces)
    }

    /// Assign a face to a person
    pub fn assign_face_to_person(&self, face_id: i64, person_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE faces SET person_id = ? WHERE id = ?",
            params![person_id, face_id],
        )?;
        Ok(())
    }

    /// Unassign a face from a person
    pub fn unassign_face(&self, face_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE faces SET person_id = NULL WHERE id = ?",
            params![face_id],
        )?;
        Ok(())
    }

    /// Get all unassigned faces (not belonging to any person)
    pub fn get_unassigned_faces(&self) -> Result<Vec<FaceWithPhoto>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.id, f.photo_id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h,
                   f.embedding, f.person_id, f.confidence, p.path, p.filename
            FROM faces f
            JOIN photos p ON f.photo_id = p.id
            WHERE f.person_id IS NULL
            ORDER BY p.taken_at DESC
            "#,
        )?;

        let faces = stmt
            .query_map([], |row| {
                let embedding_bytes: Option<Vec<u8>> = row.get(6)?;
                Ok(FaceWithPhoto {
                    face: Face {
                        id: row.get(0)?,
                        photo_id: row.get(1)?,
                        bbox: BoundingBox {
                            x: row.get(2)?,
                            y: row.get(3)?,
                            width: row.get(4)?,
                            height: row.get(5)?,
                        },
                        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
                        person_id: row.get(7)?,
                        confidence: row.get(8)?,
                    },
                    photo_path: row.get(9)?,
                    photo_filename: row.get(10)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(faces)
    }

    /// Get photos that haven't been scanned for faces in a specific directory (and subdirectories)
    pub fn get_photos_without_faces_in_dir(&self, directory: &str, limit: usize) -> Result<Vec<(i64, String)>> {
        // Use LIKE with directory prefix to match subdirectories
        let dir_pattern = if directory.ends_with('/') {
            format!("{}%", directory)
        } else {
            format!("{}/%", directory)
        };

        let mut stmt = self.conn.prepare(
            r#"
            SELECT p.id, p.path
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
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

    /// Mark a photo as scanned for faces
    pub fn mark_photo_scanned(&self, photo_id: i64, faces_found: usize) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO face_scans (photo_id, faces_found, scanned_at) VALUES (?, ?, CURRENT_TIMESTAMP)",
            params![photo_id, faces_found as i64],
        )?;
        Ok(())
    }

    /// Get count of photos that need face scanning
    pub fn count_photos_needing_face_scan(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            r#"
            SELECT COUNT(*)
            FROM photos p
            LEFT JOIN face_scans fs ON p.id = fs.photo_id
            WHERE fs.photo_id IS NULL
            "#,
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count total faces
    pub fn count_faces(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM faces",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Count total people
    pub fn count_people(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM people",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// Get all faces with embeddings for clustering
    pub fn get_all_face_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, embedding FROM faces WHERE embedding IS NOT NULL",
        )?;

        let results = stmt
            .query_map([], |row| {
                let bytes: Vec<u8> = row.get(1)?;
                Ok((row.get(0)?, bytes_to_embedding(&bytes)))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get faces that don't have embeddings yet (for on-demand generation)
    pub fn get_faces_without_embeddings(&self, limit: usize) -> Result<Vec<(i64, i64, BoundingBox)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, photo_id, bbox_x, bbox_y, bbox_w, bbox_h
            FROM faces
            WHERE embedding IS NULL
            LIMIT ?
            "#,
        )?;

        let results = stmt
            .query_map([limit as i64], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    BoundingBox {
                        x: row.get(2)?,
                        y: row.get(3)?,
                        width: row.get(4)?,
                        height: row.get(5)?,
                    },
                ))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    /// Get photo path by ID
    pub fn get_photo_path(&self, photo_id: i64) -> Result<Option<String>> {
        let result = self.conn.query_row(
            "SELECT path FROM photos WHERE id = ?",
            [photo_id],
            |row| row.get(0),
        );

        match result {
            Ok(path) => Ok(Some(path)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update a face's embedding
    pub fn update_face_embedding(&self, face_id: i64, embedding: &[f32]) -> Result<()> {
        let embedding_bytes = embedding_to_bytes(embedding);
        let embedding_dim = embedding.len() as i32;

        self.conn.execute(
            "UPDATE faces SET embedding = ?, embedding_dim = ? WHERE id = ?",
            params![embedding_bytes, embedding_dim, face_id],
        )?;
        Ok(())
    }

    /// Count faces without embeddings
    pub fn count_faces_without_embeddings(&self) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM faces WHERE embedding IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    // ========================================================================
    // Face clusters (for auto-grouping before naming)
    // ========================================================================

    /// Create a face cluster
    pub fn create_face_cluster(&self, representative_face_id: Option<i64>, auto_name: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO face_clusters (representative_face_id, auto_name) VALUES (?, ?)",
            params![representative_face_id, auto_name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Add a face to a cluster
    pub fn add_face_to_cluster(&self, face_id: i64, cluster_id: i64, similarity_score: f32) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT OR REPLACE INTO face_cluster_members (face_id, cluster_id, similarity_score)
            VALUES (?, ?, ?)
            "#,
            params![face_id, cluster_id, similarity_score],
        )?;
        Ok(())
    }

    /// Get all face clusters with counts
    pub fn get_all_face_clusters(&self) -> Result<Vec<FaceCluster>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT fc.id, fc.auto_name, fc.representative_face_id, COUNT(fcm.face_id) as face_count
            FROM face_clusters fc
            LEFT JOIN face_cluster_members fcm ON fc.id = fcm.cluster_id
            GROUP BY fc.id
            ORDER BY face_count DESC
            "#,
        )?;

        let clusters = stmt
            .query_map([], |row| {
                Ok(FaceCluster {
                    id: row.get(0)?,
                    auto_name: row.get(1)?,
                    representative_face_id: row.get(2)?,
                    face_count: row.get(3)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(clusters)
    }

    /// Clear all face clusters (for re-clustering)
    pub fn clear_face_clusters(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            DELETE FROM face_cluster_members;
            DELETE FROM face_clusters;
            "#,
        )?;
        Ok(())
    }

    /// Convert a cluster to a person
    pub fn cluster_to_person(&self, cluster_id: i64, person_name: &str) -> Result<i64> {
        // Create the person
        let person_id = self.create_person(person_name)?;

        // Assign all faces in the cluster to this person
        self.conn.execute(
            r#"
            UPDATE faces SET person_id = ?
            WHERE id IN (SELECT face_id FROM face_cluster_members WHERE cluster_id = ?)
            "#,
            params![person_id, cluster_id],
        )?;

        // Remove the cluster
        self.conn.execute(
            "DELETE FROM face_cluster_members WHERE cluster_id = ?",
            params![cluster_id],
        )?;
        self.conn.execute(
            "DELETE FROM face_clusters WHERE id = ?",
            params![cluster_id],
        )?;

        Ok(person_id)
    }

    /// Search for photos by person
    pub fn search_photos_by_person(&self, person_id: i64) -> Result<Vec<(i64, String, String)>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT DISTINCT p.id, p.path, p.filename
            FROM photos p
            JOIN faces f ON p.id = f.photo_id
            WHERE f.person_id = ?
            ORDER BY p.taken_at DESC
            "#,
        )?;

        let results = stmt
            .query_map([person_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }
}

// ============================================================================
// Helper functions
// ============================================================================

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
