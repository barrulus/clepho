use anyhow::Result;

use crate::db::Database;

/// Result of face clustering
#[derive(Debug, Clone)]
pub struct FaceClusteringResult {
    /// Number of clusters created
    pub clusters_created: usize,
    /// Number of faces clustered
    pub faces_clustered: usize,
    /// Number of faces that couldn't be clustered (no embedding)
    pub faces_skipped: usize,
}

/// Cluster faces based on embedding similarity
///
/// This function groups similar faces together based on their embeddings.
/// Faces without embeddings are skipped.
///
/// The clustering algorithm uses a simple greedy approach:
/// 1. Pick an unclustered face as a new cluster center
/// 2. Find all faces within the similarity threshold
/// 3. Add them to the cluster
/// 4. Repeat until all faces are processed
pub fn cluster_faces(
    db: &Database,
    similarity_threshold: f32,
) -> Result<FaceClusteringResult> {
    // Clear existing clusters
    db.clear_face_clusters()?;

    // Get all face embeddings
    let face_embeddings = db.get_all_face_embeddings()?;

    if face_embeddings.is_empty() {
        return Ok(FaceClusteringResult {
            clusters_created: 0,
            faces_clustered: 0,
            faces_skipped: 0,
        });
    }

    let mut clustered: Vec<bool> = vec![false; face_embeddings.len()];
    let mut clusters_created = 0;
    let mut faces_clustered = 0;

    for i in 0..face_embeddings.len() {
        if clustered[i] {
            continue;
        }

        // Create a new cluster with this face as the representative
        let (face_id, ref embedding) = face_embeddings[i];
        let auto_name = format!("Person {}", clusters_created + 1);
        let cluster_id = db.create_face_cluster(Some(face_id), &auto_name)?;

        // Add this face to the cluster
        db.add_face_to_cluster(face_id, cluster_id, 1.0)?;
        clustered[i] = true;
        faces_clustered += 1;

        // Find all similar faces
        for j in (i + 1)..face_embeddings.len() {
            if clustered[j] {
                continue;
            }

            let (other_face_id, ref other_embedding) = face_embeddings[j];
            let similarity = cosine_similarity(embedding, other_embedding);

            if similarity >= similarity_threshold {
                db.add_face_to_cluster(other_face_id, cluster_id, similarity)?;
                clustered[j] = true;
                faces_clustered += 1;
            }
        }

        clusters_created += 1;
    }

    // Count faces without embeddings
    let total_faces = db.count_faces()? as usize;
    let faces_skipped = total_faces.saturating_sub(faces_clustered);

    Ok(FaceClusteringResult {
        clusters_created,
        faces_clustered,
        faces_skipped,
    })
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
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

/// Merge multiple clusters into one
pub fn merge_clusters(
    db: &Database,
    cluster_ids: &[i64],
    new_name: Option<&str>,
) -> Result<i64> {
    if cluster_ids.is_empty() {
        anyhow::bail!("No clusters to merge");
    }

    // Create a new person from the first cluster
    let name = new_name.unwrap_or("Merged Person");
    let person_id = db.cluster_to_person(cluster_ids[0], name)?;

    // Merge remaining clusters into this person
    for &cluster_id in &cluster_ids[1..] {
        // Get all faces in this cluster and assign to person
        let faces = db.get_unassigned_faces()?;
        for face in faces {
            db.assign_face_to_person(face.face.id, person_id)?;
        }
    }

    Ok(person_id)
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
}
