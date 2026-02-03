//! Database types for face detection and people management.

#![allow(dead_code)]

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

// ============================================================================
// Helper functions
// ============================================================================

/// Convert f32 slice to bytes for storage
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &val in embedding {
        bytes.extend_from_slice(&val.to_le_bytes());
    }
    bytes
}

/// Convert bytes back to f32 vector
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap();
            f32::from_le_bytes(arr)
        })
        .collect()
}
