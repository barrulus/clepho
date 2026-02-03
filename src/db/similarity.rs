//! Types for duplicate detection and similarity grouping.

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

/// Compute hamming distance between two perceptual hashes (base64-encoded).
pub fn hamming_distance(hash1: &str, hash2: &str) -> anyhow::Result<u32> {
    use img_hash::ImageHash;

    let h1 = ImageHash::<Box<[u8]>>::from_base64(hash1)
        .map_err(|e| anyhow::anyhow!("Invalid hash1: {:?}", e))?;
    let h2 = ImageHash::<Box<[u8]>>::from_base64(hash2)
        .map_err(|e| anyhow::anyhow!("Invalid hash2: {:?}", e))?;

    Ok(h1.dist(&h2))
}

pub fn calculate_quality_score(photo: &PhotoRecord) -> i32 {
    let mut score = 0;

    // Prefer larger dimensions
    if let (Some(w), Some(h)) = (photo.width, photo.height) {
        score += (w * h / 10000) as i32;
    }

    // Prefer larger file sizes (usually means less compression)
    score += (photo.size_bytes / 100000) as i32;

    // Prefer photos with EXIF date (means original, not copied)
    if photo.taken_at.is_some() {
        score += 10;
    }

    score
}
