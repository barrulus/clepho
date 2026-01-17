use anyhow::Result;
use std::path::Path;
use std::sync::mpsc;

use crate::db::Database;
use super::detector;

/// Status updates during face processing
#[derive(Debug, Clone)]
pub enum FaceProcessingStatus {
    /// Starting face detection
    Starting { total_photos: usize },
    /// Initializing face detection models
    InitializingModels,
    /// Processing a specific photo
    Processing {
        current: usize,
        total: usize,
        path: String,
    },
    /// Found faces in a photo
    FoundFaces {
        path: String,
        count: usize,
    },
    /// Completed processing
    Completed {
        photos_processed: usize,
        faces_found: usize,
    },
    /// Error occurred
    Error { message: String },
}

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

    /// Initialize face detection models (call once at startup for faster first detection)
    pub fn init_models(&mut self) -> Result<()> {
        detector::init_models()?;
        self._initialized = true;
        Ok(())
    }

    /// Process a single image and detect faces using dlib
    pub fn process_image(&self, db: &Database, photo_id: i64, image_path: &Path) -> Result<usize> {
        // Detect faces using dlib (includes embeddings)
        let detected_faces = detector::detect_faces(image_path)?;

        let mut faces_added = 0;

        for face in detected_faces {
            // Store face with embedding
            db.store_face(
                photo_id,
                &face.bbox,
                Some(&face.embedding),
                Some(face.confidence),
            )?;
            faces_added += 1;
        }

        Ok(faces_added)
    }

    /// Process multiple photos in batch
    pub fn process_batch(
        &mut self,
        db: &Database,
        photos: &[(i64, String)],
        status_sender: Option<mpsc::Sender<FaceProcessingStatus>>,
    ) -> Result<(usize, usize)> {
        let total = photos.len();

        if let Some(ref tx) = status_sender {
            let _ = tx.send(FaceProcessingStatus::Starting { total_photos: total });
        }

        // Initialize models if not already done
        if !self._initialized {
            if let Some(ref tx) = status_sender {
                let _ = tx.send(FaceProcessingStatus::InitializingModels);
            }
            self.init_models()?;
        }

        let mut total_faces = 0;
        let mut photos_processed = 0;

        for (idx, (photo_id, path)) in photos.iter().enumerate() {
            if let Some(ref tx) = status_sender {
                let _ = tx.send(FaceProcessingStatus::Processing {
                    current: idx + 1,
                    total,
                    path: path.clone(),
                });
            }

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

                    if count > 0 {
                        if let Some(ref tx) = status_sender {
                            let _ = tx.send(FaceProcessingStatus::FoundFaces {
                                path: path.clone(),
                                count,
                            });
                        }
                    }
                }
                Err(e) => {
                    if let Some(ref tx) = status_sender {
                        let _ = tx.send(FaceProcessingStatus::Error {
                            message: format!("Error processing {}: {}", path, e),
                        });
                    }
                }
            }
        }

        if let Some(ref tx) = status_sender {
            let _ = tx.send(FaceProcessingStatus::Completed {
                photos_processed,
                faces_found: total_faces,
            });
        }

        Ok((photos_processed, total_faces))
    }
}

impl Default for FaceProcessor {
    fn default() -> Self {
        Self::new()
    }
}
