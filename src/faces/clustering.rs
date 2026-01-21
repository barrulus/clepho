use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use crate::db::Database;
use crate::tasks::{TaskProgress, TaskUpdate};
use super::detector;

/// Result of face clustering
#[derive(Debug, Clone)]
pub struct FaceClusteringResult {
    /// Number of clusters created
    pub clusters_created: usize,
    /// Number of faces clustered
    pub faces_clustered: usize,
    /// Number of faces that couldn't be clustered (no embedding)
    pub faces_skipped: usize,
    /// Number of embeddings generated on-demand
    pub embeddings_generated: usize,
    /// Number of embeddings that failed to generate
    pub embeddings_failed: usize,
    /// Last error message if any
    pub last_error: Option<String>,
    /// Total faces found in database (for debugging)
    pub total_faces_in_db: usize,
    /// Faces needing embeddings (for debugging)
    pub faces_needing_embeddings: usize,
}

/// Result of embedding generation
#[derive(Debug, Clone)]
pub struct EmbeddingGenerationResult {
    pub generated: usize,
    pub failed: usize,
    pub last_error: Option<String>,
}

/// Generate embeddings for faces that don't have them yet
/// Returns the number of embeddings generated and failed
pub fn generate_missing_embeddings(db: &Database) -> Result<EmbeddingGenerationResult> {
    // Ensure embedding model is loaded
    detector::ensure_embedding_model()?;

    let mut generated = 0;
    let mut failed = 0;
    let mut last_error: Option<String> = None;

    // Get ALL faces without embeddings (not in a loop to avoid infinite loop on failures)
    let faces_without_embeddings = db.get_faces_without_embeddings(10000)?;

    for (face_id, photo_id, bbox) in faces_without_embeddings {
        // Get photo path
        let photo_path = match db.get_photo_path(photo_id)? {
            Some(path) => path,
            None => {
                failed += 1;
                last_error = Some(format!("Photo {} not found in database", photo_id));
                continue;
            }
        };

        let path = Path::new(&photo_path);
        if !path.exists() {
            failed += 1;
            last_error = Some(format!("File not found: {}", photo_path));
            continue;
        }

        // Generate embedding
        match detector::generate_embedding_for_face(path, &bbox) {
            Ok(embedding) => {
                db.update_face_embedding(face_id, &embedding)?;
                generated += 1;
            }
            Err(e) => {
                failed += 1;
                last_error = Some(format!("{}", e));
            }
        }
    }

    Ok(EmbeddingGenerationResult {
        generated,
        failed,
        last_error,
    })
}

/// Cluster faces based on embedding similarity
///
/// This function groups similar faces together based on their embeddings.
/// If faces don't have embeddings, they will be generated on-demand.
///
/// The clustering algorithm uses a simple greedy approach:
/// 1. Generate embeddings for any faces that don't have them
/// 2. Pick an unclustered face as a new cluster center
/// 3. Find all faces within the similarity threshold
/// 4. Add them to the cluster
/// 5. Repeat until all faces are processed
pub fn cluster_faces(
    db: &Database,
    similarity_threshold: f32,
) -> Result<FaceClusteringResult> {
    // Clear existing clusters
    db.clear_face_clusters()?;

    // Check total faces and faces without embeddings
    let total_faces_in_db = db.count_faces()?;
    let faces_needing_embeddings = db.count_faces_without_embeddings()?;

    // First, generate embeddings for any faces that don't have them
    let (embeddings_generated, embeddings_failed, last_error) = if faces_needing_embeddings > 0 {
        let result = generate_missing_embeddings(db)?;
        (result.generated, result.failed, result.last_error)
    } else {
        (0, 0, None)
    };

    // Get all face embeddings
    let face_embeddings = db.get_all_face_embeddings()?;

    if face_embeddings.is_empty() {
        return Ok(FaceClusteringResult {
            clusters_created: 0,
            faces_clustered: 0,
            faces_skipped: 0,
            embeddings_generated,
            embeddings_failed,
            last_error,
            total_faces_in_db: total_faces_in_db as usize,
            faces_needing_embeddings: faces_needing_embeddings as usize,
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

    // Count faces without embeddings (those that failed to generate)
    let total_faces = db.count_faces()? as usize;
    let faces_skipped = total_faces.saturating_sub(faces_clustered);

    Ok(FaceClusteringResult {
        clusters_created,
        faces_clustered,
        faces_skipped,
        embeddings_generated,
        embeddings_failed,
        last_error,
        total_faces_in_db: total_faces_in_db as usize,
        faces_needing_embeddings: faces_needing_embeddings as usize,
    })
}

/// Cluster faces in a background task with progress reporting and cancellation support
pub fn cluster_faces_background(
    db: &Database,
    similarity_threshold: f32,
    tx: Sender<TaskUpdate>,
    cancel_flag: Arc<AtomicBool>,
) {
    // Clear existing clusters
    if let Err(e) = db.clear_face_clusters() {
        let _ = tx.send(TaskUpdate::Failed {
            error: format!("Failed to clear clusters: {}", e),
        });
        return;
    }

    // Check total faces and faces without embeddings
    let (total_faces_in_db, faces_needing_embeddings) = match (db.count_faces(), db.count_faces_without_embeddings()) {
        (Ok(total), Ok(needing)) => (total as usize, needing as usize),
        (Err(e), _) | (_, Err(e)) => {
            let _ = tx.send(TaskUpdate::Failed {
                error: format!("Failed to count faces: {}", e),
            });
            return;
        }
    };

    // Send initial progress
    let _ = tx.send(TaskUpdate::Started {
        total: total_faces_in_db + faces_needing_embeddings
    });

    let mut embeddings_generated = 0;
    let mut embeddings_failed = 0;

    // Generate embeddings for faces that don't have them
    if faces_needing_embeddings > 0 {
        if let Err(e) = detector::ensure_embedding_model() {
            let _ = tx.send(TaskUpdate::Failed {
                error: format!("Failed to load embedding model: {}", e),
            });
            return;
        }

        let faces_without_embeddings = match db.get_faces_without_embeddings(10000) {
            Ok(faces) => faces,
            Err(e) => {
                let _ = tx.send(TaskUpdate::Failed {
                    error: format!("Failed to get faces: {}", e),
                });
                return;
            }
        };

        for (idx, (face_id, photo_id, bbox)) in faces_without_embeddings.iter().enumerate() {
            // Check for cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            // Send progress
            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(idx, faces_needing_embeddings)
                    .with_message(format!("Generating embedding {}/{}", idx + 1, faces_needing_embeddings))
            ));

            let photo_path = match db.get_photo_path(*photo_id) {
                Ok(Some(path)) => path,
                _ => {
                    embeddings_failed += 1;
                    continue;
                }
            };

            let path = Path::new(&photo_path);
            if !path.exists() {
                embeddings_failed += 1;
                continue;
            }

            match detector::generate_embedding_for_face(path, bbox) {
                Ok(embedding) => {
                    if db.update_face_embedding(*face_id, &embedding).is_ok() {
                        embeddings_generated += 1;
                    } else {
                        embeddings_failed += 1;
                    }
                }
                Err(_) => {
                    embeddings_failed += 1;
                }
            }
        }
    }

    // Check for cancellation before clustering
    if cancel_flag.load(Ordering::SeqCst) {
        let _ = tx.send(TaskUpdate::Cancelled);
        return;
    }

    // Get all face embeddings
    let face_embeddings = match db.get_all_face_embeddings() {
        Ok(embeddings) => embeddings,
        Err(e) => {
            let _ = tx.send(TaskUpdate::Failed {
                error: format!("Failed to get embeddings: {}", e),
            });
            return;
        }
    };

    if face_embeddings.is_empty() {
        let _ = tx.send(TaskUpdate::Completed {
            message: format!(
                "No faces to cluster ({} embeddings generated, {} failed)",
                embeddings_generated, embeddings_failed
            ),
        });
        return;
    }

    let total_faces = face_embeddings.len();
    let _ = tx.send(TaskUpdate::Progress(
        TaskProgress::new(0, total_faces)
            .with_message("Starting clustering...")
    ));

    let mut clustered: Vec<bool> = vec![false; total_faces];
    let mut clusters_created = 0;
    let mut faces_clustered = 0;

    for i in 0..total_faces {
        // Check for cancellation periodically
        if cancel_flag.load(Ordering::SeqCst) {
            let _ = tx.send(TaskUpdate::Cancelled);
            return;
        }

        if clustered[i] {
            continue;
        }

        // Send progress
        let _ = tx.send(TaskUpdate::Progress(
            TaskProgress::new(faces_clustered, total_faces)
                .with_message(format!("Clustering faces... ({} clusters)", clusters_created))
        ));

        let (face_id, ref embedding) = face_embeddings[i];
        let auto_name = format!("Person {}", clusters_created + 1);

        let cluster_id = match db.create_face_cluster(Some(face_id), &auto_name) {
            Ok(id) => id,
            Err(e) => {
                let _ = tx.send(TaskUpdate::Failed {
                    error: format!("Failed to create cluster: {}", e),
                });
                return;
            }
        };

        if db.add_face_to_cluster(face_id, cluster_id, 1.0).is_err() {
            continue;
        }
        clustered[i] = true;
        faces_clustered += 1;

        // Find all similar faces
        for j in (i + 1)..total_faces {
            if clustered[j] {
                continue;
            }

            let (other_face_id, ref other_embedding) = face_embeddings[j];
            let similarity = cosine_similarity(embedding, other_embedding);

            if similarity >= similarity_threshold {
                if db.add_face_to_cluster(other_face_id, cluster_id, similarity).is_ok() {
                    clustered[j] = true;
                    faces_clustered += 1;
                }
            }
        }

        clusters_created += 1;
    }

    let faces_skipped = total_faces.saturating_sub(faces_clustered);
    let mut msg = format!(
        "Created {} clusters from {} faces",
        clusters_created, faces_clustered
    );
    if embeddings_generated > 0 {
        msg.push_str(&format!(" ({} embeddings generated)", embeddings_generated));
    }
    if embeddings_failed > 0 {
        msg.push_str(&format!(" ({} failed)", embeddings_failed));
    }
    if faces_skipped > 0 {
        msg.push_str(&format!(" ({} skipped)", faces_skipped));
    }

    let _ = tx.send(TaskUpdate::Completed { message: msg });
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
    for &_cluster_id in &cluster_ids[1..] {
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
