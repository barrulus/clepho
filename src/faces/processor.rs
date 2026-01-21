use anyhow::Result;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::db::Database;
use crate::tasks::{TaskUpdate, TaskProgress};
use super::detector;

/// Face processor that detects and stores faces using dlib
pub struct FaceProcessor {
    _initialized: bool,
}

impl FaceProcessor {
    /// Create a new face processor
    /// Note: Models are loaded lazily on first detection
    pub fn new() -> Self {
        Self { _initialized: false }
    }

    /// Initialize face detection model only (fast - no embedding model)
    /// Embedding model will be loaded on-demand when clustering is requested
    pub fn init_models(&mut self) -> Result<()> {
        detector::init_detection_model()?;
        self._initialized = true;
        Ok(())
    }

    /// Process a single image and detect faces (fast mode - no embeddings)
    /// Embeddings will be generated later when clustering is requested
    pub fn process_image(&self, db: &Database, photo_id: i64, image_path: &Path) -> Result<usize> {
        // Detect faces using fast mode (no embeddings)
        let detected_faces = detector::detect_faces_fast(image_path)?;

        let mut faces_added = 0;

        for face in detected_faces {
            // Store face without embedding (will be generated on-demand for clustering)
            let embedding = if face.embedding.is_empty() {
                None
            } else {
                Some(face.embedding.as_slice())
            };

            db.store_face(
                photo_id,
                &face.bbox,
                embedding,
                Some(face.confidence),
            )?;
            faces_added += 1;
        }

        Ok(faces_added)
    }

    /// Process multiple photos in batch with cancellation support via TaskUpdate protocol.
    pub fn process_batch_cancellable(
        &mut self,
        db: &Database,
        photos: &[(i64, String)],
        tx: mpsc::Sender<TaskUpdate>,
        cancel_flag: Arc<AtomicBool>,
    ) {
        let total = photos.len();

        let _ = tx.send(TaskUpdate::Started { total });

        // Initialize models if not already done
        if !self._initialized {
            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(0, total).with_message("Loading face detection models...")
            ));
            if let Err(e) = self.init_models() {
                let _ = tx.send(TaskUpdate::Failed {
                    error: format!("Failed to initialize face models: {}", e),
                });
                return;
            }
        }

        let mut total_faces = 0;
        let mut photos_processed = 0;

        for (idx, (photo_id, path)) in photos.iter().enumerate() {
            // Check for cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            let filename = Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());

            let _ = tx.send(TaskUpdate::Progress(
                TaskProgress::new(idx + 1, total).with_item(&filename)
            ));

            let image_path = Path::new(path);
            if !image_path.exists() {
                continue;
            }

            match self.process_image(db, *photo_id, image_path) {
                Ok(count) => {
                    // Mark photo as scanned (even if 0 faces found)
                    let _ = db.mark_photo_scanned(*photo_id, count);
                    total_faces += count;
                    photos_processed += 1;
                }
                Err(e) => {
                    // Log error but continue processing
                    tracing::error!(path = %path, error = %e, "Face detection error");
                }
            }
        }

        let _ = tx.send(TaskUpdate::Completed {
            message: format!("{} photos, {} faces found", photos_processed, total_faces),
        });
    }
}

impl Default for FaceProcessor {
    fn default() -> Self {
        Self::new()
    }
}
