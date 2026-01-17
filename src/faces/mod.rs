pub mod clustering;
pub mod detector;
pub mod processor;

pub use clustering::{FaceClusteringResult, cluster_faces};
pub use detector::{
    detect_faces, init_models, DetectedFace, DEFAULT_MATCH_THRESHOLD,
    embedding_similarity, embedding_distance, faces_match,
};
pub use processor::{FaceProcessor, FaceProcessingStatus};
